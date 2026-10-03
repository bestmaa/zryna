//! Verified aggregate layout authority for `DataOwnershipV1`.
//!
//! Raw graph values are untrusted claims. [`verify`] binds them to one exact [`SourceMap`], assigns
//! canonical type identities, checks every size and graph operation, and atomically constructs an
//! opaque target-specific layout snapshot.

#![forbid(unsafe_code)]

use std::cmp::Ordering;
use std::collections::BTreeSet;

use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;
use zryna_source::{SourceMap, SourceMapIdentity, Span};

/// Maximum type nodes in one layout graph.
pub const MAX_TYPE_NODES: usize = 65_536;
/// Maximum fields plus variants in one graph.
pub const MAX_MEMBERS: usize = 65_536;
/// Maximum fields or variants in one declaration.
pub const MAX_MEMBERS_PER_DECLARATION: usize = 1_024;
/// Maximum layout dependency edges.
pub const MAX_DEPENDENCY_EDGES: usize = 262_144;
/// Maximum canonical or by-value traversal depth.
pub const MAX_TRAVERSAL_DEPTH: usize = 256;
/// Maximum fixed-array length.
pub const MAX_ARRAY_LENGTH: u64 = 1_048_576;
/// Maximum universally stored object size.
pub const MAX_OBJECT_SIZE: u64 = u32::MAX as u64;
/// Maximum retained diagnostics, including the terminal diagnostic.
pub const MAX_DIAGNOSTICS: usize = 256;

const FINGERPRINT_DOMAIN: &[u8] = b"ZRYNA-AGGREGATE-LAYOUT-V1\0";
const NO_PAYLOAD_TYPE_ID: u32 = u32::MAX;

/// Untrusted type-graph claims supplied by a lowerer.
pub mod raw;

/// Separately sealed successor layouts for bounded closed generic types.
pub mod generic_v1;

/// Exact storage target sealed into a verified snapshot.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StorageTarget {
    /// Core WebAssembly 32-bit linear memory.
    Linear32V1,
    /// Audited Linux x86-64 native storage.
    LinuxX8664V1,
}

impl StorageTarget {
    const fn tag(self) -> u32 {
        match self {
            Self::Linear32V1 => 1,
            Self::LinuxX8664V1 => 2,
        }
    }

    const fn word_layout(self) -> (u64, u64) {
        match self {
            Self::Linear32V1 => (4, 4),
            Self::LinuxX8664V1 => (8, 8),
        }
    }
}

/// Target-neutral identity of one complete canonical type universe.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TypeUniverseIdentity([u8; 32]);

impl TypeUniverseIdentity {
    /// Returns the deterministic target-neutral SHA-256 identity bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Canonical dense type identity branded by its complete type universe.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TypeId {
    universe: TypeUniverseIdentity,
    index: u32,
}

impl TypeId {
    /// Returns the canonical zero-based value.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
    /// Returns the target-neutral universe that issued this identity.
    #[must_use]
    pub const fn universe_identity(self) -> TypeUniverseIdentity {
        self.universe
    }
}

/// Verified stored type category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeCategory {
    /// Stored Boolean.
    Bool,
    /// Stored signed integer.
    I32,
    /// Nominal struct.
    Struct,
    /// Nominal enum.
    Enum,
    /// Fixed array.
    FixedArray,
    /// Owned string.
    String,
    /// Owned vector.
    Vec,
    /// Immutable shared handle.
    Shared,
    /// Weak shared handle.
    Weak,
}

/// Verified source-ordered struct field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedField {
    ordinal: u32,
    ty: TypeId,
    offset: u64,
}

impl VerifiedField {
    /// Returns the source declaration ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }
    /// Returns the canonical field type.
    #[must_use]
    pub const fn ty(self) -> TypeId {
        self.ty
    }
    /// Returns the exact byte offset.
    #[must_use]
    pub const fn offset(self) -> u64 {
        self.offset
    }
}

/// Verified source-ordered enum variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedVariant {
    ordinal: u32,
    payload: Option<TypeId>,
}

impl VerifiedVariant {
    /// Returns the source ordinal and exact discriminant.
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }
    /// Returns the canonical payload type, when present.
    #[must_use]
    pub const fn payload(self) -> Option<TypeId> {
        self.payload
    }
}

#[derive(Clone, Debug)]
enum VerifiedKind {
    Bool,
    I32,
    String,
    Struct {
        module: u32,
        declaration: u32,
        fields: Vec<VerifiedField>,
    },
    Enum {
        module: u32,
        declaration: u32,
        variants: Vec<VerifiedVariant>,
        payload_offset: u64,
        payload_size: u64,
    },
    FixedArray {
        element: TypeId,
        length: u64,
        stride: u64,
    },
    Vec {
        element: TypeId,
    },
    Shared {
        payload: TypeId,
    },
    Weak {
        payload: TypeId,
    },
}

/// Immutable view of one verified target layout.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedType<'a> {
    record: &'a LayoutRecord,
}

impl<'a> VerifiedType<'a> {
    /// Returns the canonical type identity.
    #[must_use]
    pub const fn id(self) -> TypeId {
        self.record.id
    }
    /// Returns the stored category.
    #[must_use]
    pub const fn category(self) -> TypeCategory {
        self.record.category()
    }
    /// Returns the exact stored size in bytes.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.record.size
    }
    /// Returns the exact stored alignment in bytes.
    #[must_use]
    pub const fn alignment(self) -> u64 {
        self.record.alignment
    }
    /// Returns verified struct fields or an empty slice for another category.
    #[must_use]
    pub fn fields(self) -> &'a [VerifiedField] {
        match &self.record.kind {
            VerifiedKind::Struct { fields, .. } => fields,
            _ => &[],
        }
    }
    /// Returns verified enum variants or an empty slice for another category.
    #[must_use]
    pub fn variants(self) -> &'a [VerifiedVariant] {
        match &self.record.kind {
            VerifiedKind::Enum { variants, .. } => variants,
            _ => &[],
        }
    }
    /// Returns the fixed-array stride, when applicable.
    #[must_use]
    pub const fn array_stride(self) -> Option<u64> {
        match self.record.kind {
            VerifiedKind::FixedArray { stride, .. } => Some(stride),
            _ => None,
        }
    }
    /// Returns the authenticated nominal `(module, declaration)` identity.
    #[must_use]
    pub const fn nominal_identity(self) -> Option<(u32, u32)> {
        match self.record.kind {
            VerifiedKind::Struct { module, declaration, .. }
            | VerifiedKind::Enum { module, declaration, .. } => Some((module, declaration)),
            _ => None,
        }
    }
    /// Returns the fixed-array length, when applicable.
    #[must_use]
    pub const fn array_length(self) -> Option<u64> {
        match self.record.kind {
            VerifiedKind::FixedArray { length, .. } => Some(length),
            _ => None,
        }
    }
    /// Returns the enum payload offset and total payload area.
    #[must_use]
    pub const fn enum_payload_layout(self) -> Option<(u64, u64)> {
        match self.record.kind {
            VerifiedKind::Enum { payload_offset, payload_size, .. } => {
                Some((payload_offset, payload_size))
            }
            _ => None,
        }
    }
    /// Returns the referenced element or payload type for a stored container handle.
    #[must_use]
    pub const fn referenced_type(self) -> Option<TypeId> {
        match self.record.kind {
            VerifiedKind::FixedArray { element, .. } | VerifiedKind::Vec { element } => {
                Some(element)
            }
            VerifiedKind::Shared { payload } | VerifiedKind::Weak { payload } => Some(payload),
            _ => None,
        }
    }
    /// Returns the exact sealed drop metadata kind.
    #[must_use]
    pub const fn drop_kind(self) -> u32 {
        self.record.drop_kind
    }
    /// Returns the exact sealed runtime metadata kind.
    #[must_use]
    pub const fn runtime_kind(self) -> u32 {
        self.record.runtime_kind
    }
}

#[derive(Clone, Debug)]
struct LayoutRecord {
    id: TypeId,
    kind: VerifiedKind,
    size: u64,
    alignment: u64,
    drop_kind: u32,
    runtime_kind: u32,
}

