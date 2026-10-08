use super::{
    BTreeSet, Errors, MAX_NESTING_DEPTH, NormalizedSourcePath, RawExpressionKind,
    RawFunctionBodySyntax, RawFunctionSyntax, RawStatementKind, SourceMap, checked_span,
    contains_claim, expression_children, identifier, is_place, lexical_bindings, own_type,
    statement_expression_roots, token, verify_arena_order, verify_expression, verify_statement,
};

pub(in crate::v4) fn verify_function(
    raw: &RawFunctionSyntax,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    type_owners: &mut [u32],
    top_names: &mut BTreeSet<String>,
    errors: &mut Errors,
) {
    checked_span(raw.span, file, path, sources, errors, "function");
    if let Some(span) = raw.export_span {
        token(span, file, path, sources, errors, "export", "export keyword");
    }
    token(raw.function_span, file, path, sources, errors, "function", "function keyword");
    identifier(&raw.name, file, path, sources, errors, "function name");
    if !contains_claim(raw.span, raw.function_span)
        || !contains_claim(raw.span, raw.name.span)
        || !contains_claim(raw.span, raw.body.span)
    {
        errors.node(path, "function child span is outside its owner span");
    }
    if !top_names.insert(raw.name.text.clone()) {
        errors.node(path, "duplicate top-level declaration name");
    }
    let mut locals = BTreeSet::new();
    for parameter in &raw.parameters {
        checked_span(parameter.span, file, path, sources, errors, "parameter");
        identifier(&parameter.name, file, path, sources, errors, "parameter name");
        if !locals.insert(parameter.name.text.clone()) {
            errors.node(path, "duplicate parameter name");
        }
        own_type(parameter.type_syntax, type_owners, path, errors);
    }
    own_type(raw.result_type, type_owners, path, errors);
    verify_body(&raw.body, file, path, sources, type_owners, &mut locals, errors);
}

pub(in crate::v4) fn verify_body(
    raw: &RawFunctionBodySyntax,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    type_owners: &mut [u32],
    locals: &mut BTreeSet<String>,
    errors: &mut Errors,
) {
    checked_span(raw.span, file, path, sources, errors, "function body");
    if raw.root_block != 0 {
        errors.node(path, "root block id is not zero");
    }
    let mut block_owners = vec![0u32; raw.blocks.len()];
    if let Some(root) = block_owners.get_mut(0) {
        *root = 1;
    }
    let mut statement_owners = vec![0u32; raw.statements.len()];
    for block in &raw.blocks {
        checked_span(block.span, file, path, sources, errors, "block");
        token(block.open_brace_span, file, path, sources, errors, "{", "block open brace");
        token(block.close_brace_span, file, path, sources, errors, "}", "block close brace");
        for id in &block.statements {
            match usize::try_from(*id).ok().and_then(|i| statement_owners.get_mut(i)) {
                Some(owner) => *owner = owner.saturating_add(1),
                None => errors.node(path, "block references an unknown statement"),
            }
        }
    }
    let mut expression_owners = vec![0u32; raw.expressions.len()];
    let mut expression_depths = vec![1u32; raw.expressions.len()];
    for (index, expression) in raw.expressions.iter().enumerate() {
        verify_expression(
            expression,
            index,
            file,
            path,
            sources,
            type_owners,
            &mut expression_owners,
            &mut expression_depths,
            errors,
        );
    }
    for expression in &raw.expressions {
        match &expression.kind {
            RawExpressionKind::Borrow { value, .. }
            | RawExpressionKind::BorrowMut { value, .. }
                if !is_place(&raw.expressions, *value) =>
            {
                errors.node(path, "borrow operand is not syntactically a place")
            }
            RawExpressionKind::VecPush { vector, .. } if !is_place(&raw.expressions, *vector) => {
                errors.node(path, "push target is not syntactically a place")
            }
            _ => {}
        }
    }
    for statement in &raw.statements {
        verify_statement(
            statement,
            file,
            path,
            sources,
            type_owners,
            raw.expressions.len(),
            &mut expression_owners,
            &mut block_owners,
            errors,
        );
    }
    lexical_bindings::verify(raw, locals, path, errors);
    for block in &raw.blocks {
        if !contains_claim(raw.span, block.span) {
            errors.node(path, "block span is outside its function body");
        }
        for statement in &block.statements {
            if let Some(statement) =
                usize::try_from(*statement).ok().and_then(|id| raw.statements.get(id))
                && !contains_claim(block.span, statement.span)
            {
                errors.node(path, "statement span is outside its owning block");
            }
        }
    }
    for expression in &raw.expressions {
        if !contains_claim(raw.span, expression.span) {
            errors.node(path, "expression span is outside its function body");
        }
        for child in expression_children(&expression.kind) {
            if let Some(child) = usize::try_from(child).ok().and_then(|id| raw.expressions.get(id))
                && !contains_claim(expression.span, child.span)
            {
                errors.node(path, "expression child span is outside its parent expression");
            }
        }
    }
    for statement in &raw.statements {
        for root in statement_expression_roots(&statement.kind) {
            if let Some(root) = usize::try_from(root).ok().and_then(|id| raw.expressions.get(id))
                && !contains_claim(statement.span, root.span)
            {
                errors.node(path, "root expression span is outside its owning statement");
            }
        }
    }
    for statement in &raw.statements {
        if let RawStatementKind::Assignment { target, .. } = statement.kind {
            if !is_place(&raw.expressions, target) {
                errors.node(path, "assignment target is not syntactically a place");
            }
        }
    }
    if block_owners.iter().any(|owners| *owners != 1) {
        errors.node(path, "block arena has a shared or orphan block");
    }
    if statement_owners.iter().any(|owners| *owners != 1) {
        errors.node(path, "statement arena has a shared or orphan statement");
    }
    if expression_owners.iter().any(|owners| *owners != 1) {
        errors.node(path, "expression arena has a shared or orphan expression");
    }
    if expression_depths.iter().any(|depth| *depth > MAX_NESTING_DEPTH) {
        errors.limit("expression nesting exceeds the protocol-v4 limit");
    }
    verify_arena_order(raw, path, errors);
}
