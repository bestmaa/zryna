use zryna_syntax::v5::RawExpressionKind;

use super::super::{DeclarationIdentity, DeclarationKind};
use super::model::{Head, Kind, Scalar, Ty};
use super::value_names::Scope;
use super::{
    BodyTypeFailure, Checker, arguments, calls, constructions, containers, matches, projections,
    resources, scalars, substitution, type_resolution, value_names,
};

/// Push in reverse evaluation order. Match arms require their own lexical walk.
pub(super) fn children(kind: &RawExpressionKind, mut push: impl FnMut(u32)) {
    match kind {
        RawExpressionKind::Reference { .. }
        | RawExpressionKind::BoolLiteral { .. }
        | RawExpressionKind::I32Literal { .. }
        | RawExpressionKind::StringLiteral { .. }
        | RawExpressionKind::Match { .. } => {}
        RawExpressionKind::Negation { operand, .. } => push(*operand),
        RawExpressionKind::Addition { lhs, rhs, .. }
        | RawExpressionKind::Subtraction { lhs, rhs, .. }
        | RawExpressionKind::Multiplication { lhs, rhs, .. }
        | RawExpressionKind::Equal { lhs, rhs, .. }
        | RawExpressionKind::NotEqual { lhs, rhs, .. }
        | RawExpressionKind::LessThan { lhs, rhs, .. }
        | RawExpressionKind::LessEqual { lhs, rhs, .. }
        | RawExpressionKind::GreaterThan { lhs, rhs, .. }
        | RawExpressionKind::GreaterEqual { lhs, rhs, .. } => {
            push(*rhs);
            push(*lhs);
        }
        RawExpressionKind::Call { arguments, .. } => {
            for argument in arguments.iter().rev() {
                push(*argument);
            }
        }
        RawExpressionKind::StructConstruction { fields, .. } => {
            for field in fields.iter().rev() {
                match field.kind {
                    zryna_syntax::v4::RawFieldInitializerKind::Explicit { value, .. }
                    | zryna_syntax::v4::RawFieldInitializerKind::Shorthand { value, .. } => {
                        push(value);
                    }
                }
            }
        }
        RawExpressionKind::EnumConstruction { payload, .. } => {
            if let Some(payload) = payload {
                push(*payload);
            }
        }
        RawExpressionKind::FixedArrayConstruction { elements, .. }
        | RawExpressionKind::VecConstruction { elements, .. } => {
            for element in elements.iter().rev() {
                push(*element);
            }
        }
        RawExpressionKind::FieldAccess { base, .. } => push(*base),
        RawExpressionKind::Index { base, index, .. } => {
            push(*index);
            push(*base);
        }
        RawExpressionKind::Clone { value, .. }
        | RawExpressionKind::Shared { value, .. }
        | RawExpressionKind::Downgrade { value, .. }
        | RawExpressionKind::Borrow { value, .. }
        | RawExpressionKind::BorrowMut { value, .. } => push(*value),
        RawExpressionKind::VecPush { vector, value, .. } => {
            push(*value);
            push(*vector);
        }
    }
}