impl LayoutRecord {
    const fn category(&self) -> TypeCategory {
        match self.kind {
            VerifiedKind::Bool => TypeCategory::Bool,
            VerifiedKind::I32 => TypeCategory::I32,
            VerifiedKind::String => TypeCategory::String,
            VerifiedKind::Struct { .. } => TypeCategory::Struct,
            VerifiedKind::Enum { .. } => TypeCategory::Enum,
            VerifiedKind::FixedArray { .. } => TypeCategory::FixedArray,
            VerifiedKind::Vec { .. } => TypeCategory::Vec,
            VerifiedKind::Shared { .. } => TypeCategory::Shared,
            VerifiedKind::Weak { .. } => TypeCategory::Weak,
        }
    }
}

/// Atomically verified target layout snapshot.
///
/// Raw graph recovery and direct construction are intentionally unavailable.
///
/// ```compile_fail
/// let _ = zryna_layout::VerifiedLayouts { records: Vec::new() };
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedLayouts {
    source_map: SourceMapIdentity,
    universe: TypeUniverseIdentity,
    target: StorageTarget,
    records: Vec<LayoutRecord>,
    fingerprint: [u8; 32],
    #[cfg(test)]
    canonical_bytes: Vec<u8>,
}

impl VerifiedLayouts {
    /// Returns the source-map authority to which this snapshot is bound.
    #[must_use]
    pub const fn source_map_identity(&self) -> SourceMapIdentity {
        self.source_map
    }
    /// Returns the target-neutral identity shared by both admitted storage targets.
    #[must_use]
    pub const fn universe_identity(&self) -> TypeUniverseIdentity {
        self.universe
    }
    /// Returns the exact sealed storage target.
    #[must_use]
    pub const fn target(&self) -> StorageTarget {
        self.target
    }
    /// Iterates records in canonical `TypeId` order.
    #[must_use]
    pub fn types(&self) -> impl ExactSizeIterator<Item = VerifiedType<'_>> {
        self.records.iter().map(|record| VerifiedType { record })
    }
    /// Looks up one canonical type identity.
    #[must_use]
    pub fn type_by_id(&self, id: TypeId) -> Option<VerifiedType<'_>> {
        if id.universe != self.universe {
            return None;
        }
        usize::try_from(id.index)
            .ok()
            .and_then(|index| self.records.get(index))
            .map(|record| VerifiedType { record })
    }
    /// Returns the sealed SHA-256 fingerprint.
    #[must_use]
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
}

