//! Independent successor layout verification for bounded closed generic types.
//!
//! This authority has its own complete keys, identities, records and fingerprint domain.
//! Existing aggregate-v1 verification and backend entrypoints do not consume it.

use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;
use zryna_source::{SourceMap, SourceMapIdentity};

use crate::{CanonicalMap, LayoutRecord, StorageTarget, TypeUniverseIdentity};

mod encoding;
mod keys;
pub mod raw;
#[cfg(test)]
mod resource_tests;
#[cfg(test)]
mod tests;
mod validation;
mod views;

pub use views::{TypeId, TypeView};

/// Atomic successor layout failure, with no partial sealed graph.
#[derive(Debug)]
pub enum Failure {
    /// Source or raw graph rejection.
    Diagnostics(Vec<Diagnostic>),
    /// Fallible temporary storage could not be reserved.
    AllocationFailure,
    /// An internal checked arithmetic or graph invariant failed.
    InternalFailure,
}

#[derive(Clone, Debug)]
struct Record {
    physical: LayoutRecord,
    key: Vec<u8>,
    kind: raw::TypeKind,
    arguments: Vec<u32>,
}

/// Immutable separately sealed closed layouts for one exact source map and storage target.
///
/// ```compile_fail
/// fn old(layouts: &zryna_layout::generic_v1::VerifiedLayouts) {
///     let _: &zryna_layout::VerifiedLayouts = layouts;
/// }
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedLayouts {
    source_map: SourceMapIdentity,
    universe: [u8; 32],
    target: StorageTarget,
    records: Vec<Record>,
    fingerprint: [u8; 32],
    bytes: Vec<u8>,
}

impl VerifiedLayouts {
    /// Exact issuing source-map identity.
    #[must_use]
    pub const fn source_map_identity(&self) -> SourceMapIdentity {
        self.source_map
    }
    /// Selected storage target.
    #[must_use]
    pub const fn target(&self) -> StorageTarget {
        self.target
    }
    /// Target-neutral complete closed graph identity.
    #[must_use]
    pub const fn universe_identity(&self) -> &[u8; 32] {
        &self.universe
    }
    /// Complete successor fingerprint.
    #[must_use]
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    /// Immutable canonical successor document.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Records in unsigned complete-key order.
    #[must_use]
    pub fn types(&self) -> impl ExactSizeIterator<Item = TypeView<'_>> {
        self.records.iter().map(|record| TypeView { layouts: self, record })
    }
    /// Looks up only an ID issued by this complete closed universe.
    #[must_use]
    pub fn type_by_id(&self, id: TypeId) -> Option<TypeView<'_>> {
        if id.universe != self.universe || id.source_map != self.source_map {
            return None;
        }
        self.records
            .get(usize::try_from(id.index).ok()?)
            .map(|record| TypeView { layouts: self, record })
    }
}

/// Independently verifies raw successor claims and computes both targets before sealing one.
///
/// # Errors
/// Rejects invalid source authority, keys, arity, records, cycles, arithmetic or resources.
pub fn verify(
    graph: &raw::Graph,
    sources: &SourceMap,
    target: StorageTarget,
) -> Result<VerifiedLayouts, Failure> {
    validation::preflight(graph, sources)?;
    let keys = keys::derive(graph)?;
    let canonical = keys::canonical(graph, &keys)?;
    let physical = validation::physical(graph)?;
    let mut errors = crate::Errors::default();
    crate::reject_by_value_cycles(&physical, &canonical, &mut errors);
    check(errors)?;
    let linear = records(graph, &physical, &canonical, &keys, StorageTarget::Linear32V1)?;
    let linux = records(graph, &physical, &canonical, &keys, StorageTarget::LinuxX8664V1)?;
    let linear_bytes = encoding::document(StorageTarget::Linear32V1, &linear)?;
    let linux_bytes = encoding::document(StorageTarget::LinuxX8664V1, &linux)?;
    let (records, bytes) = match target {
        StorageTarget::Linear32V1 => (linear, linear_bytes),
        StorageTarget::LinuxX8664V1 => (linux, linux_bytes),
    };
    let fingerprint = Sha256::digest(&bytes).into();
    Ok(VerifiedLayouts {
        source_map: sources.identity(),
        universe: canonical.universe.0,
        target,
        records,
        fingerprint,
        bytes,
    })
}

/// Independently verifies a producer's complete record bytes and fingerprint.
///
/// # Errors
/// Rejects any changed target, record, ordinal, argument, payload, ID or fingerprint byte.
pub fn verify_claim(
    graph: &raw::Graph,
    sources: &SourceMap,
    target: StorageTarget,
    bytes: &[u8],
    fingerprint: &[u8; 32],
) -> Result<VerifiedLayouts, Failure> {
    let verified = verify(graph, sources, target)?;
    if verified.bytes != bytes || verified.fingerprint != *fingerprint {
        return Err(reject(
            None,
            "ZRYNA-L3001",
            "successor record bytes or fingerprint differ from independent verification",
        ));
    }
    Ok(verified)
}

fn records(
    graph: &raw::Graph,
    physical: &crate::raw::Graph,
    canonical: &CanonicalMap,
    keys: &[Vec<u8>],
    target: StorageTarget,
) -> Result<Vec<Record>, Failure> {
    let mut errors = crate::Errors::default();
    let computed = crate::compute_records(physical, canonical, target, &mut errors);
    check(errors)?;
    let computed = computed.ok_or(Failure::InternalFailure)?;
    let mut records = reserve(computed.len())?;
    for (id, physical) in computed.into_iter().enumerate() {
        let index = canonical.type_to_raw[id];
        let mut arguments = reserve(2)?;
        for argument in keys::children(&graph.types[index].kind) {
            if matches!(graph.types[index].kind, raw::TypeKind::Base(_)) {
                break;
            }
            arguments.push(canonical.raw_to_index[argument]);
        }
        records.push(Record {
            physical,
            key: copy(&keys[index])?,
            kind: graph.types[index].kind.clone(),
            arguments,
        });
    }
    Ok(records)
}

fn check(errors: crate::Errors) -> Result<(), Failure> {
    if errors.is_empty() {
        return Ok(());
    }
    let mut diagnostics = errors.finish();
    for error in &mut diagnostics {
        if error.code() == "ZRYNA-L3201" {
            error.code = "ZRYNA-L7201".into();
        }
    }
    Err(Failure::Diagnostics(diagnostics))
}

fn reject(at: Option<zryna_source::Span>, code: &str, message: &str) -> Failure {
    Failure::Diagnostics(vec![crate::at(
        at,
        code,
        message,
        "provide one complete authenticated successor graph",
    )])
}

fn reserve<T>(count: usize) -> Result<Vec<T>, Failure> {
    let mut result = Vec::new();
    result.try_reserve_exact(count).map_err(|_| Failure::AllocationFailure)?;
    Ok(result)
}

fn copy(bytes: &[u8]) -> Result<Vec<u8>, Failure> {
    let mut result = reserve(bytes.len())?;
    result.extend_from_slice(bytes);
    Ok(result)
}