pub(super) fn check(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    index: u32,
    scope: &Scope<'_>,
) -> Result<(), BodyTypeFailure> {
    let function = resources::raw_function(checker.context, owner);
    let expression = &function.body.expressions[index as usize];
    let ty = match &expression.kind {
        RawExpressionKind::Reference { name } => reference(checker, owner, index, name, scope)?,
        RawExpressionKind::BoolLiteral { .. } => Some(Ty::scalar(Scalar::Bool)),
        RawExpressionKind::I32Literal { spelling } => {
            scalars::integer(checker, spelling, expression.span)
        }
        RawExpressionKind::StringLiteral { .. } => Some(Ty::scalar(Scalar::String)),
        RawExpressionKind::Negation { operator_span, operand } => {
            scalars::operation(checker, owner, &expression.kind, *operator_span, *operand, None)?
        }
        RawExpressionKind::Addition { operator_span, lhs, rhs }
        | RawExpressionKind::Subtraction { operator_span, lhs, rhs }
        | RawExpressionKind::Multiplication { operator_span, lhs, rhs }
        | RawExpressionKind::Equal { operator_span, lhs, rhs }
        | RawExpressionKind::NotEqual { operator_span, lhs, rhs }
        | RawExpressionKind::LessThan { operator_span, lhs, rhs }
        | RawExpressionKind::LessEqual { operator_span, lhs, rhs }
        | RawExpressionKind::GreaterThan { operator_span, lhs, rhs }
        | RawExpressionKind::GreaterEqual { operator_span, lhs, rhs } => {
            scalars::operation(checker, owner, &expression.kind, *operator_span, *lhs, Some(*rhs))?
        }
        RawExpressionKind::Call { callee, type_arguments, arguments, .. } => {
            calls::check(checker, owner, callee, type_arguments.as_ref(), arguments, scope)?
        }
        RawExpressionKind::StructConstruction { type_name, type_arguments, fields, .. } => {
            constructions::structure(
                checker,
                owner,
                index,
                type_name,
                type_arguments.as_ref(),
                fields,
            )?
        }
        RawExpressionKind::EnumConstruction {
            type_name, variant, type_arguments, payload, ..
        } => constructions::enumeration(
            checker,
            owner,
            index,
            type_name,
            variant,
            type_arguments.as_ref(),
            *payload,
        )?,
        RawExpressionKind::FixedArrayConstruction { type_syntax, elements, .. } => {
            containers::construction(checker, owner, *type_syntax, elements, true, expression.span)?
        }
        RawExpressionKind::VecConstruction { type_syntax, elements, .. } => {
            containers::construction(
                checker,
                owner,
                *type_syntax,
                elements,
                false,
                expression.span,
            )?
        }
        RawExpressionKind::FieldAccess { base, dot_span, field } => {
            projections::field(checker, owner, index, *base, field, *dot_span)?
        }
        RawExpressionKind::Index { base, open_bracket_span, index: subscript, .. } => {
            projections::index(checker, owner, index, *base, *subscript, *open_bracket_span)?
        }
        RawExpressionKind::Clone { keyword_span, value, .. }
        | RawExpressionKind::Shared { keyword_span, value, .. }
        | RawExpressionKind::Downgrade { keyword_span, value, .. }
        | RawExpressionKind::Borrow { keyword_span, value, .. }
        | RawExpressionKind::BorrowMut { keyword_span, value, .. } => {
            containers::unary(checker, owner, index, &expression.kind, *value, *keyword_span)?
        }
        RawExpressionKind::VecPush { keyword_span, vector, value, .. } => {
            containers::push(checker, owner, *vector, *value, *keyword_span)?
        }
        RawExpressionKind::Match { .. } => matches::finish(checker, owner, index)?,
    };
    checker.tables.function_mut(owner).expressions[index as usize].ty = ty;
    Ok(())
}

fn reference(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
    index: u32,
    name: &zryna_syntax::v4::RawIdentifierSyntax,
    scope: &Scope<'_>,
) -> Result<Option<Ty>, BodyTypeFailure> {
    Ok(if let Some(binding) = scope.get(&name.text) {
        binding.ty
    } else if let Some(target) =
        type_resolution::declaration(checker.context, owner.module(), &name.text)
    {
        if target.kind() == DeclarationKind::Function {
            if checker
                .context
                .declaration(target)
                .expect("original value name")
                .type_parameters()
                .count()
                > 0
            {
                arguments::error(
                    checker,
                    name.span,
                    "generic function value requires explicit arguments",
                );
                None
            } else {
                Some(substitution::issue_expression(
                    checker,
                    owner,
                    index,
                    Head::leaf(Kind::Function(target)),
                )?)
            }
        } else {
            value_names::missing(checker, name);
            None
        }
    } else {
        value_names::missing(checker, name);
        None
    })
}