/// Verifies one complete raw graph and seals target-specific aggregate layouts.
///
/// # Errors
///
/// Returns deterministic bounded diagnostics. No partial verified snapshot is exposed after any
/// identity, graph, storage, arithmetic, target, fingerprint, or resource failure.
pub fn verify(
    graph: &raw::Graph,
    sources: &SourceMap,
    target: StorageTarget,
) -> Result<VerifiedLayouts, Vec<Diagnostic>> {
    let mut errors = Errors::default();
    preflight(graph, sources, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }

    let keys = canonical_keys(graph, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let Some(keys) = keys else {
        return Err(errors.finish());
    };
    let canonical = assign_type_ids(graph, &keys, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let Some(canonical) = canonical else {
        return Err(errors.finish());
    };

    reject_by_value_cycles(graph, &canonical, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let linear_records = compute_records(graph, &canonical, StorageTarget::Linear32V1, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let Some(linear_records) = linear_records else {
        return Err(errors.finish());
    };
    let linux_records =
        compute_records(graph, &canonical, StorageTarget::LinuxX8664V1, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let Some(linux_records) = linux_records else {
        return Err(errors.finish());
    };
    let Some((linear_bytes, linear_fingerprint)) =
        seal_target(StorageTarget::Linear32V1, &linear_records, &mut errors)
    else {
        return Err(errors.finish());
    };
    let Some((linux_bytes, linux_fingerprint)) =
        seal_target(StorageTarget::LinuxX8664V1, &linux_records, &mut errors)
    else {
        return Err(errors.finish());
    };
    let (records, canonical_bytes, fingerprint) = match target {
        StorageTarget::Linear32V1 => (linear_records, linear_bytes, linear_fingerprint),
        StorageTarget::LinuxX8664V1 => (linux_records, linux_bytes, linux_fingerprint),
    };
    #[cfg(not(test))]
    let _ = canonical_bytes;
    Ok(VerifiedLayouts {
        source_map: sources.identity(),
        universe: canonical.universe,
        target,
        records,
        fingerprint,
        #[cfg(test)]
        canonical_bytes,
    })
}

fn seal_target(
    target: StorageTarget,
    records: &[LayoutRecord],
    errors: &mut Errors,
) -> Option<(Vec<u8>, [u8; 32])> {
    let canonical_bytes = encode_document(target, records, errors)?;
    let fingerprint: [u8; 32] = Sha256::digest(&canonical_bytes).into();
    if let Err(diagnostic) = audit_document(&canonical_bytes, target, records, &fingerprint) {
        errors.push(diagnostic);
        return None;
    }
    Some((canonical_bytes, fingerprint))
}

fn preflight(graph: &raw::Graph, sources: &SourceMap, errors: &mut Errors) {
    if graph.types.len() > MAX_TYPE_NODES {
        errors.limit("type node count", MAX_TYPE_NODES);
        return;
    }
    if graph.modules.len() != sources.len() {
        errors.push(global(
            "ZRYNA-L3001",
            "module inventory does not match the final source map",
            "provide every final module exactly once in canonical source order",
        ));
    }
    for (index, module) in graph.modules.iter().enumerate() {
        let Ok(expected) = u32::try_from(index) else {
            errors.limit("module identity", u32::MAX as usize);
            return;
        };
        if module.id != raw::ModuleId(expected)
            || sources.verify_file_id(expected).ok() != Some(module.source_file)
        {
            errors.push(global(
                "ZRYNA-L3001",
                format!("module #{index} has a noncanonical or foreign identity"),
                "bind dense modules to FileIds from the exact final SourceMap",
            ));
        }
    }
    let mut members = 0_usize;
    let mut edges = 0_usize;
    for (index, node) in graph.types.iter().enumerate() {
        let Ok(expected) = u32::try_from(index) else {
            errors.limit("type identity", u32::MAX as usize);
            return;
        };
        if node.id != raw::NodeId(expected) {
            errors.push(at(
                node.span,
                "ZRYNA-L3001",
                format!("type node #{index} has a duplicate or missing dense identity"),
                "provide graph-local node IDs exactly once in dense order",
            ));
        }
        let (local_members, local_edges) = member_and_edge_count(&node.kind);
        if local_members > MAX_MEMBERS_PER_DECLARATION {
            errors.limit_at(node.span, "declaration member count", MAX_MEMBERS_PER_DECLARATION);
            return;
        }
        let Some(next_members) = checked_budget_total(members, local_members, MAX_MEMBERS) else {
            errors.limit("graph member count", MAX_MEMBERS);
            return;
        };
        members = next_members;
        let Some(next_edges) = checked_budget_total(edges, local_edges, MAX_DEPENDENCY_EDGES)
        else {
            errors.limit("layout dependency edge count", MAX_DEPENDENCY_EDGES);
            return;
        };
        edges = next_edges;
        verify_node_shape(graph, node, errors);
        verify_node_source(graph, node, sources, errors);
        if errors.exhausted() {
            return;
        }
    }
    let primitive_counts = [
        graph.types.iter().filter(|node| matches!(node.kind, raw::TypeKind::Bool)).count(),
        graph.types.iter().filter(|node| matches!(node.kind, raw::TypeKind::I32)).count(),
        graph.types.iter().filter(|node| matches!(node.kind, raw::TypeKind::String)).count(),
    ];
    if primitive_counts != [1, 1, 1] {
        errors.push(global(
            "ZRYNA-L3001",
            "the type universe must contain exactly one bool, i32, and String node",
            "provide the complete canonical primitive universe exactly once",
        ));
    }
    verify_nominal_inventory(graph, errors);
    verify_reachability(graph, errors);
}

fn verify_node_source(
    graph: &raw::Graph,
    node: &raw::TypeNode,
    sources: &SourceMap,
    errors: &mut Errors,
) {
    match &node.kind {
        raw::TypeKind::Struct { module, .. } | raw::TypeKind::Enum { module, .. } => {
            let Some(span) = node.span else {
                errors.push(global(
                    "ZRYNA-L3001",
                    "nominal type is missing its authoritative declaration span",
                    "retain the exact declaration span from the final SourceMap",
                ));
                return;
            };
            let expected_file = usize::try_from(module.0)
                .ok()
                .and_then(|index| graph.modules.get(index))
                .map(|module| module.source_file);
            if sources.resolve(span).is_err() {
                errors.push(global(
                    "ZRYNA-L3001",
                    "nominal declaration span belongs to a foreign source authority",
                    "use a declaration span issued by the exact final SourceMap",
                ));
            } else if Some(span.file()) != expected_file {
                errors.push(at(
                    Some(span),
                    "ZRYNA-L3001",
                    "nominal declaration span belongs to another module",
                    "use a declaration span from the containing module",
                ));
            }
        }
        _ => {}
    }
}

fn verify_reachability(graph: &raw::Graph, errors: &mut Errors) {
    let valid_ref = |id: raw::NodeId| {
        usize::try_from(id.0)
            .ok()
            .is_some_and(|index| graph.types.get(index).is_some_and(|node| node.id == id))
    };
    if graph.program_roots.iter().any(|root| !valid_ref(*root)) {
        errors.push(global(
            "ZRYNA-L3003",
            "program roots contain an unknown type reference",
            "provide only graph-local stored program types",
        ));
        return;
    }
    let mut reachable = vec![false; graph.types.len()];
    let mut pending = graph
        .types
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                raw::TypeKind::Bool
                    | raw::TypeKind::I32
                    | raw::TypeKind::String
                    | raw::TypeKind::Struct { .. }
                    | raw::TypeKind::Enum { .. }
            )
        })
        .map(|node| node.id)
        .chain(graph.program_roots.iter().copied())
        .collect::<Vec<_>>();
    while let Some(id) = pending.pop() {
        let Ok(index) = usize::try_from(id.0) else {
            continue;
        };
        if reachable.get(index).copied().unwrap_or(true) {
            continue;
        }
        reachable[index] = true;
        pending.extend(all_children(&graph.types[index].kind));
    }
    if let Some(index) = reachable.iter().position(|value| !value) {
        errors.push(at(
            graph.types[index].span,
            "ZRYNA-L3003",
            format!("type node #{index} is not reachable from a declaration or program root"),
            "remove the orphan claim or retain an authenticated stored program root",
        ));
    }
}

fn all_children(kind: &raw::TypeKind) -> Vec<raw::NodeId> {
    match kind {
        raw::TypeKind::Struct { fields, .. } => fields.iter().map(|field| field.ty).collect(),
        raw::TypeKind::Enum { variants, .. } => {
            variants.iter().filter_map(|variant| variant.payload).collect()
        }
        raw::TypeKind::FixedArray { element, .. } | raw::TypeKind::Vec { element } => {
            vec![*element]
        }
        raw::TypeKind::Shared { payload } | raw::TypeKind::Weak { payload } => vec![*payload],
        raw::TypeKind::Borrow { referent } => vec![*referent],
        raw::TypeKind::Bool | raw::TypeKind::I32 | raw::TypeKind::String => Vec::new(),
    }
}

fn member_and_edge_count(kind: &raw::TypeKind) -> (usize, usize) {
    match kind {
        raw::TypeKind::Struct { fields, .. } => (fields.len(), fields.len()),
        raw::TypeKind::Enum { variants, .. } => {
            (variants.len(), variants.iter().filter(|variant| variant.payload.is_some()).count())
        }
        raw::TypeKind::FixedArray { .. }
        | raw::TypeKind::Vec { .. }
        | raw::TypeKind::Shared { .. }
        | raw::TypeKind::Weak { .. }
        | raw::TypeKind::Borrow { .. } => (0, 1),
        raw::TypeKind::Bool | raw::TypeKind::I32 | raw::TypeKind::String => (0, 0),
    }
}

fn verify_node_shape(graph: &raw::Graph, node: &raw::TypeNode, errors: &mut Errors) {
    let valid_ref = |id: raw::NodeId| {
        usize::try_from(id.0)
            .ok()
            .is_some_and(|index| graph.types.get(index).is_some_and(|candidate| candidate.id == id))
    };
    match &node.kind {
        raw::TypeKind::Struct { module, declaration, fields } => {
            verify_nominal_ref(graph, node.span, *module, *declaration, errors);
            if fields.is_empty() {
                errors.push(at(
                    node.span,
                    "ZRYNA-L3003",
                    "a stored struct must contain at least one field",
                    "add a field or remove the declaration",
                ));
            }
            for (index, field) in fields.iter().enumerate() {
                if field.ordinal != u32::try_from(index).unwrap_or(u32::MAX) || !valid_ref(field.ty)
                {
                    errors.push(at(
                        node.span,
                        "ZRYNA-L3003",
                        "struct fields have a noncanonical ordinal or unknown type",
                        "use dense source ordinals and graph-local type references",
                    ));
                }
            }
        }
        raw::TypeKind::Enum { module, declaration, variants } => {
            verify_nominal_ref(graph, node.span, *module, *declaration, errors);
            if variants.is_empty() {
                errors.push(at(
                    node.span,
                    "ZRYNA-L3003",
                    "a stored enum must contain at least one variant",
                    "add a variant or remove the declaration",
                ));
            }
            for (index, variant) in variants.iter().enumerate() {
                if variant.ordinal != u32::try_from(index).unwrap_or(u32::MAX)
                    || variant.payload.is_some_and(|payload| !valid_ref(payload))
                {
                    errors.push(at(
                        node.span,
                        "ZRYNA-L3003",
                        "enum variants have a noncanonical ordinal or unknown payload type",
                        "use dense source ordinals and graph-local payload references",
                    ));
                }
            }
        }
        raw::TypeKind::FixedArray { element, length } => {
            if !valid_ref(*element) || *length > MAX_ARRAY_LENGTH {
                errors.push(at(
                    node.span,
                    "ZRYNA-L3003",
                    "fixed array has an unknown element type or excessive length",
                    format!("use a known element and a length no greater than {MAX_ARRAY_LENGTH}"),
                ));
            }
        }
        raw::TypeKind::Vec { element }
        | raw::TypeKind::Shared { payload: element }
        | raw::TypeKind::Weak { payload: element }
        | raw::TypeKind::Borrow { referent: element } => {
            if !valid_ref(*element) {
                errors.push(at(
                    node.span,
                    "ZRYNA-L3003",
                    "container references an unknown type",
                    "use a graph-local type reference",
                ));
            }
        }
        raw::TypeKind::Bool | raw::TypeKind::I32 | raw::TypeKind::String => {
            if node.span.is_some() {
                errors.push(at(
                    node.span,
                    "ZRYNA-L3003",
                    "primitive nodes cannot claim a declaration span",
                    "remove the non-nominal span claim",
                ));
            }
        }
    }
}

fn verify_nominal_ref(
    graph: &raw::Graph,
    span: Option<Span>,
    module: raw::ModuleId,
    declaration: u32,
    errors: &mut Errors,
) {
    let valid =
        usize::try_from(module.0).ok().and_then(|index| graph.modules.get(index)).is_some_and(
            |candidate| candidate.id == module && declaration < candidate.data_declarations,
        );
    if !valid {
        errors.push(at(
            span,
            "ZRYNA-L3001",
            "nominal type has an unknown module or declaration identity",
            "bind nominal identities to the authenticated module inventory",
        ));
    }
}

fn verify_nominal_inventory(graph: &raw::Graph, errors: &mut Errors) {
    let mut seen = BTreeSet::new();
    for node in &graph.types {
        let identity = match node.kind {
            raw::TypeKind::Struct { module, declaration, .. }
            | raw::TypeKind::Enum { module, declaration, .. } => Some((module.0, declaration)),
            _ => None,
        };
        if let Some(identity) = identity
            && !seen.insert(identity)
        {
            errors.push(at(
                node.span,
                "ZRYNA-L3001",
                "duplicate nominal type identity",
                "provide each source declaration exactly once",
            ));
        }
    }
    let expected = graph
        .modules
        .iter()
        .map(|module| usize::try_from(module.data_declarations).unwrap_or(usize::MAX))
        .try_fold(0_usize, usize::checked_add);
    if expected != Some(seen.len()) {
        errors.push(global(
            "ZRYNA-L3001",
            "nominal declaration inventory has a missing identity",
            "provide every source-ordered data declaration exactly once",
        ));
    }
}

#[derive(Clone, Copy)]
struct CanonicalKey {
    prefix: [u8; 9],
    prefix_len: u8,
    child: Option<usize>,
    encoded_len: usize,
}

impl CanonicalKey {
    fn new(prefix: &[u8], child: Option<usize>, child_len: usize) -> Option<Self> {
        let mut stored = [0_u8; 9];
        stored.get_mut(..prefix.len())?.copy_from_slice(prefix);
        let encoded_len = prefix.len().checked_add(if child.is_some() {
            4_usize.checked_add(child_len)?
        } else {
            0
        })?;
        Some(Self {
            prefix: stored,
            prefix_len: u8::try_from(prefix.len()).ok()?,
            child,
            encoded_len,
        })
    }
}

struct KeyBytes<'a> {
    keys: &'a [CanonicalKey],
    current: Option<usize>,
    offset: usize,
}

impl Iterator for KeyBytes<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let index = self.current?;
            let key = self.keys[index];
            let prefix_len = usize::from(key.prefix_len);
            if self.offset < prefix_len {
                let byte = key.prefix[self.offset];
                self.offset += 1;
                return Some(byte);
            }
            let child = key.child?;
            if self.offset < prefix_len + 4 {
                let lane = u32::try_from(self.keys[child].encoded_len).ok()?.to_le_bytes();
                let byte = lane[self.offset - prefix_len];
                self.offset += 1;
                return Some(byte);
            }
            self.current = Some(child);
            self.offset = 0;
        }
    }
}

