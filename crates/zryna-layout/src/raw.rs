//! Untrusted claims for the original aggregate layout v1 boundary.

use zryna_source::{FileId, Span};

/// Claimed dense module identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ModuleId(pub u32);

/// Claimed graph-local node identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NodeId(pub u32);

/// One final-module inventory claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    /// Claimed dense module identity.
    pub id: ModuleId,
    /// Exact source-map authority for the module.
    pub source_file: FileId,
    /// Number of source-ordered nominal data declarations.
    pub data_declarations: u32,
}

/// One source-ordered struct field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    /// Claimed zero-based source ordinal.
    pub ordinal: u32,
    /// Graph-local field type.
    pub ty: NodeId,
}

/// One source-ordered enum variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Variant {
    /// Claimed zero-based source ordinal and discriminant.
    pub ordinal: u32,
    /// Optional graph-local payload type.
    pub payload: Option<NodeId>,
}

/// Exhaustive stored type forms admitted by aggregate layout v1.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeKind {
    /// One-byte stored Boolean.
    Bool,
    /// Little-endian signed 32-bit integer.
    I32,
    /// Owned string handle.
    String,
    /// Nominal source-ordered struct.
    Struct {
        /// Authenticated containing module.
        module: ModuleId,
        /// Zero-based source declaration index.
        declaration: u32,
        /// Fields in exact source order.
        fields: Vec<Field>,
    },
    /// Nominal source-ordered enum.
    Enum {
        /// Authenticated containing module.
        module: ModuleId,
        /// Zero-based source declaration index.
        declaration: u32,
        /// Variants in exact source order.
        variants: Vec<Variant>,
    },
    /// Structural fixed array.
    FixedArray {
        /// Graph-local element type.
        element: NodeId,
        /// Exact admitted element count.
        length: u64,
    },
    /// Owned vector handle.
    Vec {
        /// Graph-local element type.
        element: NodeId,
    },
    /// Immutable shared handle.
    Shared {
        /// Graph-local control-block payload type.
        payload: NodeId,
    },
    /// Weak shared handle.
    Weak {
        /// Graph-local control-block payload type.
        payload: NodeId,
    },
    /// Unstorable borrow authority retained for fail-closed rejection.
    Borrow {
        /// Graph-local referent retained only for rejection.
        referent: NodeId,
    },
}

/// One claimed graph node. IDs must be dense but need not be in canonical type order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeNode {
    /// Claimed graph-local identity.
    pub id: NodeId,
    /// Authoritative declaration span for nominal nodes.
    pub span: Option<Span>,
    /// Claimed type structure.
    pub kind: TypeKind,
}

/// Complete raw aggregate type graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Graph {
    /// Complete final module inventory in canonical path order.
    pub modules: Vec<Module>,
    /// Complete stored type universe in arbitrary discovery order.
    pub types: Vec<TypeNode>,
    /// Stored program types not otherwise reachable from nominal declarations.
    pub program_roots: Vec<NodeId>,
}
