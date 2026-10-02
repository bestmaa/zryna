//! Exact quoted keys and expression arrows; coverage is not semantic exhaustiveness.

use super::{
    DeclarationError, RawExpressionKind, RawFunctionBodySyntax, arena,
    coverage::{Context, Cursor, Role},
    expression_source::{child, node},
};

pub(super) fn validate(
    cursor: &mut Cursor<'_>,
    body: &RawFunctionBodySyntax,
    kind: &RawExpressionKind,
    context: Context,
) -> Result<(), DeclarationError> {
    let RawExpressionKind::Match {
        keyword_span,
        open_paren_span,
        scrutinee,
        close_paren_span,
        open_brace_span,
        arms,
        close_brace_span,
    } = kind
    else {
        return Err(arena::malformed());
    };
    cursor.token(*keyword_span, "match")?;
    cursor.token(*open_paren_span, "(")?;
    child(cursor, body, *scrutinee)?;
    cursor.punctuation(",")?;
    cursor.token(*open_brace_span, "{")?;
    for (index, arm) in arms.iter().enumerate() {
        if index != 0 {
            cursor.punctuation(",")?;
        }
        cursor.bound(arm.span)?;
        cursor.bound(arm.type_name.span)?;
        cursor.bound(arm.dot_span)?;
        cursor.bound(arm.variant.span)?;
        if arm.span.start.checked_add(1) != Some(arm.type_name.span.start)
            || arm.type_name.span.end != arm.dot_span.start
            || arm.dot_span.end != arm.variant.span.start
        {
            return Err(arena::malformed());
        }
        let mut item = Cursor::new_from(cursor, arm.span)?;
        item.punctuation("\"")?;
        item.identifier(&arm.type_name)?;
        item.token(arm.dot_span, ".")?;
        item.identifier(&arm.variant)?;
        let quote = zryna_source::UntrustedSpan {
            file: arm.span.file,
            start: arm.variant.span.end,
            end: arm.variant.span.end.checked_add(1).ok_or_else(arena::malformed)?,
        };
        item.token(quote, "\"")?;
        item.punctuation(":")?;
        item.punctuation("(")?;
        if let Some(binding) = &arm.binding {
            context.identifier(&mut item, binding, Role::ValueBinding)?;
        }
        item.punctuation(")")?;
        item.token(arm.arrow_span, "=>")?;
        child(&mut item, body, arm.value)?;
        item.finish()?;
        if arm.span.end != node(body, arm.value)?.span.end {
            return Err(arena::malformed());
        }
        cursor.child(arm.span)?;
    }
    if !arms.is_empty() {
        cursor.comma()?;
    }
    cursor.token(*close_brace_span, "}")?;
    cursor.token(*close_paren_span, ")")
}