fn key_bytes(keys: &[CanonicalKey], index: usize) -> KeyBytes<'_> {
    KeyBytes { keys, current: Some(index), offset: 0 }
}

fn canonical_keys(graph: &raw::Graph, errors: &mut Errors) -> Option<Vec<CanonicalKey>> {
    let mut states = vec![0_u8; graph.types.len()];
    let mut keys = vec![None; graph.types.len()];
    for index in 0..graph.types.len() {
        build_key(graph, index, 0, &mut states, &mut keys, errors);
        if errors.exhausted() {
            return None;
        }
    }
    keys.into_iter().collect()
}

fn build_key(
    graph: &raw::Graph,
    index: usize,
    depth: usize,
    states: &mut [u8],
    keys: &mut [Option<CanonicalKey>],
    errors: &mut Errors,
) {
    if states[index] == 2 {
        return;
    }
    if depth > MAX_TRAVERSAL_DEPTH {
        errors.limit("canonical type-key depth", MAX_TRAVERSAL_DEPTH);
        return;
    }
    if states[index] == 1 {
        errors.push(at(
            graph.types[index].span,
            "ZRYNA-L3003",
            "structural container identity is recursive",
            "place recursion behind a nominal Vec, Shared, or Weak payload",
        ));
        return;
    }
    states[index] = 1;
    let node = &graph.types[index];
    let key = match node.kind {
        raw::TypeKind::Bool => CanonicalKey::new(&[0x00], None, 0),
        raw::TypeKind::I32 => CanonicalKey::new(&[0x01], None, 0),
        raw::TypeKind::String => CanonicalKey::new(&[0x02], None, 0),
        raw::TypeKind::Struct { module, declaration, .. } => {
            let mut prefix = [0_u8; 9];
            prefix[0] = 0x10;
            prefix[1..5].copy_from_slice(&module.0.to_le_bytes());
            prefix[5..9].copy_from_slice(&declaration.to_le_bytes());
            CanonicalKey::new(&prefix, None, 0)
        }
        raw::TypeKind::Enum { module, declaration, .. } => {
            let mut prefix = [0_u8; 9];
            prefix[0] = 0x11;
            prefix[1..5].copy_from_slice(&module.0.to_le_bytes());
            prefix[5..9].copy_from_slice(&declaration.to_le_bytes());
            CanonicalKey::new(&prefix, None, 0)
        }
        raw::TypeKind::FixedArray { element, length } => {
            let Ok(length) = u32::try_from(length) else {
                errors.push(at(
                    node.span,
                    "ZRYNA-L3003",
                    "fixed-array length does not fit the canonical key",
                    "use an admitted fixed-array length",
                ));
                states[index] = 2;
                return;
            };
            let mut prefix = [0_u8; 5];
            prefix[0] = 0x20;
            prefix[1..].copy_from_slice(&length.to_le_bytes());
            child_key(graph, element, depth, states, keys, errors)
                .and_then(|child| CanonicalKey::new(&prefix, Some(child), keys[child]?.encoded_len))
        }
        raw::TypeKind::Vec { element } => child_key(graph, element, depth, states, keys, errors)
            .and_then(|child| CanonicalKey::new(&[0x21], Some(child), keys[child]?.encoded_len)),
        raw::TypeKind::Shared { payload } => child_key(graph, payload, depth, states, keys, errors)
            .and_then(|child| CanonicalKey::new(&[0x22], Some(child), keys[child]?.encoded_len)),
        raw::TypeKind::Weak { payload } => child_key(graph, payload, depth, states, keys, errors)
            .and_then(|child| CanonicalKey::new(&[0x23], Some(child), keys[child]?.encoded_len)),
        raw::TypeKind::Borrow { .. } => {
            errors.push(at(
                node.span,
                "ZRYNA-L3004",
                "borrow authorities have no stored layout",
                "remove the borrow from the stored type graph",
            ));
            None
        }
    };
    states[index] = 2;
    keys[index] = key;
}

fn child_key(
    graph: &raw::Graph,
    child: raw::NodeId,
    depth: usize,
    states: &mut [u8],
    keys: &mut [Option<CanonicalKey>],
    errors: &mut Errors,
) -> Option<usize> {
    let index = usize::try_from(child.0).ok()?;
    build_key(graph, index, depth + 1, states, keys, errors);
    keys[index]?;
    Some(index)
}

fn assign_type_ids(
    graph: &raw::Graph,
    keys: &[CanonicalKey],
    errors: &mut Errors,
) -> Option<CanonicalMap> {
    let mut ordered = (0..keys.len()).collect::<Vec<_>>();
    ordered.sort_by(|left, right| key_bytes(keys, *left).cmp(key_bytes(keys, *right)));
    if ordered.windows(2).any(|pair| key_bytes(keys, pair[0]).eq(key_bytes(keys, pair[1]))) {
        errors.push(global(
            "ZRYNA-L3001",
            "duplicate canonical type key",
            "provide each nominal or structural type exactly once",
        ));
        return None;
    }
    let mut raw_to_index = vec![0_u32; keys.len()];
    let mut type_to_raw = Vec::with_capacity(keys.len());
    for (id, raw_index) in ordered.into_iter().enumerate() {
        let Ok(id) = u32::try_from(id) else {
            errors.limit("canonical TypeId", u32::MAX as usize);
            return None;
        };
        raw_to_index[raw_index] = id;
        type_to_raw.push(raw_index);
    }
    let mut canonical =
        CanonicalMap { raw_to_index, type_to_raw, universe: TypeUniverseIdentity([0; 32]) };
    canonical.universe = seal_universe_identity(graph, keys, &canonical, errors)?;
    Some(canonical)
}

struct CanonicalMap {
    raw_to_index: Vec<u32>,
    type_to_raw: Vec<usize>,
    universe: TypeUniverseIdentity,
}

impl CanonicalMap {
    fn type_id(&self, raw_index: usize) -> TypeId {
        TypeId { universe: self.universe, index: self.raw_to_index[raw_index] }
    }
}

