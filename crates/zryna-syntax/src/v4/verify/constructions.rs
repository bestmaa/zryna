use super::{
    BTreeSet, Errors, NormalizedSourcePath, RawExpressionKind, RawFieldInitializerKind, SourceMap,
    checked_span, identifier, own_type, token,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_construction<F: FnMut(u32)>(
    kind: &RawExpressionKind,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    type_owners: &mut [u32],
    edge: &mut F,
    errors: &mut Errors,
) {
    match kind {
        RawExpressionKind::StructConstruction {
            type_name,
            open_paren_span,
            open_brace_span,
            fields,
            close_brace_span,
            close_paren_span,
        } => {
            identifier(type_name, file, path, sources, errors, "struct construction type");
            token(
                *open_paren_span,
                file,
                path,
                sources,
                errors,
                "(",
                "construction open parenthesis",
            );
            token(*open_brace_span, file, path, sources, errors, "{", "construction open brace");
            let mut names = BTreeSet::new();
            for field in fields {
                checked_span(field.span, file, path, sources, errors, "field initializer");
                match &field.kind {
                    RawFieldInitializerKind::Shorthand { name, value } => {
                        identifier(name, file, path, sources, errors, "initializer name");
                        if !names.insert(name.text.as_str()) {
                            errors.node(path, "duplicate initializer name");
                        }
                        edge(*value);
                    }
                    RawFieldInitializerKind::Explicit { name, colon_span, value } => {
                        identifier(name, file, path, sources, errors, "initializer name");
                        if !names.insert(name.text.as_str()) {
                            errors.node(path, "duplicate initializer name");
                        }
                        token(*colon_span, file, path, sources, errors, ":", "initializer colon");
                        edge(*value);
                    }
                }
            }
            token(*close_brace_span, file, path, sources, errors, "}", "construction close brace");
            token(
                *close_paren_span,
                file,
                path,
                sources,
                errors,
                ")",
                "construction close parenthesis",
            );
        }
        RawExpressionKind::EnumConstruction {
            type_name,
            dot_span,
            variant,
            open_paren_span,
            payload,
            close_paren_span,
        } => {
            identifier(type_name, file, path, sources, errors, "enum construction type");
            token(*dot_span, file, path, sources, errors, ".", "enum construction dot");
            identifier(variant, file, path, sources, errors, "enum construction variant");
            token(
                *open_paren_span,
                file,
                path,
                sources,
                errors,
                "(",
                "construction open parenthesis",
            );
            if let Some(id) = payload {
                edge(*id);
            }
            token(
                *close_paren_span,
                file,
                path,
                sources,
                errors,
                ")",
                "construction close parenthesis",
            );
        }
        RawExpressionKind::FixedArrayConstruction {
            type_syntax,
            open_paren_span,
            open_bracket_span,
            elements,
            close_bracket_span,
            close_paren_span,
        }
        | RawExpressionKind::VecConstruction {
            type_syntax,
            open_paren_span,
            open_bracket_span,
            elements,
            close_bracket_span,
            close_paren_span,
        } => {
            own_type(*type_syntax, type_owners, path, errors);
            token(
                *open_paren_span,
                file,
                path,
                sources,
                errors,
                "(",
                "typed construction open parenthesis",
            );
            token(*open_bracket_span, file, path, sources, errors, "[", "array open bracket");
            for id in elements {
                edge(*id);
            }
            token(*close_bracket_span, file, path, sources, errors, "]", "array close bracket");
            token(
                *close_paren_span,
                file,
                path,
                sources,
                errors,
                ")",
                "typed construction close parenthesis",
            );
        }
        _ => {}
    }
}
