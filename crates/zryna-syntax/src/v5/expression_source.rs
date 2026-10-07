//! Source spelling, precedence and operand shape for the complete expression arena.

use zryna_source::{SourceMap, UntrustedSpan};

use super::{
    DeclarationError, RawExpressionKind as Kind, RawExpressionSyntax, RawFunctionBodySyntax,
    RawSourceUnit, arena,
    coverage::{Context, Cursor, Role},
};

pub(super) fn children(kind: &Kind) -> Vec<u32> {
    match kind {
        Kind::Negation { operand, .. } => vec![*operand],
        Kind::Addition { lhs, rhs, .. }
        | Kind::Subtraction { lhs, rhs, .. }
        | Kind::Multiplication { lhs, rhs, .. }
        | Kind::Equal { lhs, rhs, .. }
        | Kind::NotEqual { lhs, rhs, .. }
        | Kind::LessThan { lhs, rhs, .. }
        | Kind::LessEqual { lhs, rhs, .. }
        | Kind::GreaterThan { lhs, rhs, .. }
        | Kind::GreaterEqual { lhs, rhs, .. } => vec![*lhs, *rhs],
        Kind::Call { arguments, .. } => arguments.clone(),
        Kind::StructConstruction { fields, .. } => fields
            .iter()
            .map(|field| match &field.kind {
                crate::v4::RawFieldInitializerKind::Shorthand { value, .. }
                | crate::v4::RawFieldInitializerKind::Explicit { value, .. } => *value,
            })
            .collect(),
        Kind::EnumConstruction { payload, .. } => payload.iter().copied().collect(),
        Kind::FixedArrayConstruction { elements, .. } | Kind::VecConstruction { elements, .. } => {
            elements.clone()
        }
        Kind::FieldAccess { base, .. } => vec![*base],
        Kind::Index { base, index, .. } => vec![*base, *index],
        Kind::Clone { value, .. }
        | Kind::Shared { value, .. }
        | Kind::Downgrade { value, .. }
        | Kind::Borrow { value, .. }
        | Kind::BorrowMut { value, .. } => vec![*value],
        Kind::VecPush { vector, value, .. } => vec![*vector, *value],
        Kind::Match { scrutinee, arms, .. } => {
            std::iter::once(*scrutinee).chain(arms.iter().map(|arm| arm.value)).collect()
        }
        _ => Vec::new(),
    }
}

pub(super) fn node(
    body: &RawFunctionBodySyntax,
    id: u32,
) -> Result<&RawExpressionSyntax, DeclarationError> {
    body.expressions.get(id as usize).ok_or_else(arena::malformed)
}

pub(super) fn child(
    cursor: &mut Cursor<'_>,
    body: &RawFunctionBodySyntax,
    id: u32,
) -> Result<(), DeclarationError> {
    cursor.child(node(body, id)?.span)
}

fn binary(kind: &Kind) -> Option<(u8, &str, UntrustedSpan, u32, u32)> {
    match kind {
        Kind::Addition { operator_span, lhs, rhs } => Some((3, "+", *operator_span, *lhs, *rhs)),
        Kind::Subtraction { operator_span, lhs, rhs } => Some((3, "-", *operator_span, *lhs, *rhs)),
        Kind::Multiplication { operator_span, lhs, rhs } => {
            Some((4, "*", *operator_span, *lhs, *rhs))
        }
        Kind::Equal { operator_span, lhs, rhs } => Some((1, "===", *operator_span, *lhs, *rhs)),
        Kind::NotEqual { operator_span, lhs, rhs } => Some((1, "!==", *operator_span, *lhs, *rhs)),
        Kind::LessThan { operator_span, lhs, rhs } => Some((2, "<", *operator_span, *lhs, *rhs)),
        Kind::LessEqual { operator_span, lhs, rhs } => Some((2, "<=", *operator_span, *lhs, *rhs)),
        Kind::GreaterThan { operator_span, lhs, rhs } => Some((2, ">", *operator_span, *lhs, *rhs)),
        Kind::GreaterEqual { operator_span, lhs, rhs } => {
            Some((2, ">=", *operator_span, *lhs, *rhs))
        }
        _ => None,
    }
}

fn primary(kind: &Kind) -> bool {
    binary(kind).is_none() && !matches!(kind, Kind::Negation { .. })
}