#[allow(clippy::too_many_lines)]
fn seal_universe_identity(
    graph: &raw::Graph,
    keys: &[CanonicalKey],
    canonical: &CanonicalMap,
    errors: &mut Errors,
) -> Option<TypeUniverseIdentity> {
    let mut hash = Sha256::new();
    hash.update(b"ZRYNA-TYPE-UNIVERSE-V1\0");
    hash.update(u32::try_from(canonical.type_to_raw.len()).ok()?.to_le_bytes());
    for raw_index in &canonical.type_to_raw {
        let Ok(key_length) = u32::try_from(keys[*raw_index].encoded_len) else {
            errors.limit("canonical type-key byte length", u32::MAX as usize);
            return None;
        };
        hash.update(key_length.to_le_bytes());
        for byte in key_bytes(keys, *raw_index) {
            hash.update([byte]);
        }
        let mut record = Vec::new();
        match &graph.types[*raw_index].kind {
            raw::TypeKind::Bool => push_u32(&mut record, 1),
            raw::TypeKind::I32 => push_u32(&mut record, 2),
            raw::TypeKind::String => push_u32(&mut record, 6),
            raw::TypeKind::Struct { module, declaration, fields } => {
                push_u32(&mut record, 3);
                push_u32(&mut record, module.0);
                push_u32(&mut record, *declaration);
                push_u32(&mut record, u32::try_from(fields.len()).ok()?);
                for field in fields {
                    push_u32(&mut record, field.ordinal);
                    let child = usize::try_from(field.ty.0).ok()?;
                    push_u32(&mut record, canonical.raw_to_index[child]);
                }
            }
            raw::TypeKind::Enum { module, declaration, variants } => {
                push_u32(&mut record, 4);
                push_u32(&mut record, module.0);
                push_u32(&mut record, *declaration);
                push_u32(&mut record, u32::try_from(variants.len()).ok()?);
                for variant in variants {
                    push_u32(&mut record, variant.ordinal);
                    let payload = variant.payload.map_or(NO_PAYLOAD_TYPE_ID, |payload| {
                        canonical.raw_to_index[usize::try_from(payload.0).expect("preflight child")]
                    });
                    push_u32(&mut record, payload);
                }
            }
            raw::TypeKind::FixedArray { element, length } => {
                push_u32(&mut record, 5);
                let child = usize::try_from(element.0).ok()?;
                push_u32(&mut record, canonical.raw_to_index[child]);
                push_u64(&mut record, *length);
            }
            raw::TypeKind::Vec { element } => {
                push_u32(&mut record, 7);
                let child = usize::try_from(element.0).ok()?;
                push_u32(&mut record, canonical.raw_to_index[child]);
            }
            raw::TypeKind::Shared { payload } => {
                push_u32(&mut record, 8);
                let child = usize::try_from(payload.0).ok()?;
                push_u32(&mut record, canonical.raw_to_index[child]);
            }
            raw::TypeKind::Weak { payload } => {
                push_u32(&mut record, 9);
                let child = usize::try_from(payload.0).ok()?;
                push_u32(&mut record, canonical.raw_to_index[child]);
            }
            raw::TypeKind::Borrow { .. } => return None,
        }
        hash.update(record);
    }
    Some(TypeUniverseIdentity(hash.finalize().into()))
}

fn reject_by_value_cycles(graph: &raw::Graph, canonical: &CanonicalMap, errors: &mut Errors) {
    let adjacency = canonical_adjacency(graph, canonical);
    let finish = graph_finish_order(&adjacency);
    let mut reverse = vec![Vec::new(); adjacency.len()];
    for (source, children) in adjacency.iter().enumerate() {
        for child in children {
            reverse[*child].push(source);
        }
    }
    for edges in &mut reverse {
        edges.sort_unstable();
    }
    let mut assigned = vec![false; adjacency.len()];
    let mut cyclic_components = Vec::new();
    for root in finish.iter().rev().copied() {
        if assigned[root] {
            continue;
        }
        let mut component = Vec::new();
        let mut pending = vec![root];
        assigned[root] = true;
        while let Some(node) = pending.pop() {
            component.push(node);
            for parent in reverse[node].iter().rev().copied() {
                if !assigned[parent] {
                    assigned[parent] = true;
                    pending.push(parent);
                }
            }
        }
        component.sort_unstable();
        if component.len() > 1 || adjacency[root].binary_search(&root).is_ok() {
            cyclic_components.push(component);
        }
    }
    cyclic_components.sort();
    if let Some(component) = cyclic_components.first() {
        let cycle = deterministic_component_cycle(&adjacency, component);
        errors.push(global(
            "ZRYNA-L3002",
            format!("by-value recursive layout through canonical TypeIds {cycle:?}"),
            "place recursive ownership behind Vec, Shared, or Weak",
        ));
        return;
    }
    let mut depth = vec![1_usize; adjacency.len()];
    for node in finish {
        let child_depth = adjacency[node].iter().map(|child| depth[*child]).max().unwrap_or(0);
        depth[node] = child_depth.saturating_add(1);
        if depth[node] > MAX_TRAVERSAL_DEPTH {
            errors.limit("by-value traversal depth", MAX_TRAVERSAL_DEPTH);
            return;
        }
    }
}

fn canonical_adjacency(graph: &raw::Graph, canonical: &CanonicalMap) -> Vec<Vec<usize>> {
    canonical
        .type_to_raw
        .iter()
        .map(|raw_index| {
            let mut children = by_value_children(&graph.types[*raw_index].kind)
                .into_iter()
                .map(|child| {
                    let raw_child = usize::try_from(child.0).expect("preflight admitted child");
                    usize::try_from(canonical.raw_to_index[raw_child]).expect("TypeId fits usize")
                })
                .collect::<Vec<_>>();
            children.sort_unstable();
            children.dedup();
            children
        })
        .collect()
}

fn graph_finish_order(adjacency: &[Vec<usize>]) -> Vec<usize> {
    let mut visited = vec![false; adjacency.len()];
    let mut finish = Vec::with_capacity(adjacency.len());
    for root in 0..adjacency.len() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut stack = vec![(root, 0_usize)];
        while let Some((node, next_child)) = stack.last_mut() {
            if let Some(child) = adjacency[*node].get(*next_child).copied() {
                *next_child += 1;
                if !visited[child] {
                    visited[child] = true;
                    stack.push((child, 0));
                }
            } else {
                finish.push(*node);
                let _ = stack.pop();
            }
        }
    }
    finish
}

fn deterministic_component_cycle(adjacency: &[Vec<usize>], component: &[usize]) -> Vec<u32> {
    let start = component[0];
    if adjacency[start].binary_search(&start).is_ok() {
        return vec![u32::try_from(start).expect("TypeId fits u32")];
    }
    let members = component.iter().copied().collect::<BTreeSet<_>>();
    for first in adjacency[start].iter().copied().filter(|child| members.contains(child)) {
        let mut parent = vec![None; adjacency.len()];
        let mut visited = vec![false; adjacency.len()];
        visited[start] = true;
        visited[first] = true;
        parent[first] = Some(start);
        let mut stack = vec![(first, 0_usize)];
        while let Some((node, next_child)) = stack.last_mut() {
            if *next_child == adjacency[*node].len() {
                let _ = stack.pop();
                continue;
            }
            let child = adjacency[*node][*next_child];
            *next_child += 1;
            if !members.contains(&child) {
                continue;
            }
            if child == start {
                let mut reversed = Vec::new();
                let mut cursor = *node;
                while cursor != start {
                    reversed.push(cursor);
                    cursor = parent[cursor].expect("discovered cycle path has a parent");
                }
                reversed.reverse();
                let mut cycle = vec![u32::try_from(start).expect("TypeId fits u32")];
                cycle.extend(
                    reversed.into_iter().map(|node| u32::try_from(node).expect("TypeId fits u32")),
                );
                return cycle;
            }
            if !visited[child] {
                visited[child] = true;
                parent[child] = Some(*node);
                stack.push((child, 0));
            }
        }
    }
    vec![u32::try_from(start).expect("TypeId fits u32")]
}

