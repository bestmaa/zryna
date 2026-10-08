use super::{
    Errors, NormalizedSourcePath, RawExpressionKind, RawExpressionSyntax, SourceMap, UntrustedSpan,
    checked_span, identifier, span_text, token, verify_construction, verify_match,
};

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(in crate::v4) fn verify_expression(
    raw: &RawExpressionSyntax,
    index: usize,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    type_owners: &mut [u32],
    owners: &mut [u32],
    depths: &mut [u32],
    errors: &mut Errors,
) {
    let span = checked_span(raw.span, file, path, sources, errors, "expression");
    let mut edge_failures = Vec::new();
    let mut edge = |id| {
        if let Err(message) = expression_edge(id, index, owners, depths) {
            edge_failures.push(message);
        }
    };
    match &raw.kind {
        RawExpressionKind::Reference { name } => {
            identifier(name, file, path, sources, errors, "reference")
        }
        RawExpressionKind::BoolLiteral { value } => {
            if span.and_then(|s| span_text(s, sources))
                != Some(if *value { "true" } else { "false" })
            {
                errors.node(path, "Boolean literal disagrees with authoritative source");
            }
        }
        RawExpressionKind::I32Literal { spelling } => {
            if !canonical_i32(spelling)
                || span.and_then(|s| span_text(s, sources)) != Some(spelling.as_str())
            {
                errors.node(path, "i32 literal is not canonical or source faithful");
            }
        }
        RawExpressionKind::StringLiteral { spelling } => {
            if !canonical_string_literal(spelling)
                || span.and_then(|s| span_text(s, sources)) != Some(spelling.as_str())
            {
                errors.node(path, "string literal is not canonical or source faithful");
            }
        }
        RawExpressionKind::Negation { operator_span, operand } => {
            token(*operator_span, file, path, sources, errors, "-", "negation operator");
            edge(*operand);
        }
        RawExpressionKind::Addition { operator_span, lhs, rhs } => {
            binary(*operator_span, "+", *lhs, *rhs, file, path, sources, &mut edge, errors)
        }
        RawExpressionKind::Subtraction { operator_span, lhs, rhs } => {
            binary(*operator_span, "-", *lhs, *rhs, file, path, sources, &mut edge, errors)
        }
        RawExpressionKind::Multiplication { operator_span, lhs, rhs } => {
            binary(*operator_span, "*", *lhs, *rhs, file, path, sources, &mut edge, errors)
        }
        RawExpressionKind::Equal { operator_span, lhs, rhs } => {
            binary(*operator_span, "===", *lhs, *rhs, file, path, sources, &mut edge, errors)
        }
        RawExpressionKind::NotEqual { operator_span, lhs, rhs } => {
            binary(*operator_span, "!==", *lhs, *rhs, file, path, sources, &mut edge, errors)
        }
        RawExpressionKind::LessThan { operator_span, lhs, rhs } => {
            binary(*operator_span, "<", *lhs, *rhs, file, path, sources, &mut edge, errors)
        }
        RawExpressionKind::LessEqual { operator_span, lhs, rhs } => {
            binary(*operator_span, "<=", *lhs, *rhs, file, path, sources, &mut edge, errors)
        }
        RawExpressionKind::GreaterThan { operator_span, lhs, rhs } => {
            binary(*operator_span, ">", *lhs, *rhs, file, path, sources, &mut edge, errors)
        }
        RawExpressionKind::GreaterEqual { operator_span, lhs, rhs } => {
            binary(*operator_span, ">=", *lhs, *rhs, file, path, sources, &mut edge, errors)
        }
        RawExpressionKind::Call { callee, open_paren_span, arguments, close_paren_span } => {
            identifier(callee, file, path, sources, errors, "call callee");
            token(*open_paren_span, file, path, sources, errors, "(", "call open parenthesis");
            for id in arguments {
                edge(*id);
            }
            token(*close_paren_span, file, path, sources, errors, ")", "call close parenthesis");
        }
        kind @ (RawExpressionKind::StructConstruction { .. }
        | RawExpressionKind::EnumConstruction { .. }
        | RawExpressionKind::FixedArrayConstruction { .. }
        | RawExpressionKind::VecConstruction { .. }) => {
            verify_construction(kind, file, path, sources, type_owners, &mut edge, errors);
        }
        RawExpressionKind::FieldAccess { base, dot_span, field } => {
            edge(*base);
            token(*dot_span, file, path, sources, errors, ".", "field access dot");
            identifier(field, file, path, sources, errors, "field access name");
        }
        RawExpressionKind::Index { base, open_bracket_span, index, close_bracket_span } => {
            edge(*base);
            token(*open_bracket_span, file, path, sources, errors, "[", "index open bracket");
            edge(*index);
            token(*close_bracket_span, file, path, sources, errors, "]", "index close bracket");
        }
        RawExpressionKind::Clone { keyword_span, open_paren_span, value, close_paren_span } => {
            unary(
                "clone",
                *keyword_span,
                *open_paren_span,
                *value,
                *close_paren_span,
                file,
                path,
                sources,
                &mut edge,
                errors,
            )
        }
        RawExpressionKind::Shared { keyword_span, open_paren_span, value, close_paren_span } => {
            unary(
                "shared",
                *keyword_span,
                *open_paren_span,
                *value,
                *close_paren_span,
                file,
                path,
                sources,
                &mut edge,
                errors,
            )
        }
        RawExpressionKind::Downgrade { keyword_span, open_paren_span, value, close_paren_span } => {
            unary(
                "downgrade",
                *keyword_span,
                *open_paren_span,
                *value,
                *close_paren_span,
                file,
                path,
                sources,
                &mut edge,
                errors,
            )
        }
        RawExpressionKind::Borrow { keyword_span, open_paren_span, value, close_paren_span } => {
            unary(
                "borrow",
                *keyword_span,
                *open_paren_span,
                *value,
                *close_paren_span,
                file,
                path,
                sources,
                &mut edge,
                errors,
            )
        }
        RawExpressionKind::BorrowMut { keyword_span, open_paren_span, value, close_paren_span } => {
            unary(
                "borrowMut",
                *keyword_span,
                *open_paren_span,
                *value,
                *close_paren_span,
                file,
                path,
                sources,
                &mut edge,
                errors,
            )
        }
        RawExpressionKind::VecPush {
            keyword_span,
            open_paren_span,
            vector,
            comma_span,
            value,
            close_paren_span,
        } => {
            token(*keyword_span, file, path, sources, errors, "push", "push keyword");
            token(*open_paren_span, file, path, sources, errors, "(", "push open parenthesis");
            edge(*vector);
            token(*comma_span, file, path, sources, errors, ",", "push comma");
            edge(*value);
            token(*close_paren_span, file, path, sources, errors, ")", "push close parenthesis");
        }
        kind @ RawExpressionKind::Match { .. } => {
            verify_match(kind, file, path, sources, &mut edge, errors);
        }
    }
    drop(edge);
    for message in edge_failures {
        errors.node(path, message);
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::v4) fn binary<F: FnMut(u32)>(
    operator: UntrustedSpan,
    expected: &str,
    lhs: u32,
    rhs: u32,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    edge: &mut F,
    errors: &mut Errors,
) {
    token(operator, file, path, sources, errors, expected, "binary operator");
    edge(lhs);
    edge(rhs);
}

#[allow(clippy::too_many_arguments)]
pub(in crate::v4) fn unary<F: FnMut(u32)>(
    expected: &str,
    keyword: UntrustedSpan,
    open: UntrustedSpan,
    value: u32,
    close: UntrustedSpan,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    edge: &mut F,
    errors: &mut Errors,
) {
    token(keyword, file, path, sources, errors, expected, "ownership intrinsic");
    token(open, file, path, sources, errors, "(", "intrinsic open parenthesis");
    edge(value);
    token(close, file, path, sources, errors, ")", "intrinsic close parenthesis");
}

pub(in crate::v4) fn expression_edge(
    raw: u32,
    parent: usize,
    owners: &mut [u32],
    depths: &mut [u32],
) -> Result<(), &'static str> {
    let child = usize::try_from(raw).map_err(|_| "expression edge references an unknown node")?;
    if child >= parent {
        return Err("expression edge is not canonical postorder");
    }
    let owner = owners.get_mut(child).ok_or("expression edge references an unknown node")?;
    *owner = owner.saturating_add(1);
    depths[parent] = depths[parent].max(depths[child].saturating_add(1));
    Ok(())
}

pub(in crate::v4) fn canonical_i32(value: &str) -> bool {
    if value.is_empty() || value.len() > 64 || !value.is_ascii() {
        return false;
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    (digits == "0" || (!digits.starts_with('0') && digits.bytes().all(|b| b.is_ascii_digit())))
        && value != "-0"
}

pub(in crate::v4) fn canonical_string_literal(value: &str) -> bool {
    let Some(first) = value.chars().next() else {
        return false;
    };
    (first == '\'' || first == '"')
        && value.ends_with(first)
        && value.len() >= 2
        && !value[1..value.len() - 1]
            .chars()
            .any(|c| c == first || c == '\\' || c == '\r' || c == '\n')
}
