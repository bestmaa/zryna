//! Prepared expression shapes consumed by the existing lowering decisions.

use super::{Span, Ty, syntax};

pub(in super::super) struct StructDecision {
    pub(in super::super) children: Vec<(u32, u32)>,
}

pub(in super::super) struct ArrayDecision<'f> {
    pub(in super::super) elements: &'f [u32],
    pub(in super::super) element: Ty,
}

pub(in super::super) struct EnumDecision {
    pub(in super::super) ordinal: usize,
    pub(in super::super) payload_input: Option<(u32, Ty)>,
}

pub(in super::super) struct ExpressionDecision<'f> {
    pub(in super::super) at: Span,
    pub(in super::super) ty: Option<Ty>,
    pub(in super::super) kind: ExpressionKind<'f>,
}

pub(in super::super) enum ExpressionKind<'f> {
    Scalar {
        operation: crate::data_ownership_v1::scalar_operations::ScalarOperation,
        inputs: Vec<u32>,
    },
    Bool(bool),
    I32(i32),
    String(&'f [u8]),
    Environment(String),
    Reference(&'f syntax::RawIdentifierSyntax),
    Projection(u32),
    InferredClone(u32),
    StringClone(u32),
    StringConcat {
        arguments: &'f [u32],
        callee: Span,
    },
    Call {
        arguments: &'f [u32],
        callee: &'f syntax::RawIdentifierSyntax,
    },
    AggregateClone(u32),
    HandleClone(u32),
    Shared(u32),
    Downgrade(u32),
    Struct(StructDecision),
    Array(ArrayDecision<'f>),
    Vec(ArrayDecision<'f>),
    Enum(EnumDecision),
}
