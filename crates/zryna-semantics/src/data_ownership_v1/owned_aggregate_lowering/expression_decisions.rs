use zryna_layout::{self as layout, TypeCategory, raw as raw_layout};
use zryna_source::Span;
use zryna_syntax::v4::{self as syntax, RawExpressionKind};

use super::super::layout_graph::semantic_type;
use super::super::{Decl, Errors, SemanticInput, Ty};
use crate::data_ownership_v1::diagnostics::span;

#[path = "constructor_decisions.rs"]
mod constructors;

pub(super) struct ExpressionDecisions<'a, 'f, 'e> {
    pub(super) input: SemanticInput<'a>,
    pub(super) file: &'a syntax::SourceUnit,
    pub(super) function: &'f syntax::RawFunctionSyntax,
    pub(super) module: usize,
    pub(super) declarations: &'a [Decl],
    pub(super) graph: &'a raw_layout::Graph,
    pub(super) node_types: &'a [Option<Ty>],
    pub(super) layouts: &'a layout::VerifiedLayouts,
    pub(super) errors: &'e mut Errors<'a>,
}

pub(super) struct StructDecision {
    pub(super) children: Vec<(u32, u32)>,
}

pub(super) struct ArrayDecision<'f> {
    pub(super) elements: &'f [u32],
    pub(super) element: Ty,
}

pub(super) struct EnumDecision {
    pub(super) ordinal: usize,
    pub(super) payload_input: Option<(u32, Ty)>,
}

pub(super) struct ExpressionDecision<'f> {
    pub(super) at: Span,
    pub(super) ty: Option<Ty>,
    pub(super) kind: ExpressionKind<'f>,
}

