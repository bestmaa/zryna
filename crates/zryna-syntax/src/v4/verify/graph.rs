use super::{
    Errors, MAX_NESTING_DEPTH, NormalizedSourcePath, RawExpressionKind, RawExpressionSyntax,
    RawFieldInitializerKind, RawFunctionBodySyntax, RawStatementKind,
};

pub(in crate::v4) fn statement_expression_roots(kind: &RawStatementKind) -> Vec<u32> {
    match kind {
        RawStatementKind::LocalDeclaration { initializer, .. } => vec![*initializer],
        RawStatementKind::Assignment { target, value, .. } => vec![*target, *value],
        RawStatementKind::Return { value, .. } => vec![*value],
        RawStatementKind::If { condition, .. } | RawStatementKind::While { condition, .. } => {
            vec![*condition]
        }
        RawStatementKind::ExpressionStatement { expression, .. } => vec![*expression],
        RawStatementKind::WeakUpgrade { weak, .. } => vec![*weak],
        RawStatementKind::Block { .. } => Vec::new(),
    }
}

pub(in crate::v4) fn expression_children(kind: &RawExpressionKind) -> Vec<u32> {
    match kind {
        RawExpressionKind::Negation { operand, .. } => vec![*operand],
        RawExpressionKind::Addition { lhs, rhs, .. }
        | RawExpressionKind::Subtraction { lhs, rhs, .. }
        | RawExpressionKind::Multiplication { lhs, rhs, .. }
        | RawExpressionKind::Equal { lhs, rhs, .. }
        | RawExpressionKind::NotEqual { lhs, rhs, .. }
        | RawExpressionKind::LessThan { lhs, rhs, .. }
        | RawExpressionKind::LessEqual { lhs, rhs, .. }
        | RawExpressionKind::GreaterThan { lhs, rhs, .. }
        | RawExpressionKind::GreaterEqual { lhs, rhs, .. } => vec![*lhs, *rhs],
        RawExpressionKind::Call { arguments, .. } => arguments.clone(),
        RawExpressionKind::StructConstruction { fields, .. } => fields
            .iter()
            .map(|field| match field.kind {
                RawFieldInitializerKind::Explicit { value, .. }
                | RawFieldInitializerKind::Shorthand { value, .. } => value,
            })
            .collect(),
        RawExpressionKind::EnumConstruction { payload, .. } => payload.iter().copied().collect(),
        RawExpressionKind::FixedArrayConstruction { elements, .. }
        | RawExpressionKind::VecConstruction { elements, .. } => elements.clone(),
        RawExpressionKind::FieldAccess { base, .. } => vec![*base],
        RawExpressionKind::Index { base, index, .. } => vec![*base, *index],
        RawExpressionKind::Clone { value, .. }
        | RawExpressionKind::Shared { value, .. }
        | RawExpressionKind::Downgrade { value, .. }
        | RawExpressionKind::Borrow { value, .. }
        | RawExpressionKind::BorrowMut { value, .. } => vec![*value],
        RawExpressionKind::VecPush { vector, value, .. } => vec![*vector, *value],
        RawExpressionKind::Match { scrutinee, arms, .. } => {
            std::iter::once(*scrutinee).chain(arms.iter().map(|arm| arm.value)).collect()
        }
        RawExpressionKind::Reference { .. }
        | RawExpressionKind::BoolLiteral { .. }
        | RawExpressionKind::I32Literal { .. }
        | RawExpressionKind::StringLiteral { .. } => Vec::new(),
    }
}

pub(in crate::v4) fn is_place(expressions: &[RawExpressionSyntax], id: u32) -> bool {
    let mut current = id;
    for _ in 0..=MAX_NESTING_DEPTH {
        let Some(expression) =
            usize::try_from(current).ok().and_then(|index| expressions.get(index))
        else {
            return false;
        };
        match &expression.kind {
            RawExpressionKind::Reference { .. } => return true,
            RawExpressionKind::FieldAccess { base, .. } | RawExpressionKind::Index { base, .. } => {
                current = *base
            }
            _ => return false,
        }
    }
    false
}

pub(in crate::v4) fn verify_arena_order(
    raw: &RawFunctionBodySyntax,
    path: &NormalizedSourcePath,
    errors: &mut Errors,
) {
    if raw.blocks.is_empty() {
        return;
    }
    let mut expected_block = 0usize;
    let mut expected_statement = 0usize;
    let mut seen = vec![false; raw.blocks.len()];
    let mut stack = vec![(0usize, 0usize, 1u32, false)];
    while let Some((block_id, offset, depth, entered)) = stack.pop() {
        if depth > MAX_NESTING_DEPTH {
            errors.limit("block nesting exceeds the protocol-v4 limit");
            break;
        }
        let Some(block) = raw.blocks.get(block_id) else {
            continue;
        };
        if !entered {
            if seen[block_id] {
                errors.node(path, "block graph has cyclic or shared reachability");
                continue;
            }
            seen[block_id] = true;
            if block_id != expected_block {
                errors.node(path, "block arena is not canonical preorder");
            }
            expected_block = expected_block.saturating_add(1);
        }
        let Some(statement_id) = block.statements.get(offset) else {
            continue;
        };
        stack.push((block_id, offset.saturating_add(1), depth, true));
        let Some(statement_index) = usize::try_from(*statement_id).ok() else {
            continue;
        };
        let Some(statement) = raw.statements.get(statement_index) else {
            continue;
        };
        if statement_index != expected_statement {
            errors.node(path, "statement arena is not canonical preorder");
        }
        expected_statement = expected_statement.saturating_add(1);
        match &statement.kind {
            RawStatementKind::Block { block } => push_block(*block, depth, &mut stack),
            RawStatementKind::If { then_block, else_clause, .. } => {
                if let Some(value) = else_clause {
                    push_block(value.block, depth, &mut stack);
                }
                push_block(*then_block, depth, &mut stack);
            }
            RawStatementKind::While { body_block, .. } => {
                push_block(*body_block, depth, &mut stack)
            }
            RawStatementKind::WeakUpgrade { success_block, failure_block, .. } => {
                push_block(*failure_block, depth, &mut stack);
                push_block(*success_block, depth, &mut stack);
            }
            _ => {}
        }
    }
    if seen.iter().any(|seen| !seen) {
        errors.node(path, "block arena has an unreachable block");
    }
}

pub(in crate::v4) fn push_block(
    raw: u32,
    parent_depth: u32,
    stack: &mut Vec<(usize, usize, u32, bool)>,
) {
    if let Ok(index) = usize::try_from(raw) {
        stack.push((index, 0, parent_depth.saturating_add(1), false));
    }
}
