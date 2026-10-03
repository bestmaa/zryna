//! Preorder diagnostic depth over bounded, untrusted postorder expression arenas.

use zryna_diagnostics::Diagnostic;
use zryna_source::{SourceMap, UntrustedSpan};
use zryna_syntax::{v3, v4};

mod shapes;
pub(super) use shapes::diagnostic as source_diagnostic;

pub(super) struct ExpressionBudgets {
    pub(super) function: usize,
    pub(super) project: usize,
    pub(super) aggregate: usize,
}

fn first_overflow(
    sources: &SourceMap,
    root: u32,
    initial_depth: u32,
    version: u32,
    node: impl Fn(u32) -> (UntrustedSpan, &'static str, Vec<u32>, bool),
) -> Option<Diagnostic> {
    let mut pending = vec![(root, initial_depth)];
    while let Some((id, depth)) = pending.pop() {
        let (span, kind, children, stop) = node(id);
        if depth > v3::MAX_NESTING_DEPTH {
            return Some(Diagnostic::error_at(
                "ZRYNA-F2002",
                sources.verify_span(span).expect("native expression is source-bound"),
                format!("expression depth uses unsupported syntax '{kind}'"),
                format!("use only the documented protocol-v{version} bootstrap syntax"),
            ));
        }
        if stop {
            return None;
        }
        pending.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
    }
    None
}

pub(super) fn v3_diagnostic(
    sources: &SourceMap,
    expressions: &[v3::RawExpressionSyntax],
    root: u32,
    block_depth: u32,
) -> Option<Diagnostic> {
    use v3::RawExpressionKind as E;
    first_overflow(sources, root, block_depth + 1, 3, |id| {
        let expression = &expressions[id as usize];
        let (kind, children) = match &expression.kind {
            E::Reference { .. } => ("Identifier", vec![]),
            E::BoolLiteral { value } => (boolean_kind(*value), vec![]),
            E::I32Literal { spelling } => (integer_kind(spelling), vec![]),
            E::Negation { operand, .. } => ("PrefixUnaryExpression", vec![*operand]),
            E::Addition { lhs, rhs, .. }
            | E::Subtraction { lhs, rhs, .. }
            | E::Multiplication { lhs, rhs, .. }
            | E::Equal { lhs, rhs, .. }
            | E::NotEqual { lhs, rhs, .. }
            | E::LessThan { lhs, rhs, .. }
            | E::LessEqual { lhs, rhs, .. }
            | E::GreaterThan { lhs, rhs, .. }
            | E::GreaterEqual { lhs, rhs, .. } => ("BinaryExpression", vec![*lhs, *rhs]),
            E::Call { arguments, .. } => ("CallExpression", arguments.clone()),
        };
        (expression.span, kind, children, false)
    })
}

pub(super) fn v4_diagnostic(
    sources: &SourceMap,
    expressions: &[v4::RawExpressionSyntax],
    root: u32,
    block_depth: u32,
) -> Option<Diagnostic> {
    use v4::{RawExpressionKind as E, RawFieldInitializerKind as F};
    first_overflow(sources, root, block_depth + 1, 4, |id| {
        let expression = &expressions[id as usize];
        let (kind, children) = match &expression.kind {
            E::Reference { .. } => ("Identifier", vec![]),
            E::BoolLiteral { value } => (boolean_kind(*value), vec![]),
            E::I32Literal { spelling } => (integer_kind(spelling), vec![]),
            E::StringLiteral { .. } => ("StringLiteral", vec![]),
            E::Negation { operand, .. } => ("PrefixUnaryExpression", vec![*operand]),
            E::Addition { lhs, rhs, .. }
            | E::Subtraction { lhs, rhs, .. }
            | E::Multiplication { lhs, rhs, .. }
            | E::Equal { lhs, rhs, .. }
            | E::NotEqual { lhs, rhs, .. }
            | E::LessThan { lhs, rhs, .. }
            | E::LessEqual { lhs, rhs, .. }
            | E::GreaterThan { lhs, rhs, .. }
            | E::GreaterEqual { lhs, rhs, .. } => ("BinaryExpression", vec![*lhs, *rhs]),
            E::FieldAccess { base, .. } => ("PropertyAccessExpression", vec![*base]),
            E::Index { base, index, .. } => ("ElementAccessExpression", vec![*base, *index]),
            E::Call { arguments, .. } => ("CallExpression", arguments.clone()),
            E::StructConstruction { fields, .. } => (
                "CallExpression",
                fields
                    .iter()
                    .map(|field| match field.kind {
                        F::Shorthand { value, .. } | F::Explicit { value, .. } => value,
                    })
                    .collect(),
            ),
            E::EnumConstruction { payload, .. } => {
                ("CallExpression", payload.iter().copied().collect())
            }
            E::FixedArrayConstruction { elements, .. } | E::VecConstruction { elements, .. } => {
                ("CallExpression", elements.clone())
            }
            E::Clone { value, .. }
            | E::Shared { value, .. }
            | E::Downgrade { value, .. }
            | E::Borrow { value, .. }
            | E::BorrowMut { value, .. } => ("CallExpression", vec![*value]),
            E::VecPush { vector, value, .. } => ("CallExpression", vec![*vector, *value]),
            E::Match { scrutinee, arms, .. } => (
                "CallExpression",
                std::iter::once(*scrutinee).chain(arms.iter().map(|arm| arm.value)).collect(),
            ),
        };
        (expression.span, kind, children, false)
    })
}

fn boolean_kind(value: bool) -> &'static str {
    if value { "TrueKeyword" } else { "FalseKeyword" }
}

fn integer_kind(spelling: &str) -> &'static str {
    if spelling.starts_with('-') { "PrefixUnaryExpression" } else { "FirstLiteralToken" }
}
