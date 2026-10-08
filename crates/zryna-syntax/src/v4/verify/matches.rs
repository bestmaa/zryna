use super::{
    BTreeSet, Errors, NormalizedSourcePath, RawExpressionKind, SourceMap, checked_span, identifier,
    token,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_match<F: FnMut(u32)>(
    kind: &RawExpressionKind,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    edge: &mut F,
    errors: &mut Errors,
) {
    if let RawExpressionKind::Match {
        keyword_span,
        open_paren_span,
        scrutinee,
        close_paren_span,
        open_brace_span,
        arms,
        close_brace_span,
    } = kind
    {
        token(*keyword_span, file, path, sources, errors, "match", "match keyword");
        token(*open_paren_span, file, path, sources, errors, "(", "match open parenthesis");
        edge(*scrutinee);
        token(*open_brace_span, file, path, sources, errors, "{", "match open brace");
        let mut seen_arms = BTreeSet::new();
        for arm in arms {
            checked_span(arm.span, file, path, sources, errors, "match arm");
            identifier(&arm.type_name, file, path, sources, errors, "match arm type");
            token(arm.dot_span, file, path, sources, errors, ".", "match arm dot");
            identifier(&arm.variant, file, path, sources, errors, "match arm variant");
            if !seen_arms.insert((arm.type_name.text.as_str(), arm.variant.text.as_str())) {
                errors.node(path, "duplicate qualified match arm");
            }
            if let Some(binding) = &arm.binding {
                identifier(binding, file, path, sources, errors, "match arm binding");
            }
            token(arm.arrow_span, file, path, sources, errors, "=>", "match arm arrow");
            edge(arm.value);
        }
        token(*close_brace_span, file, path, sources, errors, "}", "match close brace");
        token(*close_paren_span, file, path, sources, errors, ")", "match close parenthesis");
    }
}
