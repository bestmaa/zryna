//! Untrusted successor graph claims; existing aggregate-v1 claims remain separate.

use zryna_source::Span;

pub use crate::raw::{Field, Module, ModuleId, NodeId, Variant};

/// Original declaration shape retained independently of closed instances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NominalKind {
    /// Source struct.
    Struct,
    /// Source enum.
    Enum,
}

/// One original declaration in the complete authenticated module inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Declaration {
    /// Containing module.
    pub module: ModuleId,
    /// Source-ordered declaration index.
    pub index: u32,
    /// Source declaration kind.
    pub kind: NominalKind,
    /// Zero, one or two original type parameters.
    pub parameters: u32,
    /// Exact original field or variant count, before substitution.
    pub members: u32,
    /// Exact original declaration span.
    pub span: Span,
}

/// Exhaustive successor stored shapes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeKind {
    /// Original primitive, nongeneric nominal or container shape.
    Base(crate::raw::TypeKind),
    /// Closed source generic struct.
    Struct {
        /// Original module.
        module: ModuleId,
        /// Original source declaration index.
        declaration: u32,
        /// Ordered closed arguments.
        arguments: Vec<NodeId>,
        /// Substituted source-ordered fields.
        fields: Vec<Field>,
    },
    /// Closed source generic enum.
    Enum {
        /// Original module.
        module: ModuleId,
        /// Original source declaration index.
        declaration: u32,
        /// Ordered closed arguments.
        arguments: Vec<NodeId>,
        /// Substituted source-ordered variants.
        variants: Vec<Variant>,
    },
    /// Compiler-owned Option family with none=0 and some=1.
    Option {
        /// Exact some payload.
        argument: NodeId,
    },
    /// Compiler-owned Result family with ok=0 and err=1.
    Result {
        /// Exact ok payload.
        okay: NodeId,
        /// Exact err payload.
        error: NodeId,
    },
}

/// One graph-local stored node, in dense discovery order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeNode {
    /// Dense graph-local identity.
    pub id: NodeId,
    /// Original declaration span for source nominal types; absent otherwise.
    pub span: Option<Span>,
    /// Complete claimed stored shape.
    pub kind: TypeKind,
}

/// Complete untrusted closed graph; no executable authority is carried here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Graph {
    /// Complete final module inventory.
    pub modules: Vec<Module>,
    /// Every original declaration, including uninstantiated templates.
    pub declarations: Vec<Declaration>,
    /// Complete closed stored universe.
    pub types: Vec<TypeNode>,
    /// Program types not otherwise reachable from nominal instances.
    pub program_roots: Vec<NodeId>,
}
