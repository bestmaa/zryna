//! Full statement spelling, including delimiters absent from the wire leaf records.

use super::{
    DeclarationError, RawFunctionBodySyntax, RawSourceUnit, arena,
    coverage::{self, Context, Cursor, Role},
    expression_source::{child, node},
};
use crate::v4::{RawStatementKind as Kind, RawStatementSyntax};
use zryna_source::{SourceMap, UntrustedSpan};

pub(super) fn roots(kind: &Kind) -> Vec<u32> {
    match kind {
        Kind::LocalDeclaration { initializer, .. } => vec![*initializer],
        Kind::Assignment { target, value, .. } => vec![*target, *value],
        Kind::Return { value, .. } => vec![*value],
        Kind::If { condition, .. } | Kind::While { condition, .. } => vec![*condition],
        Kind::ExpressionStatement { expression, .. } => vec![*expression],
        Kind::WeakUpgrade { weak, .. } => vec![*weak],
        Kind::Block { .. } => Vec::new(),
    }
}

pub(super) fn blocks(kind: &Kind) -> Vec<u32> {
    match kind {
        Kind::Block { block } => vec![*block],
        Kind::If { then_block, else_clause, .. } => std::iter::once(*then_block)
            .chain(else_clause.iter().map(|clause| clause.block))
            .collect(),
        Kind::While { body_block, .. } => vec![*body_block],
        Kind::WeakUpgrade { success_block, failure_block, .. } => {
            vec![*success_block, *failure_block]
        }
        _ => Vec::new(),
    }
}

fn block_span(body: &RawFunctionBodySyntax, id: u32) -> Result<UntrustedSpan, DeclarationError> {
    body.blocks.get(id as usize).map(|block| block.span).ok_or_else(arena::malformed)
}

fn place(body: &RawFunctionBodySyntax, mut id: u32) -> Result<(), DeclarationError> {
    for _ in 0..=crate::v4::MAX_NESTING_DEPTH {
        match &node(body, id)?.kind {
            super::RawExpressionKind::Reference { .. } => return Ok(()),
            super::RawExpressionKind::FieldAccess { base, .. }
            | super::RawExpressionKind::Index { base, .. } => id = *base,
            _ => return Err(arena::malformed()),
        }
    }
    Err(arena::malformed())
}

pub(super) fn validate(
    sources: &SourceMap,
    unit: &RawSourceUnit,
    body: &RawFunctionBodySyntax,
    statement: &RawStatementSyntax,
    context: Context,
) -> Result<(), DeclarationError> {
    let mut cursor = Cursor::new(sources, statement.span)?;
    if statement.span.file != unit.id {
        return Err(arena::malformed());
    }
    let endpoints = endpoints(&cursor, body, statement)?;
    if endpoints != (statement.span.start, statement.span.end) {
        return Err(arena::malformed());
    }
    match &statement.kind {
        Kind::LocalDeclaration {
            keyword_span,
            mutable,
            name,
            type_syntax,
            equals_span,
            initializer,
            semicolon_span,
        } => {
            cursor.token(*keyword_span, if *mutable { "let" } else { "const" })?;
            context.identifier(&mut cursor, name, Role::LocalBinding)?;
            if !coverage::missing(unit, *type_syntax)? {
                cursor.punctuation(":")?;
            }
            cursor.annotation(unit, *type_syntax)?;
            cursor.token(*equals_span, "=")?;
            child(&mut cursor, body, *initializer)?;
            cursor.token(*semicolon_span, ";")?;
        }
        Kind::Assignment { target, equals_span, value, semicolon_span } => {
            place(body, *target)?;
            if let super::RawExpressionKind::Reference { name } = &node(body, *target)?.kind
                && !context.allows(&name.text, Role::Assignment)
            {
                return Err(DeclarationError::malformed(Some(cursor.bound(name.span)?)));
            }
            child(&mut cursor, body, *target)?;
            cursor.token(*equals_span, "=")?;
            child(&mut cursor, body, *value)?;
            cursor.token(*semicolon_span, ";")?;
        }
        Kind::Return { keyword_span, value, semicolon_span } => {
            cursor.token(*keyword_span, "return")?;
            let gap = UntrustedSpan {
                file: keyword_span.file,
                start: keyword_span.end,
                end: node(body, *value)?.span.start,
            };
            if cursor.text(gap)?.chars().any(super::source::line_terminator) {
                return Err(DeclarationError::malformed(Some(cursor.bound(gap)?)));
            }
            child(&mut cursor, body, *value)?;
            cursor.token(*semicolon_span, ";")?;
        }
        Kind::Block { block } => cursor.child(block_span(body, *block)?)?,
        Kind::If {
            keyword_span,
            open_paren_span,
            condition,
            close_paren_span,
            then_block,
            else_clause,
        } => {
            cursor.token(*keyword_span, "if")?;
            cursor.token(*open_paren_span, "(")?;
            child(&mut cursor, body, *condition)?;
            cursor.token(*close_paren_span, ")")?;
            cursor.child(block_span(body, *then_block)?)?;
            if let Some(clause) = else_clause {
                cursor.token(clause.keyword_span, "else")?;
                cursor.child(block_span(body, clause.block)?)?;
            }
        }
        Kind::While { keyword_span, open_paren_span, condition, close_paren_span, body_block } => {
            cursor.token(*keyword_span, "while")?;
            cursor.token(*open_paren_span, "(")?;
            child(&mut cursor, body, *condition)?;
            cursor.token(*close_paren_span, ")")?;
            cursor.child(block_span(body, *body_block)?)?;
        }
        Kind::ExpressionStatement { expression, semicolon_span } => {
            child(&mut cursor, body, *expression)?;
            cursor.token(*semicolon_span, ";")?;
        }
        Kind::WeakUpgrade { .. } => {
            upgrade(&mut cursor, body, context, &statement.kind)?;
        }
    }
    cursor.finish()
}