fn by_value_children(kind: &raw::TypeKind) -> Vec<raw::NodeId> {
    match kind {
        raw::TypeKind::Struct { fields, .. } => fields.iter().map(|field| field.ty).collect(),
        raw::TypeKind::Enum { variants, .. } => {
            variants.iter().filter_map(|variant| variant.payload).collect()
        }
        raw::TypeKind::FixedArray { element, .. } => vec![*element],
        _ => Vec::new(),
    }
}

fn compute_records(
    graph: &raw::Graph,
    canonical: &CanonicalMap,
    target: StorageTarget,
    errors: &mut Errors,
) -> Option<Vec<LayoutRecord>> {
    let mut records = vec![None; graph.types.len()];
    let mut visiting = vec![false; graph.types.len()];
    for type_index in 0..graph.types.len() {
        compute_one(graph, canonical, type_index, target, &mut records, &mut visiting, errors);
        if errors.exhausted() {
            return None;
        }
    }
    let records = records.into_iter().collect::<Option<Vec<_>>>()?;
    for record in &records {
        if let VerifiedKind::Vec { element } = record.kind {
            let index = usize::try_from(element.index).expect("admitted TypeId fits usize");
            if records[index].size == 0 {
                errors.push(global(
                    "ZRYNA-L3003",
                    format!("Vec element TypeId {} has zero stored size", element.index),
                    "use a non-zero-sized Vec element type",
                ));
                return None;
            }
        }
    }
    Some(records)
}

#[allow(clippy::too_many_lines)]
fn compute_one(
    graph: &raw::Graph,
    canonical: &CanonicalMap,
    type_index: usize,
    target: StorageTarget,
    records: &mut [Option<LayoutRecord>],
    visiting: &mut [bool],
    errors: &mut Errors,
) {
    if records[type_index].is_some() || visiting[type_index] {
        return;
    }
    visiting[type_index] = true;
    let raw_index = canonical.type_to_raw[type_index];
    let node = &graph.types[raw_index];
    for child in by_value_children(&node.kind) {
        let child_raw = usize::try_from(child.0).expect("preflight admitted child index");
        let child_type =
            usize::try_from(canonical.raw_to_index[child_raw]).expect("admitted TypeId fits usize");
        compute_one(graph, canonical, child_type, target, records, visiting, errors);
    }
    if !errors.is_empty() {
        visiting[type_index] = false;
        return;
    }
    let id = TypeId {
        universe: canonical.universe,
        index: u32::try_from(type_index).expect("type budget fits u32"),
    };
    let record = match &node.kind {
        raw::TypeKind::Bool => LayoutRecord {
            id,
            kind: VerifiedKind::Bool,
            size: 1,
            alignment: 1,
            drop_kind: 0,
            runtime_kind: 0,
        },
        raw::TypeKind::I32 => LayoutRecord {
            id,
            kind: VerifiedKind::I32,
            size: 4,
            alignment: 4,
            drop_kind: 0,
            runtime_kind: 0,
        },
        raw::TypeKind::String => {
            let (word, alignment) = target.word_layout();
            LayoutRecord {
                id,
                kind: VerifiedKind::String,
                size: word * 3,
                alignment,
                drop_kind: 2,
                runtime_kind: 2,
            }
        }
        raw::TypeKind::Struct { module, declaration, fields } => {
            let mut cursor = 0_u64;
            let mut alignment = 1_u64;
            let mut verified = Vec::with_capacity(fields.len());
            let mut needs_drop = false;
            for field in fields {
                let child = resolved_record(field.ty, canonical, records);
                let Some(child) = child else {
                    errors.push(at(
                        node.span,
                        "ZRYNA-L3003",
                        "struct field layout was not resolved",
                        "provide an acyclic complete type graph",
                    ));
                    visiting[type_index] = false;
                    return;
                };
                let Some(offset) = checked_align_up(cursor, child.alignment) else {
                    errors.overflow(node.span, "struct field offset");
                    visiting[type_index] = false;
                    return;
                };
                let Some(next) = checked_storage_add(offset, child.size, MAX_OBJECT_SIZE) else {
                    errors.overflow(node.span, "struct size");
                    visiting[type_index] = false;
                    return;
                };
                if next > MAX_OBJECT_SIZE {
                    errors.overflow(node.span, "struct object size");
                    visiting[type_index] = false;
                    return;
                }
                cursor = next;
                alignment = alignment.max(child.alignment);
                needs_drop |= child.drop_kind != 0;
                verified.push(VerifiedField { ordinal: field.ordinal, ty: child.id, offset });
            }
            let Some(size) = checked_align_up(cursor, alignment) else {
                errors.overflow(node.span, "struct tail alignment");
                visiting[type_index] = false;
                return;
            };
            LayoutRecord {
                id,
                kind: VerifiedKind::Struct {
                    module: module.0,
                    declaration: *declaration,
                    fields: verified,
                },
                size,
                alignment,
                drop_kind: u32::from(needs_drop),
                runtime_kind: u32::from(needs_drop),
            }
        }
        raw::TypeKind::Enum { module, declaration, variants } => {
            let mut payload_alignment = 1_u64;
            let mut payload_size = 0_u64;
            let mut needs_drop = false;
            let mut verified = Vec::with_capacity(variants.len());
            for variant in variants {
                let payload = variant.payload.map(|child| {
                    let child =
                        resolved_record(child, canonical, records).expect("acyclic child computed");
                    payload_alignment = payload_alignment.max(child.alignment);
                    payload_size = payload_size.max(child.size);
                    needs_drop |= child.drop_kind != 0;
                    child.id
                });
                verified.push(VerifiedVariant { ordinal: variant.ordinal, payload });
            }
            let alignment = 4_u64.max(payload_alignment);
            let Some(payload_offset) = checked_align_up(4, payload_alignment) else {
                errors.overflow(node.span, "enum payload offset");
                visiting[type_index] = false;
                return;
            };
            let Some(end) = checked_storage_add(payload_offset, payload_size, MAX_OBJECT_SIZE)
            else {
                errors.overflow(node.span, "enum payload size");
                visiting[type_index] = false;
                return;
            };
            let Some(size) = checked_align_up(end, alignment) else {
                errors.overflow(node.span, "enum tail alignment");
                visiting[type_index] = false;
                return;
            };
            if size > MAX_OBJECT_SIZE {
                errors.overflow(node.span, "enum object size");
                visiting[type_index] = false;
                return;
            }
            LayoutRecord {
                id,
                kind: VerifiedKind::Enum {
                    module: module.0,
                    declaration: *declaration,
                    variants: verified,
                    payload_offset,
                    payload_size,
                },
                size,
                alignment,
                drop_kind: u32::from(needs_drop),
                runtime_kind: u32::from(needs_drop),
            }
        }
        raw::TypeKind::FixedArray { element, length } => {
            let child =
                resolved_record(*element, canonical, records).expect("acyclic child computed");
            let Some(stride) = checked_align_up(child.size, child.alignment) else {
                errors.overflow(node.span, "array stride");
                visiting[type_index] = false;
                return;
            };
            let Some(size) = checked_storage_mul(stride, *length, MAX_OBJECT_SIZE) else {
                errors.overflow(node.span, "array size");
                visiting[type_index] = false;
                return;
            };
            if size > MAX_OBJECT_SIZE {
                errors.overflow(node.span, "array object size");
                visiting[type_index] = false;
                return;
            }
            let needs_drop = child.drop_kind != 0;
            LayoutRecord {
                id,
                kind: VerifiedKind::FixedArray { element: child.id, length: *length, stride },
                size,
                alignment: child.alignment,
                drop_kind: u32::from(needs_drop),
                runtime_kind: u32::from(needs_drop),
            }
        }
        raw::TypeKind::Vec { element } => {
            let child_id = canonical.type_id(usize::try_from(element.0).expect("preflight child"));
            let child_type = usize::try_from(child_id.index).expect("TypeId fits usize");
            if records[child_type].as_ref().is_some_and(|child| child.size == 0) {
                errors.push(at(
                    node.span,
                    "ZRYNA-L3003",
                    "Vec does not admit a zero-sized element type",
                    "use a non-zero-sized element type",
                ));
                visiting[type_index] = false;
                return;
            }
            let (word, alignment) = target.word_layout();
            LayoutRecord {
                id,
                kind: VerifiedKind::Vec { element: child_id },
                size: word * 3,
                alignment,
                drop_kind: 3,
                runtime_kind: 3,
            }
        }
        raw::TypeKind::Shared { payload } => {
            let (word, alignment) = target.word_layout();
            LayoutRecord {
                id,
                kind: VerifiedKind::Shared {
                    payload: canonical
                        .type_id(usize::try_from(payload.0).expect("preflight child")),
                },
                size: word,
                alignment,
                drop_kind: 4,
                runtime_kind: 4,
            }
        }
        raw::TypeKind::Weak { payload } => {
            let (word, alignment) = target.word_layout();
            LayoutRecord {
                id,
                kind: VerifiedKind::Weak {
                    payload: canonical
                        .type_id(usize::try_from(payload.0).expect("preflight child")),
                },
                size: word,
                alignment,
                drop_kind: 5,
                runtime_kind: 5,
            }
        }
        raw::TypeKind::Borrow { .. } => {
            visiting[type_index] = false;
            return;
        }
    };
    if record.alignment == 0
        || !record.alignment.is_power_of_two()
        || record.alignment > 8
        || record.size > MAX_OBJECT_SIZE
    {
        errors.overflow(node.span, "stored layout limit");
    } else {
        records[type_index] = Some(record);
    }
    visiting[type_index] = false;
}

