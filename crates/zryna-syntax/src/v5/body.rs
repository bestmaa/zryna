//! Canonical body ownership and complete source coverage, without name resolution.

use super::{
    DeclarationError, RawFunctionBodySyntax, RawSourceUnit, arena,
    coverage::{Context, Cursor},
    expression_source, statements,
};
use zryna_source::SourceMap;

pub(super) fn validate(
    sources: &SourceMap,
    unit: &RawSourceUnit,
    body: &RawFunctionBodySyntax,
    context: Context,
) -> Result<(), DeclarationError> {
    if body.root_block != 0 || body.blocks.first().is_none_or(|root| root.span != body.span) {
        return Err(arena::malformed());
    }
    let mut block_owners = vec![false; body.blocks.len()];
    let mut statement_owners = vec![false; body.statements.len()];
    let mut next_block = 0;
    let mut next_statement = 0;
    let mut roots = Vec::new();
    let mut root_depths = Vec::new();
    let mut stack = vec![(true, body.root_block, 1u32)];
    while let Some((is_block, id, depth)) = stack.pop() {
        if depth > crate::v4::MAX_NESTING_DEPTH {
            return Err(arena::budget());
        }
        if is_block {
            let index = arena::own(&mut block_owners, id)?;
            if index != next_block {
                return Err(arena::malformed());
            }
            next_block += 1;
            for statement in body.blocks[index].statements.iter().rev() {
                stack.push((false, *statement, depth));
            }
        } else {
            let index = arena::own(&mut statement_owners, id)?;
            if index != next_statement {
                return Err(arena::malformed());
            }
            next_statement += 1;
            let kind = &body.statements[index].kind;
            for root in statements::roots(kind) {
                roots.push(root);
                root_depths.push(depth);
            }
            for block in statements::blocks(kind).into_iter().rev() {
                stack.push((true, block, depth.checked_add(1).ok_or_else(arena::budget)?));
            }
        }
    }
    if next_block != body.blocks.len() || next_statement != body.statements.len() {
        return Err(arena::malformed());
    }
    arena::forest(body.expressions.len(), &roots, |index| {
        expression_source::children(&body.expressions[index].kind)
    })?;
    let mut expression_depths = Vec::with_capacity(body.expressions.len());
    for expression in &body.expressions {
        let mut depth = 1u32;
        for child in expression_source::children(&expression.kind) {
            let child_depth: u32 =
                *expression_depths.get(child as usize).ok_or_else(arena::malformed)?;
            depth = depth.max(child_depth.checked_add(1).ok_or_else(arena::budget)?);
        }
        expression_depths.push(depth);
    }
    for (root, block_depth) in roots.iter().zip(root_depths) {
        if expression_depths[*root as usize].checked_add(block_depth).ok_or_else(arena::budget)?
            > crate::v4::MAX_NESTING_DEPTH
        {
            return Err(arena::budget());
        }
    }
    for block in &body.blocks {
        let mut cursor = Cursor::new(sources, block.span)?;
        if block.span.file != unit.id
            || block.span.start != block.open_brace_span.start
            || block.span.end != block.close_brace_span.end
        {
            return Err(arena::malformed());
        }
        cursor.token(block.open_brace_span, "{")?;
        for id in &block.statements {
            cursor.child(body.statements[*id as usize].span)?;
        }
        cursor.token(block.close_brace_span, "}")?;
        cursor.finish()?;
    }
    for statement in &body.statements {
        statements::validate(sources, unit, body, statement, context)?;
    }
    for expression in &body.expressions {
        expression_source::validate(sources, unit, body, expression, context)?;
    }
    reject_directive(sources, body)
}

fn reject_directive(
    sources: &SourceMap,
    body: &RawFunctionBodySyntax,
) -> Result<(), DeclarationError> {
    // A directive changes identifier roles; the frozen grammar admits no strict directive.
    for id in &body.blocks[0].statements {
        let crate::v4::RawStatementKind::ExpressionStatement { expression, .. } =
            &body.statements[*id as usize].kind
        else {
            break;
        };
        let super::RawExpressionKind::StringLiteral { spelling } =
            &body.expressions[*expression as usize].kind
        else {
            break;
        };
        if matches!(spelling.as_str(), "\"use strict\"" | "'use strict'") {
            let span = sources
                .verify_span(body.expressions[*expression as usize].span)
                .map_err(|_| arena::malformed())?;
            return Err(DeclarationError::malformed(Some(span)));
        }
    }
    Ok(())
}
