use super::{
    Errors, NormalizedSourcePath, RawExpressionKind, RawExpressionSyntax, RawFieldInitializerKind,
    RawFunctionBodySyntax, RawTypeSyntax, check_sequence, checked_index, expression_span,
    require_claim_contains,
};

pub(in crate::v4) fn verify_expression_structure(
    raw: &RawExpressionSyntax,
    body: &RawFunctionBodySyntax,
    types: &[RawTypeSyntax],
    path: &NormalizedSourcePath,
    errors: &mut Errors,
) {
    let child = |id| expression_span(body, id);
    let children = match &raw.kind {
        RawExpressionKind::Reference { name } => vec![name.span],
        RawExpressionKind::BoolLiteral { .. }
        | RawExpressionKind::I32Literal { .. }
        | RawExpressionKind::StringLiteral { .. } => Vec::new(),
        RawExpressionKind::Negation { operator_span, operand } => {
            [Some(*operator_span), child(*operand)].into_iter().flatten().collect()
        }
        RawExpressionKind::Addition { operator_span, lhs, rhs }
        | RawExpressionKind::Subtraction { operator_span, lhs, rhs }
        | RawExpressionKind::Multiplication { operator_span, lhs, rhs }
        | RawExpressionKind::Equal { operator_span, lhs, rhs }
        | RawExpressionKind::NotEqual { operator_span, lhs, rhs }
        | RawExpressionKind::LessThan { operator_span, lhs, rhs }
        | RawExpressionKind::LessEqual { operator_span, lhs, rhs }
        | RawExpressionKind::GreaterThan { operator_span, lhs, rhs }
        | RawExpressionKind::GreaterEqual { operator_span, lhs, rhs } => {
            [child(*lhs), Some(*operator_span), child(*rhs)].into_iter().flatten().collect()
        }
        RawExpressionKind::Call { callee, open_paren_span, arguments, close_paren_span } => {
            let mut out = vec![callee.span, *open_paren_span];
            out.extend(arguments.iter().filter_map(|id| child(*id)));
            out.push(*close_paren_span);
            out
        }
        RawExpressionKind::StructConstruction {
            type_name,
            open_paren_span,
            open_brace_span,
            fields,
            close_brace_span,
            close_paren_span,
        } => {
            let mut out = vec![type_name.span, *open_paren_span, *open_brace_span];
            for field in fields {
                match &field.kind {
                    RawFieldInitializerKind::Shorthand { name, value } => {
                        require_claim_contains(
                            field.span,
                            name.span,
                            path,
                            errors,
                            "shorthand initializer name",
                        );
                        if let Some(value) = child(*value) {
                            require_claim_contains(
                                field.span,
                                value,
                                path,
                                errors,
                                "shorthand initializer value",
                            );
                        }
                    }
                    RawFieldInitializerKind::Explicit { name, colon_span, value } => {
                        let inner = [Some(name.span), Some(*colon_span), child(*value)]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>();
                        check_sequence(
                            field.span,
                            &inner,
                            path,
                            errors,
                            "explicit initializer children",
                        );
                    }
                }
                out.push(field.span);
            }
            out.extend([*close_brace_span, *close_paren_span]);
            out
        }
        RawExpressionKind::EnumConstruction {
            type_name,
            dot_span,
            variant,
            open_paren_span,
            payload,
            close_paren_span,
        } => {
            let mut out = vec![type_name.span, *dot_span, variant.span, *open_paren_span];
            out.extend(payload.iter().filter_map(|id| child(*id)));
            out.push(*close_paren_span);
            out
        }
        RawExpressionKind::FixedArrayConstruction {
            type_syntax,
            open_paren_span,
            open_bracket_span,
            elements,
            close_bracket_span,
            close_paren_span,
            ..
        }
        | RawExpressionKind::VecConstruction {
            type_syntax,
            open_paren_span,
            open_bracket_span,
            elements,
            close_bracket_span,
            close_paren_span,
            ..
        } => {
            let mut out =
                checked_index(types, *type_syntax).map_or_else(Vec::new, |ty| vec![ty.span]);
            out.extend([*open_paren_span, *open_bracket_span]);
            out.extend(elements.iter().filter_map(|id| child(*id)));
            out.extend([*close_bracket_span, *close_paren_span]);
            out
        }
        RawExpressionKind::FieldAccess { base, dot_span, field } => {
            [child(*base), Some(*dot_span), Some(field.span)].into_iter().flatten().collect()
        }
        RawExpressionKind::Index { base, open_bracket_span, index, close_bracket_span } => {
            [child(*base), Some(*open_bracket_span), child(*index), Some(*close_bracket_span)]
                .into_iter()
                .flatten()
                .collect()
        }
        RawExpressionKind::Clone { keyword_span, open_paren_span, value, close_paren_span }
        | RawExpressionKind::Shared { keyword_span, open_paren_span, value, close_paren_span }
        | RawExpressionKind::Downgrade { keyword_span, open_paren_span, value, close_paren_span }
        | RawExpressionKind::Borrow { keyword_span, open_paren_span, value, close_paren_span }
        | RawExpressionKind::BorrowMut { keyword_span, open_paren_span, value, close_paren_span } => {
            [Some(*keyword_span), Some(*open_paren_span), child(*value), Some(*close_paren_span)]
                .into_iter()
                .flatten()
                .collect()
        }
        RawExpressionKind::VecPush {
            keyword_span,
            open_paren_span,
            vector,
            comma_span,
            value,
            close_paren_span,
        } => [
            Some(*keyword_span),
            Some(*open_paren_span),
            child(*vector),
            Some(*comma_span),
            child(*value),
            Some(*close_paren_span),
        ]
        .into_iter()
        .flatten()
        .collect(),
        RawExpressionKind::Match {
            keyword_span,
            open_paren_span,
            scrutinee,
            close_paren_span,
            open_brace_span,
            arms,
            close_brace_span,
        } => {
            let mut out = [
                Some(*keyword_span),
                Some(*open_paren_span),
                child(*scrutinee),
                Some(*open_brace_span),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            for arm in arms {
                let mut inner = vec![arm.type_name.span, arm.dot_span, arm.variant.span];
                if let Some(binding) = &arm.binding {
                    inner.push(binding.span);
                }
                inner.push(arm.arrow_span);
                if let Some(value) = child(arm.value) {
                    inner.push(value);
                }
                check_sequence(arm.span, &inner, path, errors, "match arm children");
                out.push(arm.span);
            }
            out.push(*close_brace_span);
            out.push(*close_paren_span);
            out
        }
    };
    check_sequence(raw.span, &children, path, errors, "expression children");
}