fn resolved_record<'a>(
    id: raw::NodeId,
    canonical: &CanonicalMap,
    records: &'a [Option<LayoutRecord>],
) -> Option<&'a LayoutRecord> {
    let raw_index = usize::try_from(id.0).ok()?;
    let type_index = usize::try_from(canonical.raw_to_index[raw_index]).ok()?;
    records.get(type_index)?.as_ref()
}

fn encode_document(
    target: StorageTarget,
    records: &[LayoutRecord],
    errors: &mut Errors,
) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(FINGERPRINT_DOMAIN);
    push_u32(&mut bytes, target.tag());
    let Ok(count) = u32::try_from(records.len()) else {
        errors.limit("sealed record count", u32::MAX as usize);
        return None;
    };
    push_u32(&mut bytes, count);
    for record in records {
        let payload = encode_record(record);
        let Ok(length) = u32::try_from(payload.len()) else {
            errors.limit("sealed record byte length", u32::MAX as usize);
            return None;
        };
        push_u32(&mut bytes, length);
        bytes.extend_from_slice(&payload);
    }
    Some(bytes)
}

fn encode_record(record: &LayoutRecord) -> Vec<u8> {
    let mut payload = Vec::new();
    push_u32(&mut payload, record_tag(&record.kind));
    push_u32(&mut payload, record.id.index);
    push_u32(&mut payload, record.drop_kind);
    push_u32(&mut payload, record.runtime_kind);
    push_u64(&mut payload, record.size);
    push_u64(&mut payload, record.alignment);
    match &record.kind {
        VerifiedKind::Bool | VerifiedKind::I32 | VerifiedKind::String => {}
        VerifiedKind::Struct { module, declaration, fields } => {
            push_u32(&mut payload, *module);
            push_u32(&mut payload, *declaration);
            push_u32(&mut payload, u32::try_from(fields.len()).expect("member budget fits u32"));
            for field in fields {
                push_u32(&mut payload, field.ordinal);
                push_u32(&mut payload, field.ty.index);
                push_u64(&mut payload, field.offset);
            }
        }
        VerifiedKind::Enum { module, declaration, variants, payload_offset, payload_size } => {
            push_u32(&mut payload, *module);
            push_u32(&mut payload, *declaration);
            push_u32(&mut payload, u32::try_from(variants.len()).expect("member budget fits u32"));
            push_u64(&mut payload, *payload_offset);
            push_u64(&mut payload, *payload_size);
            for variant in variants {
                push_u32(&mut payload, variant.ordinal);
                push_u32(&mut payload, variant.payload.map_or(NO_PAYLOAD_TYPE_ID, |id| id.index));
            }
        }
        VerifiedKind::FixedArray { element, length, stride } => {
            push_u32(&mut payload, element.index);
            push_u64(&mut payload, *length);
            push_u64(&mut payload, *stride);
        }
        VerifiedKind::Vec { element } => push_u32(&mut payload, element.index),
        VerifiedKind::Shared { payload: child } | VerifiedKind::Weak { payload: child } => {
            push_u32(&mut payload, child.index);
        }
    }
    payload
}

#[allow(clippy::too_many_lines)]
fn audit_document(
    bytes: &[u8],
    target: StorageTarget,
    expected_records: &[LayoutRecord],
    expected_fingerprint: &[u8; 32],
) -> Result<(), Diagnostic> {
    let failure = || {
        global(
            "ZRYNA-L3006",
            "sealed layout document, target, or fingerprint is inconsistent",
            "discard the snapshot and rebuild it through the aggregate layout authority",
        )
    };
    let mut cursor = 0_usize;
    if !take_exact(bytes, &mut cursor, FINGERPRINT_DOMAIN) {
        return Err(failure());
    }
    if read_u32(bytes, &mut cursor) != Some(target.tag()) {
        return Err(failure());
    }
    let Some(record_count) = read_u32(bytes, &mut cursor) else {
        return Err(failure());
    };
    if usize::try_from(record_count).ok() != Some(expected_records.len()) {
        return Err(failure());
    }
    for expected_id in 0..record_count {
        let expected = &expected_records[usize::try_from(expected_id).map_err(|_| failure())?];
        let Some(payload_length) = read_u32(bytes, &mut cursor) else {
            return Err(failure());
        };
        if payload_length < 32 {
            return Err(failure());
        }
        let Ok(payload_length) = usize::try_from(payload_length) else {
            return Err(failure());
        };
        let Some(end) = cursor.checked_add(payload_length) else {
            return Err(failure());
        };
        if end > bytes.len() {
            return Err(failure());
        }
        let Some(tag) = read_u32(bytes, &mut cursor) else {
            return Err(failure());
        };
        if tag != record_tag(&expected.kind) || read_u32(bytes, &mut cursor) != Some(expected_id) {
            return Err(failure());
        }
        let Some(drop_kind) = read_u32(bytes, &mut cursor) else {
            return Err(failure());
        };
        let Some(runtime_kind) = read_u32(bytes, &mut cursor) else {
            return Err(failure());
        };
        if drop_kind != expected.drop_kind || runtime_kind != expected.runtime_kind {
            return Err(failure());
        }
        let Some(size) = read_u64(bytes, &mut cursor) else {
            return Err(failure());
        };
        let Some(alignment) = read_u64(bytes, &mut cursor) else {
            return Err(failure());
        };
        if size != expected.size || alignment != expected.alignment {
            return Err(failure());
        }
        if !audit_record_payload(bytes, &mut cursor, end, expected) || cursor != end {
            return Err(failure());
        }
    }
    if cursor != bytes.len() || Sha256::digest(bytes).as_slice() != expected_fingerprint {
        return Err(failure());
    }
    Ok(())
}