fn endpoints(
    unit: &RawSourceUnit,
    body: &RawFunctionBodySyntax,
    expression: &RawExpressionSyntax,
) -> Result<(u32, u32), DeclarationError> {
    if let Some((_, _, _, lhs, rhs)) = binary(&expression.kind) {
        return Ok((node(body, lhs)?.span.start, node(body, rhs)?.span.end));
    }
    Ok(match &expression.kind {
        Kind::Reference { name } => (name.span.start, name.span.end),
        Kind::Negation { operator_span, operand } => {
            (operator_span.start, node(body, *operand)?.span.end)
        }
        Kind::Call { callee, close_paren_span, .. } => (callee.span.start, close_paren_span.end),
        Kind::StructConstruction { type_name, close_paren_span, .. }
        | Kind::EnumConstruction { type_name, close_paren_span, .. } => {
            (type_name.span.start, close_paren_span.end)
        }
        Kind::VecConstruction { type_syntax, close_paren_span, .. }
        | Kind::FixedArrayConstruction { type_syntax, close_paren_span, .. } => {
            (super::types::occurrence(unit, *type_syntax)?.start, close_paren_span.end)
        }
        Kind::FieldAccess { base, field, .. } => (node(body, *base)?.span.start, field.span.end),
        Kind::Index { base, close_bracket_span, .. } => {
            (node(body, *base)?.span.start, close_bracket_span.end)
        }
        Kind::Clone { keyword_span, close_paren_span, .. }
        | Kind::Shared { keyword_span, close_paren_span, .. }
        | Kind::Downgrade { keyword_span, close_paren_span, .. }
        | Kind::Borrow { keyword_span, close_paren_span, .. }
        | Kind::BorrowMut { keyword_span, close_paren_span, .. }
        | Kind::VecPush { keyword_span, close_paren_span, .. }
        | Kind::Match { keyword_span, close_paren_span, .. } => {
            (keyword_span.start, close_paren_span.end)
        }
        _ => (expression.span.start, expression.span.end),
    })
}

pub(super) fn validate(
    sources: &SourceMap,
    unit: &RawSourceUnit,
    body: &RawFunctionBodySyntax,
    expression: &RawExpressionSyntax,
    context: Context,
) -> Result<(), DeclarationError> {
    let mut cursor = Cursor::new(sources, expression.span)?;
    if expression.span.file != unit.id {
        return Err(arena::malformed());
    }
    if endpoints(unit, body, expression)? != (expression.span.start, expression.span.end) {
        return Err(arena::malformed());
    }
    if binary(&expression.kind).is_some() {
        return validate_binary(cursor, body, &expression.kind);
    }
    match &expression.kind {
        Kind::Reference { name } => {
            context.identifier(&mut cursor, name, Role::Runtime)?;
        }
        Kind::BoolLiteral { value } => {
            cursor.token(expression.span, if *value { "true" } else { "false" })?;
        }
        Kind::I32Literal { spelling } | Kind::StringLiteral { spelling } => {
            literal(&mut cursor, expression, spelling)?;
        }
        Kind::Negation { operator_span, operand } => {
            let operand = node(body, *operand)?;
            if binary(&operand.kind).is_some() {
                return Err(arena::malformed());
            }
            let adjacent_forbidden = matches!(&operand.kind, Kind::Negation { .. })
                || matches!(&operand.kind, Kind::I32Literal { spelling } if spelling != "0");
            if operator_span.end == operand.span.start && adjacent_forbidden {
                return Err(arena::malformed());
            }
            cursor.token(*operator_span, "-")?;
            cursor.child(operand.span)?;
        }
        Kind::Call { .. } => call(&mut cursor, unit, body, &expression.kind, context)?,
        Kind::FieldAccess { base, dot_span, field } => {
            if !primary(&node(body, *base)?.kind) {
                return Err(arena::malformed());
            }
            child(&mut cursor, body, *base)?;
            cursor.token(*dot_span, ".")?;
            cursor.identifier(field)?;
        }
        Kind::Index { base, open_bracket_span, index, close_bracket_span } => {
            if !primary(&node(body, *base)?.kind) {
                return Err(arena::malformed());
            }
            child(&mut cursor, body, *base)?;
            cursor.token(*open_bracket_span, "[")?;
            child(&mut cursor, body, *index)?;
            cursor.token(*close_bracket_span, "]")?;
        }
        Kind::Clone { keyword_span, open_paren_span, value, close_paren_span }
        | Kind::Shared { keyword_span, open_paren_span, value, close_paren_span }
        | Kind::Downgrade { keyword_span, open_paren_span, value, close_paren_span }
        | Kind::Borrow { keyword_span, open_paren_span, value, close_paren_span }
        | Kind::BorrowMut { keyword_span, open_paren_span, value, close_paren_span } => {
            unary(
                &mut cursor,
                body,
                &expression.kind,
                *keyword_span,
                *open_paren_span,
                *value,
                *close_paren_span,
            )?;
        }
        Kind::VecPush {
            keyword_span,
            open_paren_span,
            vector,
            comma_span,
            value,
            close_paren_span,
        } => {
            cursor.token(*keyword_span, "push")?;
            cursor.token(*open_paren_span, "(")?;
            child(&mut cursor, body, *vector)?;
            cursor.token(*comma_span, ",")?;
            child(&mut cursor, body, *value)?;
            cursor.token(*close_paren_span, ")")?;
        }
        Kind::Match { .. } => {
            super::matches::validate(&mut cursor, body, &expression.kind, context)?;
        }
        Kind::StructConstruction { .. }
        | Kind::EnumConstruction { .. }
        | Kind::VecConstruction { .. }
        | Kind::FixedArrayConstruction { .. } => {
            super::constructions::validate(&mut cursor, unit, body, &expression.kind, context)?;
        }
        _ => return Err(arena::malformed()),
    }
    cursor.finish()
}