pub(super) enum ExpressionKind<'f> {
    Scalar { operation: super::super::scalar_operations::ScalarOperation, inputs: Vec<u32> },
    Bool(bool),
    I32(i32),
    String(&'f [u8]),
    Environment(String),
    Reference(&'f syntax::RawIdentifierSyntax),
    Projection(u32),
    InferredClone(u32),
    StringClone(u32),
    StringConcat { arguments: &'f [u32], callee: Span },
    Call { arguments: &'f [u32], callee: &'f syntax::RawIdentifierSyntax },
    AggregateClone(u32),
    HandleClone(u32),
    Shared(u32),
    Downgrade(u32),
    Struct(StructDecision),
    Array(ArrayDecision<'f>),
    Vec(ArrayDecision<'f>),
    Enum(EnumDecision),
}

impl<'f> ExpressionDecisions<'_, 'f, '_> {
    pub(super) fn nonindexed_owned_route(&self) -> bool {
        !super::has_indexed_borrow(self.function)
            && super::has_nonindexed_owned_borrow(
                self.function,
                self.file,
                self.module,
                self.declarations,
                self.node_types,
            )
    }

    pub(super) fn child_type(&mut self, type_syntax: u32) -> Option<Ty> {
        semantic_type(
            self.file,
            type_syntax,
            self.module,
            self.declarations,
            self.graph,
            self.node_types,
            self.errors,
        )
    }

    #[cfg(test)]
    pub(super) fn classify(&mut self, id: u32, expected: Ty) -> Option<ExpressionDecision<'f>> {
        self.classify_prepared(id, Some(expected), false)
    }

    pub(super) fn primitive(&self, category: TypeCategory) -> Option<Ty> {
        self.node_types.iter().flatten().find(|ty| ty.category == category).copied()
    }

    fn scalar_decision(
        &self,
        kind: &RawExpressionKind,
        at: Span,
    ) -> Option<ExpressionDecision<'f>> {
        let (operation, inputs) = super::super::scalar_operations::select(kind)?;
        Some(ExpressionDecision {
            at,
            ty: Some(self.primitive(operation.result_category())?),
            kind: ExpressionKind::Scalar { operation, inputs },
        })
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn classify_prepared(
        &mut self,
        id: u32,
        expected: Option<Ty>,
        scalar: bool,
    ) -> Option<ExpressionDecision<'f>> {
        let expression =
            usize::try_from(id).ok().and_then(|index| self.function.body.expressions.get(index))?;
        let at = span(self.input.sources(), expression.span);
        if scalar && let Some(decision) = self.scalar_decision(&expression.kind, at) {
            return Some(decision);
        }
        if let Some(key) =
            super::super::command_support::environment_key(self.input, self.function, id)
        {
            let ty = super::super::command_support::builtin_type(
                self.input,
                self.declarations,
                self.node_types,
            )?;
            if expected.is_some_and(|expected| expected != ty) {
                self.unsupported_prepared_expression(at);
                return None;
            }
            return Some(ExpressionDecision {
                at,
                ty: Some(ty),
                kind: ExpressionKind::Environment(key),
            });
        }
        let mut ty = expected;
        let kind = match &expression.kind {
            RawExpressionKind::BoolLiteral { value }
                if expected.is_none_or(|ty| ty.category == TypeCategory::Bool) =>
            {
                ty = Some(expected.or_else(|| self.primitive(TypeCategory::Bool))?);
                ExpressionKind::Bool(*value)
            }
            RawExpressionKind::I32Literal { spelling }
                if expected.is_none_or(|ty| ty.category == TypeCategory::I32) =>
            {
                ty = Some(expected.or_else(|| self.primitive(TypeCategory::I32))?);
                let value = self.integer(spelling, expected.is_none(), at)?;
                ExpressionKind::I32(value)
            }
            RawExpressionKind::StringLiteral { spelling }
                if expected.is_none_or(|ty| ty.category == TypeCategory::String) =>
            {
                ty = Some(expected.or_else(|| self.primitive(TypeCategory::String))?);
                let bytes = spelling.get(1..spelling.len().checked_sub(1)?)?.as_bytes();
                ExpressionKind::String(bytes)
            }
            RawExpressionKind::Reference { name } => ExpressionKind::Reference(name),
            RawExpressionKind::FieldAccess { .. } | RawExpressionKind::Index { .. } => {
                ExpressionKind::Projection(id)
            }
            RawExpressionKind::Clone { value, .. } if expected.is_none() => {
                ExpressionKind::InferredClone(*value)
            }
            RawExpressionKind::Clone { value, .. }
                if expected.is_some_and(|ty| ty.category == TypeCategory::String) =>
            {
                ExpressionKind::StringClone(*value)
            }
            RawExpressionKind::Clone { value, .. }
                if expected.is_some_and(|ty| {
                    matches!(ty.category, TypeCategory::Shared | TypeCategory::Weak)
                }) =>
            {
                ExpressionKind::HandleClone(*value)
            }
            RawExpressionKind::Clone { value, .. } => ExpressionKind::AggregateClone(*value),
            RawExpressionKind::Shared { value, .. }
                if expected.is_some_and(|ty| ty.category == TypeCategory::Shared) =>
            {
                ExpressionKind::Shared(*value)
            }
            RawExpressionKind::Downgrade { value, .. }
                if expected.is_some_and(|ty| ty.category == TypeCategory::Weak) =>
            {
                ExpressionKind::Downgrade(*value)
            }
            RawExpressionKind::Call { callee, arguments, .. }
                if expected.is_none_or(|ty| ty.category == TypeCategory::String)
                    && callee.text == "concat" =>
            {
                ty = Some(expected.or_else(|| self.primitive(TypeCategory::String))?);
                ExpressionKind::StringConcat {
                    arguments,
                    callee: span(self.input.sources(), callee.span),
                }
            }
            RawExpressionKind::StructConstruction { type_name, fields, .. }
                if expected.is_none_or(|ty| ty.category == TypeCategory::Struct) =>
            {
                let (actual, decision) = self.struct_decision(type_name, fields, expected, at)?;
                ty = Some(actual);
                ExpressionKind::Struct(decision)
            }
            RawExpressionKind::Call { callee, arguments, .. } => {
                ExpressionKind::Call { callee, arguments }
            }
            RawExpressionKind::FixedArrayConstruction { type_syntax, elements, .. }
                if expected.is_none_or(|ty| ty.category == TypeCategory::FixedArray) =>
            {
                ty = Some(match expected {
                    Some(ty) => ty,
                    None => self.child_type(*type_syntax)?,
                });
                ExpressionKind::Array(self.array_decision(*type_syntax, elements, ty?, at)?)
            }
            RawExpressionKind::VecConstruction { type_syntax, elements, .. }
                if expected.is_none_or(|ty| ty.category == TypeCategory::Vec) =>
            {
                let (actual, decision) =
                    self.vector_decision(*type_syntax, elements, expected, at)?;
                ty = Some(actual);
                ExpressionKind::Vec(decision)
            }
            RawExpressionKind::EnumConstruction { type_name, variant, payload, .. }
                if expected.is_none_or(|ty| ty.category == TypeCategory::Enum) =>
            {
                let (actual, decision) =
                    self.enum_decision(type_name, variant, *payload, expected, at)?;
                ty = Some(actual);
                ExpressionKind::Enum(decision)
            }
            _ => {
                self.unsupported_prepared_expression(at);
                return None;
            }
        };
        Some(ExpressionDecision { at, ty, kind })
    }
    fn unsupported_prepared_expression(&mut self, at: Span) {
        if self.nonindexed_owned_route() {
            self.errors.at(
                "ZRYNA-M3013",
                at,
                "expression does not produce the exact required owned type",
                "prepare a value matching the borrowed String or Vec referent type",
            );
            return;
        }
        self.errors.at(
            "ZRYNA-M3016",
            at,
            "expression is outside private owned Struct/Enum/FixedArray lowering",
            "use literals, whole-value moves, and exact Struct/Enum/FixedArray constructors",
        );
    }

    fn integer(&mut self, spelling: &str, inferred: bool, at: Span) -> Option<i32> {
        if inferred {
            super::super::scalar_operations::integer(spelling, at, self.errors)
        } else {
            spelling.parse::<i32>().ok().or_else(|| {
                self.errors.at(
                    "ZRYNA-M3016",
                    at,
                    "aggregate leaf integer is outside i32",
                    "use one in-range i32 leaf",
                );
                None
            })
        }
    }
    fn vector_decision(
        &mut self,
        syntax: u32,
        elements: &'f [u32],
        expected: Option<Ty>,
        at: Span,
    ) -> Option<(Ty, ArrayDecision<'f>)> {
        let actual = self.child_type(syntax)?;
        if expected.is_some_and(|expected| actual != expected) {
            self.errors.at(
                "ZRYNA-M3013",
                at,
                "Vec construction type differs from its contextual type",
                "construct the exact annotated Vec type",
            );
            return None;
        }
        let element_id = self.layouts.type_by_id(actual.layout)?.referenced_type()?;
        let element =
            self.node_types.iter().flatten().find(|ty| ty.layout == element_id).copied()?;
        Some((actual, ArrayDecision { elements, element }))
    }
}
