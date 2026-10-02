//! Closed, untrusted extension IR records. No record is an authority constructor.

use zryna_layout::TypeId;
use zryna_semantics::native_c_v0::body::{
    BoundaryExit, BoundaryOwner, FlowStep, PrivateOrigin, PrivateOwner, PrivatePreparation,
    ValueType,
};
use zryna_source::{FileId, SourceMapIdentity, Span};
use zryna_syntax::{native_c_source_v0::raw as syntax, native_c_v0::raw as declaration};

/// Complete native-requirement program claims, including unused declarations.
#[derive(Clone, Debug)]
pub struct Program {
    /// Claimed original source issuer; equal bytes from another map are insufficient.
    pub source_map: SourceMapIdentity,
    /// Complete declaration schema, including all libraries, imports, exports and sites.
    pub declarations: declaration::DeclarationSet,
    /// Claimed private storage/runtime descriptors; digests supply no issuer.
    pub storage: Storage,
    /// Every original function in complete source order.
    pub functions: Vec<Function>,
}

/// Claimed descriptors of the retained dual-layout/runtime authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Storage {
    /// Claimed type universe.
    pub universe: [u8; 32],
    /// Claimed Linear32 layout fingerprint.
    pub linear: [u8; 32],
    /// Claimed Linux layout fingerprint.
    pub native: [u8; 32],
    /// Claimed runtime identifier.
    pub runtime: String,
}

/// Complete source-bound typed function and explicit effect/exit inventory.
#[derive(Clone, Debug)]
pub struct Function {
    /// Original source file claim.
    pub file: FileId,
    /// Original declaration ordinal within that file.
    pub ordinal: usize,
    /// Claimed complete declaration range.
    pub span: Span,
    /// Claimed original identifier.
    pub name: String,
    /// Ordered private/scalar input bindings; no public pointer ABI is implied.
    pub parameters: Vec<syntax::Binding>,
    /// Claimed corresponding genuine private layout identities.
    pub parameter_layouts: Vec<TypeId>,
    /// Claimed language result category.
    pub result: ValueType,
    /// Claimed private result layout.
    pub result_layout: TypeId,
    /// Exact C export ordinal, or no public entry.
    pub export: Option<usize>,
    /// Complete original statements, with exact referenced expression ordinals.
    pub statements: Vec<syntax::Statement>,
    /// Dense typed value definitions, one per original expression occupant.
    pub values: Vec<Value>,
    /// Every original effect plus explicit private preparation and terminal exits.
    pub effects: Vec<Effect>,
    /// Complete private origin/release inventory, separate from foreign resources.
    pub private_owners: Vec<PrivateOwner>,
}

/// One dense typed value definition; copying it grants no enclosing seal.
#[derive(Clone, Debug)]
pub struct Value {
    /// Dense definition/source expression ordinal.
    pub id: usize,
    /// Original expression span.
    pub span: Span,
    /// Claimed language category.
    pub ty: ValueType,
    /// Closed operation and exact preceding operands.
    pub kind: ValueKind,
    /// Exact scoped foreign token, if any.
    pub token: Option<usize>,
    /// Creating status call provenance, independent of equal integer bits.
    pub status_call: Option<usize>,
    /// Stable private storage origin retained across local moves.
    pub origin: Option<PrivateOrigin>,
}

/// Closed source operations; keys are compile-time selectors, never runtime addresses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueKind {
    /// Full signed literal.
    I32(i32),
    /// Canonical Boolean literal.
    Bool(bool),
    /// Exact literal declaration/kind key.
    Key(String),
    /// Dense parameter/local binding identity; availability is independently replayed.
    Local(usize),
    /// Modulo-2^32 addition of two preceding signed definitions.
    WrappingAdd(usize, usize),
    /// Reserved operation with its exact preceding source argument order.
    Primitive(declaration::Primitive, Vec<usize>),
}

/// Explicit effect operation and all required terminal obligations at that program point.
#[derive(Clone, Debug)]
pub struct Effect {
    /// Dense original effect ordinal.
    pub id: usize,
    /// Closed typed operation; retains exact raw/safe site and call/status/slot identities.
    pub operation: FlowStep,
    /// Ordered private storage stages and issuer-specific faults.
    pub preparation: Option<PrivatePreparation>,
    /// Complete terminal outcomes, including process failure without cleanup guarantees.
    pub exits: Vec<BoundaryExit>,
    /// Conditional owners committed only on this operation's successful completion.
    pub completed: Vec<BoundaryOwner>,
}
