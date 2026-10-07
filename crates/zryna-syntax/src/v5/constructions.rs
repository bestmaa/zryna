//! Typed construction spelling and explicit source occurrence aliases.

use super::{
    DeclarationError, RawExpressionKind as Kind, RawFunctionBodySyntax, RawSourceUnit, arena,
    coverage::{Context, Cursor, Role},
    expression_source::{child, node},
};
use crate::v4::RawFieldInitializerKind;

pub(super) fn validate(
    cursor: &mut Cursor<'_>,
    unit: &RawSourceUnit,
    body: &RawFunctionBodySyntax,
    kind: &Kind,
    context: Context,
) -> Result<(), DeclarationError> {
    match kind {
        Kind::StructConstruction {
            type_name,
            type_arguments,
            open_paren_span,
            open_brace_span,
            fields,
            close_brace_span,
            close_paren_span,
        } => {
            if matches!(
                type_name.text.as_str(),
                "match"
                    | "upgradeWeak"
                    | "clone"
                    | "shared"
                    | "downgrade"
                    | "borrow"
                    | "borrowMut"
                    | "push"
                    | "Vec"
                    | "FixedArray"
            ) {
                return Err(arena::malformed());
            }
            context.identifier(cursor, type_name, Role::Runtime)?;
            cursor.type_arguments(unit, type_arguments.as_ref())?;
            cursor.token(*open_paren_span, "(")?;
            cursor.token(*open_brace_span, "{")?;
            validate_fields(cursor, body, fields, context)?;
            cursor.token(*close_brace_span, "}")?;
            cursor.token(*close_paren_span, ")")?;
        }
        Kind::EnumConstruction {
            type_name,
            dot_span,
            variant,
            type_arguments,
            open_paren_span,
            payload,
            close_paren_span,
        } => {
            context.identifier(cursor, type_name, Role::Runtime)?;
            cursor.token(*dot_span, ".")?;
            cursor.identifier(variant)?;
            cursor.type_arguments(unit, type_arguments.as_ref())?;
            cursor.token(*open_paren_span, "(")?;
            if let Some(id) = payload {
                child(cursor, body, *id)?;
                cursor.comma()?;
            }
            cursor.token(*close_paren_span, ")")?;
        }
        Kind::VecConstruction {
            type_syntax,
            open_paren_span,
            open_bracket_span,
            elements,
            close_bracket_span,
            close_paren_span,
        }
        | Kind::FixedArrayConstruction {
            type_syntax,
            open_paren_span,
            open_bracket_span,
            elements,
            close_bracket_span,
            close_paren_span,
        } => {
            let ty = unit.type_syntax.get(*type_syntax as usize).ok_or_else(arena::malformed)?;
            if !matches!(
                (kind, &ty.kind),
                (Kind::VecConstruction { .. }, super::RawTypeSyntaxKind::Vec { .. })
                    | (
                        Kind::FixedArrayConstruction { .. },
                        super::RawTypeSyntaxKind::FixedArray { .. }
                    )
            ) {
                return Err(arena::malformed());
            }
            cursor.annotation(unit, *type_syntax)?;
            cursor.token(*open_paren_span, "(")?;
            cursor.token(*open_bracket_span, "[")?;
            for (index, id) in elements.iter().enumerate() {
                if index != 0 {
                    cursor.punctuation(",")?;
                }
                child(cursor, body, *id)?;
            }
            if !elements.is_empty() {
                cursor.comma()?;
            }
            cursor.token(*close_bracket_span, "]")?;
            cursor.token(*close_paren_span, ")")?;
        }
        _ => return Err(arena::malformed()),
    }
    Ok(())
}

fn validate_fields(
    cursor: &mut Cursor<'_>,
    body: &RawFunctionBodySyntax,
    fields: &[crate::v4::RawFieldInitializer],
    context: Context,
) -> Result<(), DeclarationError> {
    for (index, field) in fields.iter().enumerate() {
        if index != 0 {
            cursor.punctuation(",")?;
        }
        cursor.bound(field.span)?;
        let (name, value) = match &field.kind {
            RawFieldInitializerKind::Shorthand { name, value } => {
                let expression = node(body, *value)?;
                if expression.span != name.span
                    || !matches!(&expression.kind, Kind::Reference { name: reference } if reference == name)
                {
                    return Err(arena::malformed());
                }
                context.identifier(cursor, name, Role::Runtime)?;
                (name, *value)
            }
            RawFieldInitializerKind::Explicit { name, colon_span, value } => {
                cursor.identifier(name)?;
                cursor.token(*colon_span, ":")?;
                child(cursor, body, *value)?;
                (name, *value)
            }
        };
        if field.span.start != name.span.start || field.span.end != node(body, value)?.span.end {
            return Err(arena::malformed());
        }
    }
    if !fields.is_empty() {
        cursor.comma()?;
    }
    Ok(())
}
