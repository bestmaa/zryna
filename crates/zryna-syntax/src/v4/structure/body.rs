use super::{
    Errors, NormalizedSourcePath, RawFunctionBodySyntax, RawFunctionSyntax, RawSourceUnit,
    RawStatementKind, RawStatementSyntax, RawTypeSyntax, UntrustedSpan, check_sequence,
    checked_index, verify_declaration_structure, verify_expression_structure,
    verify_type_structure,
};

pub(in crate::v4) fn verify_file_structure(
    raw: &RawSourceUnit,
    path: &NormalizedSourcePath,
    errors: &mut Errors,
) {
    for (index, node) in raw.type_syntax.iter().enumerate() {
        verify_type_structure(node, index, &raw.type_syntax, path, errors);
    }
    for declaration in &raw.data_declarations {
        verify_declaration_structure(declaration, &raw.type_syntax, path, errors);
    }
    for function in &raw.functions {
        verify_function_structure(function, &raw.type_syntax, path, errors);
    }
}

pub(in crate::v4) fn verify_function_structure(
    raw: &RawFunctionSyntax,
    types: &[RawTypeSyntax],
    path: &NormalizedSourcePath,
    errors: &mut Errors,
) {
    let mut children = raw.export_span.into_iter().collect::<Vec<_>>();
    children.extend([raw.function_span, raw.name.span]);
    for parameter in &raw.parameters {
        let mut inner = vec![parameter.name.span];
        if let Some(ty) = checked_index(types, parameter.type_syntax) {
            inner.push(ty.span);
        }
        check_sequence(parameter.span, &inner, path, errors, "parameter children");
        children.push(parameter.span);
    }
    if let Some(result) = checked_index(types, raw.result_type) {
        children.push(result.span);
    }
    children.push(raw.body.span);
    check_sequence(raw.span, &children, path, errors, "function children");
    verify_body_structure(&raw.body, types, path, errors);
}

pub(in crate::v4) fn verify_body_structure(
    raw: &RawFunctionBodySyntax,
    types: &[RawTypeSyntax],
    path: &NormalizedSourcePath,
    errors: &mut Errors,
) {
    for block in &raw.blocks {
        let mut children = vec![block.open_brace_span];
        for statement in &block.statements {
            if let Some(statement) = checked_index(&raw.statements, *statement) {
                children.push(statement.span);
            }
        }
        children.push(block.close_brace_span);
        check_sequence(block.span, &children, path, errors, "block children");
    }
    for statement in &raw.statements {
        verify_statement_structure(statement, raw, types, path, errors);
    }
    for expression in &raw.expressions {
        verify_expression_structure(expression, raw, types, path, errors);
    }
}

pub(in crate::v4) fn expression_span(
    body: &RawFunctionBodySyntax,
    id: u32,
) -> Option<UntrustedSpan> {
    checked_index(&body.expressions, id).map(|value| value.span)
}

pub(in crate::v4) fn block_span(body: &RawFunctionBodySyntax, id: u32) -> Option<UntrustedSpan> {
    checked_index(&body.blocks, id).map(|value| value.span)
}

pub(in crate::v4) fn verify_statement_structure(
    raw: &RawStatementSyntax,
    body: &RawFunctionBodySyntax,
    types: &[RawTypeSyntax],
    path: &NormalizedSourcePath,
    errors: &mut Errors,
) {
    let children = match &raw.kind {
        RawStatementKind::LocalDeclaration {
            keyword_span,
            name,
            type_syntax,
            equals_span,
            initializer,
            semicolon_span,
            ..
        } => {
            let mut out = vec![*keyword_span, name.span];
            if let Some(ty) = checked_index(types, *type_syntax) {
                out.push(ty.span);
            }
            out.push(*equals_span);
            if let Some(span) = expression_span(body, *initializer) {
                out.push(span);
            }
            out.push(*semicolon_span);
            out
        }
        RawStatementKind::Assignment { target, equals_span, value, semicolon_span } => [
            expression_span(body, *target),
            Some(*equals_span),
            expression_span(body, *value),
            Some(*semicolon_span),
        ]
        .into_iter()
        .flatten()
        .collect(),
        RawStatementKind::Return { keyword_span, value, semicolon_span } => {
            [Some(*keyword_span), expression_span(body, *value), Some(*semicolon_span)]
                .into_iter()
                .flatten()
                .collect()
        }
        RawStatementKind::Block { block } => block_span(body, *block).into_iter().collect(),
        RawStatementKind::If {
            keyword_span,
            open_paren_span,
            condition,
            close_paren_span,
            then_block,
            else_clause,
        } => {
            let mut out = [
                Some(*keyword_span),
                Some(*open_paren_span),
                expression_span(body, *condition),
                Some(*close_paren_span),
                block_span(body, *then_block),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            if let Some(value) = else_clause {
                out.push(value.keyword_span);
                if let Some(span) = block_span(body, value.block) {
                    out.push(span);
                }
            }
            out
        }
        RawStatementKind::While {
            keyword_span,
            open_paren_span,
            condition,
            close_paren_span,
            body_block,
        } => [
            Some(*keyword_span),
            Some(*open_paren_span),
            expression_span(body, *condition),
            Some(*close_paren_span),
            block_span(body, *body_block),
        ]
        .into_iter()
        .flatten()
        .collect(),
        RawStatementKind::ExpressionStatement { expression, semicolon_span } => {
            [expression_span(body, *expression), Some(*semicolon_span)]
                .into_iter()
                .flatten()
                .collect()
        }
        RawStatementKind::WeakUpgrade {
            keyword_span,
            weak,
            binding,
            as_span,
            success_block,
            else_span,
            failure_block,
        } => [
            Some(*keyword_span),
            expression_span(body, *weak),
            Some(binding.span),
            Some(*as_span),
            block_span(body, *success_block),
            Some(*else_span),
            block_span(body, *failure_block),
        ]
        .into_iter()
        .flatten()
        .collect(),
    };
    check_sequence(raw.span, &children, path, errors, "statement children");
}