fn audit_record_payload(
    bytes: &[u8],
    cursor: &mut usize,
    end: usize,
    expected: &LayoutRecord,
) -> bool {
    match &expected.kind {
        VerifiedKind::Bool | VerifiedKind::I32 | VerifiedKind::String => *cursor == end,
        VerifiedKind::Struct { module, declaration, fields } => {
            if read_u32(bytes, cursor) != Some(*module)
                || read_u32(bytes, cursor) != Some(*declaration)
                || read_u32(bytes, cursor) != u32::try_from(fields.len()).ok()
            {
                return false;
            }
            fields.iter().all(|field| {
                read_u32(bytes, cursor) == Some(field.ordinal)
                    && read_u32(bytes, cursor) == Some(field.ty.index)
                    && read_u64(bytes, cursor) == Some(field.offset)
            })
        }
        VerifiedKind::Enum { module, declaration, variants, payload_offset, payload_size } => {
            if read_u32(bytes, cursor) != Some(*module)
                || read_u32(bytes, cursor) != Some(*declaration)
                || read_u32(bytes, cursor) != u32::try_from(variants.len()).ok()
                || read_u64(bytes, cursor) != Some(*payload_offset)
                || read_u64(bytes, cursor) != Some(*payload_size)
            {
                return false;
            }
            variants.iter().all(|variant| {
                read_u32(bytes, cursor) == Some(variant.ordinal)
                    && read_u32(bytes, cursor)
                        == Some(variant.payload.map_or(NO_PAYLOAD_TYPE_ID, |id| id.index))
            })
        }
        VerifiedKind::FixedArray { element, length, stride } => {
            read_u32(bytes, cursor) == Some(element.index)
                && read_u64(bytes, cursor) == Some(*length)
                && read_u64(bytes, cursor) == Some(*stride)
        }
        VerifiedKind::Vec { element } => read_u32(bytes, cursor) == Some(element.index),
        VerifiedKind::Shared { payload } | VerifiedKind::Weak { payload } => {
            read_u32(bytes, cursor) == Some(payload.index)
        }
    }
}

fn take_exact(bytes: &[u8], cursor: &mut usize, expected: &[u8]) -> bool {
    let Some(end) = cursor.checked_add(expected.len()) else {
        return false;
    };
    if bytes.get(*cursor..end) != Some(expected) {
        return false;
    }
    *cursor = end;
    true
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Option<u32> {
    let end = cursor.checked_add(4)?;
    let lane: [u8; 4] = bytes.get(*cursor..end)?.try_into().ok()?;
    *cursor = end;
    Some(u32::from_le_bytes(lane))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    let end = cursor.checked_add(8)?;
    let lane: [u8; 8] = bytes.get(*cursor..end)?.try_into().ok()?;
    *cursor = end;
    Some(u64::from_le_bytes(lane))
}

const fn record_tag(kind: &VerifiedKind) -> u32 {
    match kind {
        VerifiedKind::Bool => 1,
        VerifiedKind::I32 => 2,
        VerifiedKind::Struct { .. } => 3,
        VerifiedKind::Enum { .. } => 4,
        VerifiedKind::FixedArray { .. } => 5,
        VerifiedKind::String => 6,
        VerifiedKind::Vec { .. } => 7,
        VerifiedKind::Shared { .. } => 8,
        VerifiedKind::Weak { .. } => 9,
    }
}

fn checked_align_up(value: u64, alignment: u64) -> Option<u64> {
    checked_align_up_with_limit(value, alignment, u64::MAX)
}

fn checked_align_up_with_limit(value: u64, alignment: u64, limit: u64) -> Option<u64> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return None;
    }
    let remainder = value % alignment;
    let aligned =
        if remainder == 0 { Some(value) } else { value.checked_add(alignment - remainder) }?;
    (aligned <= limit).then_some(aligned)
}

fn checked_storage_add(left: u64, right: u64, limit: u64) -> Option<u64> {
    left.checked_add(right).filter(|value| *value <= limit)
}

fn checked_storage_mul(left: u64, right: u64, limit: u64) -> Option<u64> {
    left.checked_mul(right).filter(|value| *value <= limit)
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[derive(Default)]
struct Errors {
    diagnostics: Vec<RetainedDiagnostic>,
    fatal: bool,
    truncated: bool,
}

struct RetainedDiagnostic {
    diagnostic: Diagnostic,
    numeric: Vec<u64>,
}

impl RetainedDiagnostic {
    fn new(diagnostic: Diagnostic) -> Self {
        let numeric = diagnostic_numbers(&diagnostic);
        Self { diagnostic, numeric }
    }
}

impl Errors {
    fn push(&mut self, diagnostic: Diagnostic) {
        if self.fatal {
            return;
        }
        let diagnostic = RetainedDiagnostic::new(diagnostic);
        let retained = MAX_DIAGNOSTICS - 1;
        if self.diagnostics.len() < retained {
            self.diagnostics.push(diagnostic);
        } else {
            self.truncated = true;
            let greatest = self
                .diagnostics
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| compare_retained(left, right))
                .map(|(index, _)| index)
                .expect("retained diagnostic set is nonempty");
            if compare_retained(&diagnostic, &self.diagnostics[greatest]) == Ordering::Less {
                self.diagnostics[greatest] = diagnostic;
            }
        }
    }
    fn limit(&mut self, label: &str, limit: usize) {
        self.push(global(
            "ZRYNA-L3201",
            format!("{label} exceeds the limit of {limit}"),
            "reduce the graph before layout verification",
        ));
        self.fatal = true;
    }
    fn limit_at(&mut self, span: Option<Span>, label: &str, limit: usize) {
        self.push(at(
            span,
            "ZRYNA-L3201",
            format!("{label} exceeds the limit of {limit}"),
            "reduce the declaration before layout verification",
        ));
        self.fatal = true;
    }
    fn overflow(&mut self, span: Option<Span>, label: &str) {
        self.push(at(
            span,
            "ZRYNA-L3005",
            format!("checked {label} is not representable by aggregate layout v1"),
            "reduce the stored type before verification",
        ));
    }
    const fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
    const fn exhausted(&self) -> bool {
        self.fatal
    }
    fn finish(mut self) -> Vec<Diagnostic> {
        if self.diagnostics.is_empty() {
            self.diagnostics.push(RetainedDiagnostic::new(global(
                "ZRYNA-L3003",
                "layout verification failed without a retained detail",
                "report the smallest reproducible graph",
            )));
        }
        if self.truncated {
            self.diagnostics.push(RetainedDiagnostic::new(global(
                "ZRYNA-L3201",
                "layout diagnostic budget exhausted",
                "fix the earliest reported layout errors and verify again",
            )));
        }
        self.diagnostics.sort_by(compare_retained);
        self.diagnostics.into_iter().map(|entry| entry.diagnostic).collect()
    }
}

fn compare_retained(left: &RetainedDiagnostic, right: &RetainedDiagnostic) -> Ordering {
    let left_terminal = left.diagnostic.code() == "ZRYNA-L3201";
    let right_terminal = right.diagnostic.code() == "ZRYNA-L3201";
    left_terminal
        .cmp(&right_terminal)
        .then_with(|| {
            diagnostic_location(&left.diagnostic).cmp(&diagnostic_location(&right.diagnostic))
        })
        .then_with(|| left.diagnostic.code().cmp(right.diagnostic.code()))
        .then_with(|| left.numeric.cmp(&right.numeric))
        .then_with(|| left.diagnostic.message().cmp(right.diagnostic.message()))
        .then_with(|| left.diagnostic.guidance().cmp(right.diagnostic.guidance()))
}

fn diagnostic_numbers(diagnostic: &Diagnostic) -> Vec<u64> {
    diagnostic
        .message()
        .split(|character: char| !character.is_ascii_digit())
        .filter(|digits| !digits.is_empty())
        .filter_map(|digits| digits.parse().ok())
        .collect()
}

fn diagnostic_location(diagnostic: &Diagnostic) -> (u8, u32, u32, u32, &str) {
    if let Some(span) = diagnostic.primary_span() {
        (0, span.file().index(), span.start(), span.end(), "")
    } else if let Some(path) = diagnostic.path() {
        (1, 0, 0, 0, path)
    } else {
        (2, 0, 0, 0, "")
    }
}

fn checked_budget_total(current: usize, extra: usize, limit: usize) -> Option<usize> {
    current.checked_add(extra).filter(|value| *value <= limit)
}

fn global(code: &str, message: impl Into<String>, guidance: impl Into<String>) -> Diagnostic {
    Diagnostic::error(code, None, message, guidance)
}
fn at(
    span: Option<Span>,
    code: &str,
    message: impl Into<String>,
    guidance: impl Into<String>,
) -> Diagnostic {
    match span {
        Some(span) => Diagnostic::error_at(code, span, message, guidance),
        None => global(code, message, guidance),
    }
}

#[cfg(test)]
mod tests;
