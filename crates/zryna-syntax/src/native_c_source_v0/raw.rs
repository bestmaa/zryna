//! Parsed syntax values. Constructing or cloning them supplies no authentication authority.

use crate::native_c_v0::raw::Primitive;

/// Half-open byte range within one source file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    /// Inclusive UTF-8 byte start.
    pub start: u32,
    /// Exclusive UTF-8 byte end.
    pub end: u32,
}

/// Closed source annotation vocabulary for the restricted foreign grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Type {
    /// Signed scalar.
    I32,
    /// Boolean scalar.
    Bool,
    /// Private string.
    String,
    /// Private vector of signed scalars.
    VecI32,
    /// Scoped read-only bytes.
    Bytes,
    /// Caller scalar output slot.
    I32Out,
    /// Caller handle output slot.
    HandleOut,
    /// Caller byte-pointer output slot.
    BytesOut,
    /// Caller count output slot.
    CountOut,
    /// Nominal foreign handle token.
    Handle,
    /// Nominal foreign byte token.
    OwnedBytes,
}

/// Parameter or explicitly annotated local binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    /// Exact ASCII identifier.
    pub name: String,
    /// Explicit source annotation.
    pub ty: Type,
    /// Complete name/annotation bytes.
    pub range: Range,
}

/// Expression in a flat postorder arena; indices never provide external authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expression {
    /// Exact complete expression bytes.
    pub range: Range,
    /// Parsed syntax, without inferred type or ownership state.
    pub kind: ExpressionKind,
}

/// Closed expression grammar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpressionKind {
    /// Exact signed scalar literal.
    I32(i32),
    /// Exact Boolean literal.
    Bool(bool),
    /// Literal ASCII operation/kind key, with no escapes.
    Key(String),
    /// Local reference; resolution remains downstream.
    Local(String),
    /// Left-associated addition of preceding expression indices.
    Add(usize, usize),
    /// Reserved directly spelled intrinsic and preceding argument indices.
    Intrinsic(Primitive, Vec<usize>),
}

/// Complete statement whose expression index refers to its function arena.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Statement {
    /// Complete statement bytes.
    pub range: Range,
    /// Parsed occupant.
    pub kind: StatementKind,
}

/// Closed statement grammar; no unaccounted branch or recovery occupant exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StatementKind {
    /// Explicit initialized immutable local.
    Const(Binding, usize),
    /// Value return.
    Return(usize),
    /// Exact `if (status !== 0) { return expression; }` syntax.
    Guard(String, usize),
    /// Expression statement.
    Expression(usize),
}

/// Complete parsed function, including every body statement and expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    /// Complete declaration bytes.
    pub range: Range,
    /// Exact function name.
    pub name: String,
    /// Explicit export marker.
    pub exported: bool,
    /// Ordered explicitly typed parameters.
    pub parameters: Vec<Binding>,
    /// Explicit result annotation.
    pub result: Type,
    /// Complete ordered body.
    pub statements: Vec<Statement>,
    /// Flat postorder expression arena, bounded independently of recursion.
    pub expressions: Vec<Expression>,
}
