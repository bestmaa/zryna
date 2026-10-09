use super::{RawExpressionKind, RawStatementKind, RawTypeSyntaxKind, SemanticInput, syntax};

pub(super) fn uses_owned_function(
    input: SemanticInput<'_>,
    function: &syntax::RawFunctionSyntax,
) -> bool {
    input.command.is_some()
        || function
            .body
            .statements
            .iter()
            .any(|statement| matches!(statement.kind, RawStatementKind::WeakUpgrade { .. }))
}
pub(super) fn has_root_borrow_syntax(
    file: &syntax::SourceUnit,
    function: &syntax::RawFunctionSyntax,
) -> bool {
    function.body.statements.iter().any(|statement| {
        let RawStatementKind::LocalDeclaration { type_syntax, .. } = statement.kind else {
            return false;
        };
        usize::try_from(type_syntax)
            .ok()
            .and_then(|index| file.type_syntax().get(index))
            .is_some_and(|ty| {
                matches!(
                    ty.kind,
                    RawTypeSyntaxKind::Borrow { .. } | RawTypeSyntaxKind::BorrowMut { .. }
                )
            })
    }) || function.body.expressions.iter().any(|expression| {
        matches!(
            expression.kind,
            RawExpressionKind::Borrow { .. } | RawExpressionKind::BorrowMut { .. }
        )
    })
}