fn call(
    cursor: &mut Cursor<'_>,
    unit: &RawSourceUnit,
    body: &RawFunctionBodySyntax,
    kind: &Kind,
    context: Context,
) -> Result<(), DeclarationError> {
    let Kind::Call { callee, type_arguments, open_paren_span, arguments, close_paren_span } = kind
    else {
        return Err(arena::malformed());
    };
    if matches!(
        callee.text.as_str(),
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
    context.identifier(cursor, callee, Role::Runtime)?;
    cursor.type_arguments(unit, type_arguments.as_ref())?;
    cursor.token(*open_paren_span, "(")?;
    for (index, id) in arguments.iter().enumerate() {
        if index != 0 {
            cursor.punctuation(",")?;
        }
        child(cursor, body, *id)?;
    }
    if !arguments.is_empty() {
        cursor.comma()?;
    }
    cursor.token(*close_paren_span, ")")?;
    Ok(())
}

fn unary(
    cursor: &mut Cursor<'_>,
    body: &RawFunctionBodySyntax,
    kind: &Kind,
    keyword: UntrustedSpan,
    open: UntrustedSpan,
    value: u32,
    close: UntrustedSpan,
) -> Result<(), DeclarationError> {
    let spelling = match kind {
        Kind::Clone { .. } => "clone",
        Kind::Shared { .. } => "shared",
        Kind::Downgrade { .. } => "downgrade",
        Kind::Borrow { .. } => "borrow",
        _ => "borrowMut",
    };
    cursor.token(keyword, spelling)?;
    cursor.token(open, "(")?;
    child(cursor, body, value)?;
    cursor.token(close, ")")?;
    Ok(())
}

fn validate_binary(
    mut cursor: Cursor<'_>,
    body: &RawFunctionBodySyntax,
    kind: &Kind,
) -> Result<(), DeclarationError> {
    let Some((precedence, spelling, operator, lhs, rhs)) = binary(kind) else {
        return Err(arena::malformed());
    };
    let right = node(body, rhs)?;
    if spelling == "-"
        && operator.end == right.span.start
        && cursor.text(right.span)?.starts_with('-')
    {
        return Err(arena::malformed());
    }
    for (id, right) in [(lhs, false), (rhs, true)] {
        if binary(&node(body, id)?.kind)
            .is_some_and(|(child, ..)| child < precedence || (right && child == precedence))
        {
            return Err(arena::malformed());
        }
    }
    child(&mut cursor, body, lhs)?;
    cursor.token(operator, spelling)?;
    child(&mut cursor, body, rhs)?;
    cursor.finish()
}

fn literal(
    cursor: &mut Cursor<'_>,
    expression: &RawExpressionSyntax,
    spelling: &str,
) -> Result<(), DeclarationError> {
    cursor.token(expression.span, spelling)?;
    let kind = if matches!(expression.kind, Kind::I32Literal { .. }) {
        "i32-literal"
    } else {
        "string-literal"
    };
    if !super::wire::valid(&serde_json::json!({ "kind": kind, "spelling": spelling })) {
        return Err(arena::malformed());
    }
    Ok(())
}