fn endpoints(
    cursor: &Cursor<'_>,
    body: &RawFunctionBodySyntax,
    statement: &RawStatementSyntax,
) -> Result<(u32, u32), DeclarationError> {
    Ok(match &statement.kind {
        Kind::LocalDeclaration { keyword_span, semicolon_span, .. }
        | Kind::Return { keyword_span, semicolon_span, .. } => {
            (keyword_span.start, semicolon_span.end)
        }
        Kind::Assignment { target, semicolon_span, .. } => {
            (node(body, *target)?.span.start, semicolon_span.end)
        }
        Kind::ExpressionStatement { expression, semicolon_span } => {
            (node(body, *expression)?.span.start, semicolon_span.end)
        }
        Kind::Block { block } => {
            let span = block_span(body, *block)?;
            (span.start, span.end)
        }
        Kind::If { keyword_span, then_block, else_clause, .. } => (
            keyword_span.start,
            block_span(body, else_clause.as_ref().map_or(*then_block, |clause| clause.block))?.end,
        ),
        Kind::While { keyword_span, body_block, .. } => {
            (keyword_span.start, block_span(body, *body_block)?.end)
        }
        Kind::WeakUpgrade { keyword_span, .. } => {
            if !cursor.text(statement.span)?.ends_with(';') {
                return Err(arena::malformed());
            }
            (keyword_span.start, statement.span.end)
        }
    })
}

fn upgrade(
    cursor: &mut Cursor<'_>,
    body: &RawFunctionBodySyntax,
    context: Context,
    kind: &Kind,
) -> Result<(), DeclarationError> {
    let Kind::WeakUpgrade {
        keyword_span,
        weak,
        as_span,
        binding,
        success_block,
        else_span,
        failure_block,
    } = kind
    else {
        return Err(arena::malformed());
    };
    cursor.token(*keyword_span, "upgradeWeak")?;
    cursor.punctuation("(")?;
    child(cursor, body, *weak)?;
    cursor.punctuation(",")?;
    cursor.punctuation("(")?;
    context.identifier(cursor, binding, Role::ValueBinding)?;
    cursor.punctuation(")")?;
    cursor.token(*as_span, "=>")?;
    cursor.child(block_span(body, *success_block)?)?;
    cursor.punctuation(",")?;
    cursor.punctuation("(")?;
    cursor.punctuation(")")?;
    cursor.token(*else_span, "=>")?;
    cursor.child(block_span(body, *failure_block)?)?;
    cursor.comma()?;
    cursor.punctuation(")")?;
    cursor.punctuation(";")?;
    Ok(())
}
