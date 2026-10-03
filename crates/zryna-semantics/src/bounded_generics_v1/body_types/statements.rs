use zryna_syntax::v4::RawStatementKind;
use zryna_syntax::v5::RawTypeSyntaxKind;

use super::super::DeclarationIdentity;
use super::model::{Head, Kind, Origin, Scalar, Ty};
use super::value_names::{Binding, Scope};
use super::{BodyTypeFailure, Checker, constraints, resources, substitution, type_resolution};

pub(super) fn check<'a>(
    checker: &mut Checker<'a, '_>,
    owner: DeclarationIdentity,
    index: u32,
    scope: &mut Scope<'a>,
) -> Result<(), BodyTypeFailure> {
    let function = resources::raw_function(checker.context, owner);
    let statement = &function.body.statements[index as usize];
    match &statement.kind {
        RawStatementKind::LocalDeclaration { name, type_syntax, initializer, .. } => {
            let actual = checker.expression(owner, *initializer);
            let node = &checker.context.syntax().files()[owner.module().index() as usize]
                .type_syntax[*type_syntax as usize];
            let expected = if matches!(node.kind, RawTypeSyntaxKind::Missing) {
                actual
            } else {
                type_resolution::source(checker, owner.module().index(), *type_syntax)
            };
            constraints::require(
                checker,
                owner,
                expected,
                actual,
                function.body.expressions[*initializer as usize].span,
                "local initializer",
            )?;
            // Introduction follows the complete initializer walk, so outer names remain visible.
            scope.bind(checker, Binding { name, ty: expected });
        }
        RawStatementKind::Assignment { target, value, .. } => {
            let expected = checker.expression(owner, *target);
            let actual = checker.expression(owner, *value);
            constraints::require(
                checker,
                owner,
                expected,
                actual,
                function.body.expressions[*value as usize].span,
                "assignment",
            )?;
        }
        RawStatementKind::Return { value, .. } => {
            let expected =
                type_resolution::source(checker, owner.module().index(), function.result_type);
            let actual = checker.expression(owner, *value);
            constraints::require(
                checker,
                owner,
                expected,
                actual,
                function.body.expressions[*value as usize].span,
                "return",
            )?;
        }
        RawStatementKind::Block { .. } | RawStatementKind::ExpressionStatement { .. } => {}
        RawStatementKind::If { condition, .. } | RawStatementKind::While { condition, .. } => {
            let actual = checker.expression(owner, *condition);
            if let Some(actual) = actual {
                if substitution::head(&checker.tables, owner, actual)?
                    .is_some_and(|head| matches!(head.kind, Kind::Parameter(_)))
                {
                    constraints::opaque(
                        checker,
                        function.body.expressions[*condition as usize].span,
                        "bool condition capability",
                    );
                } else {
                    constraints::require(
                        checker,
                        owner,
                        Some(Ty::scalar(Scalar::Bool)),
                        Some(actual),
                        function.body.expressions[*condition as usize].span,
                        "condition",
                    )?;
                }
            }
        }
        RawStatementKind::WeakUpgrade { keyword_span, weak, .. } => {
            let Some(ty) = checker.expression(owner, *weak) else { return Ok(()) };
            let Some(head) = substitution::head(&checker.tables, owner, ty)? else { return Ok(()) };
            if matches!(head.kind, Kind::Parameter(_)) {
                constraints::opaque(checker, *keyword_span, "Weak handle capability");
                return Ok(());
            }
            if head.kind != Kind::Weak {
                constraints::mismatch(checker, *keyword_span, "upgradeWeak requires a Weak handle");
                return Ok(());
            }
            let head = Head::unary(
                Kind::Shared,
                head.children[0].ok_or(BodyTypeFailure::InternalFailure)?,
            );
            substitution::validate_children(&checker.tables, owner, head)?;
            let records = checker.tables.function_mut(owner);
            records.next_rank =
                records.next_rank.checked_add(1).ok_or(BodyTypeFailure::InternalFailure)?;
            let record = &mut records.statements[index as usize];
            record.head = Some(head);
            record.rank = records.next_rank;
            record.ty = Some(Ty {
                origin: Origin::Statement { function: owner, statement: index },
                environment: 0,
            });
        }
    }
    Ok(())
}
