use super::{
    Errors, NormalizedSourcePath, RawStatementKind, RawStatementSyntax, SourceMap, checked_span,
    identifier, own_type, token,
};

#[allow(clippy::too_many_arguments)]
pub(in crate::v4) fn verify_statement(
    raw: &RawStatementSyntax,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    type_owners: &mut [u32],
    expression_count: usize,
    expression_owners: &mut [u32],
    block_owners: &mut [u32],
    errors: &mut Errors,
) {
    checked_span(raw.span, file, path, sources, errors, "statement");
    let mut bad_expression = false;
    let mut bad_block = false;
    let mut expression = |id| {
        if !own_expression_root(id, expression_count, expression_owners) {
            bad_expression = true;
        }
    };
    let mut block = |id| {
        if !own_block(id, block_owners) {
            bad_block = true;
        }
    };
    match &raw.kind {
        RawStatementKind::LocalDeclaration {
            keyword_span,
            mutable,
            name,
            type_syntax,
            equals_span,
            initializer,
            semicolon_span,
        } => {
            token(
                *keyword_span,
                file,
                path,
                sources,
                errors,
                if *mutable { "let" } else { "const" },
                "local keyword",
            );
            identifier(name, file, path, sources, errors, "local name");
            own_type(*type_syntax, type_owners, path, errors);
            token(*equals_span, file, path, sources, errors, "=", "initializer equals");
            expression(*initializer);
            token(*semicolon_span, file, path, sources, errors, ";", "local semicolon");
        }
        RawStatementKind::Assignment { target, equals_span, value, semicolon_span } => {
            expression(*target);
            token(*equals_span, file, path, sources, errors, "=", "assignment equals");
            expression(*value);
            token(*semicolon_span, file, path, sources, errors, ";", "assignment semicolon");
        }
        RawStatementKind::Return { keyword_span, value, semicolon_span } => {
            token(*keyword_span, file, path, sources, errors, "return", "return keyword");
            expression(*value);
            token(*semicolon_span, file, path, sources, errors, ";", "return semicolon");
        }
        RawStatementKind::Block { block: id } => block(*id),
        RawStatementKind::If {
            keyword_span,
            open_paren_span,
            condition,
            close_paren_span,
            then_block,
            else_clause,
        } => {
            token(*keyword_span, file, path, sources, errors, "if", "if keyword");
            token(*open_paren_span, file, path, sources, errors, "(", "if open parenthesis");
            expression(*condition);
            token(*close_paren_span, file, path, sources, errors, ")", "if close parenthesis");
            block(*then_block);
            if let Some(value) = else_clause {
                token(value.keyword_span, file, path, sources, errors, "else", "else keyword");
                block(value.block);
            }
        }
        RawStatementKind::While {
            keyword_span,
            open_paren_span,
            condition,
            close_paren_span,
            body_block,
        } => {
            token(*keyword_span, file, path, sources, errors, "while", "while keyword");
            token(*open_paren_span, file, path, sources, errors, "(", "while open parenthesis");
            expression(*condition);
            token(*close_paren_span, file, path, sources, errors, ")", "while close parenthesis");
            block(*body_block);
        }
        RawStatementKind::ExpressionStatement { expression: id, semicolon_span } => {
            expression(*id);
            token(*semicolon_span, file, path, sources, errors, ";", "expression semicolon");
        }
        RawStatementKind::WeakUpgrade {
            keyword_span,
            weak,
            as_span,
            binding,
            success_block,
            else_span,
            failure_block,
        } => {
            token(
                *keyword_span,
                file,
                path,
                sources,
                errors,
                "upgradeWeak",
                "weak-upgrade keyword",
            );
            expression(*weak);
            token(*as_span, file, path, sources, errors, "=>", "success arrow");
            identifier(binding, file, path, sources, errors, "weak-upgrade binding");
            block(*success_block);
            token(*else_span, file, path, sources, errors, "=>", "failure arrow");
            block(*failure_block);
        }
    }
    drop(expression);
    drop(block);
    if bad_expression {
        errors.node(path, "statement references an unknown expression");
    }
    if bad_block {
        errors.node(path, "statement references an unknown block");
    }
}

pub(in crate::v4) fn own_expression_root(id: u32, count: usize, owners: &mut [u32]) -> bool {
    let Some(index) = usize::try_from(id).ok() else {
        return false;
    };
    if index >= count {
        false
    } else {
        owners[index] = owners[index].saturating_add(1);
        true
    }
}

pub(in crate::v4) fn own_block(id: u32, owners: &mut [u32]) -> bool {
    match usize::try_from(id).ok().and_then(|i| owners.get_mut(i)) {
        Some(owner) => {
            *owner = owner.saturating_add(1);
            true
        }
        None => false,
    }
}
