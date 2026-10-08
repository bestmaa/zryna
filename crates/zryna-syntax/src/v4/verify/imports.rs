use super::{
    BTreeSet, Errors, NormalizedSourcePath, RawImportSyntax, SourceMap, checked_span, identifier,
    require_claim_contains, require_claim_order, span_text, token,
};

pub(in crate::v4) fn verify_import(
    raw: &RawImportSyntax,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    errors: &mut Errors,
) {
    checked_span(raw.span, file, path, sources, errors, "import");
    token(raw.import_span, file, path, sources, errors, "import", "import keyword");
    token(raw.from_span, file, path, sources, errors, "from", "from keyword");
    token(raw.semicolon_span, file, path, sources, errors, ";", "import semicolon");
    let mut names = BTreeSet::new();
    let mut ordered_children = vec![raw.import_span];
    for binding in &raw.bindings {
        checked_span(binding.span, file, path, sources, errors, "import binding");
        identifier(&binding.imported, file, path, sources, errors, "imported name");
        identifier(&binding.local, file, path, sources, errors, "local import name");
        if !names.insert(binding.local.text.as_str()) {
            errors.node(path, "duplicate local import name");
        }
        match binding.as_span {
            Some(span) => {
                token(span, file, path, sources, errors, "as", "as keyword");
            }
            None if binding.imported != binding.local => {
                errors.node(path, "unaliased import must repeat the exact identifier")
            }
            None => {}
        }
        for child in [Some(binding.imported.span), binding.as_span, Some(binding.local.span)]
            .into_iter()
            .flatten()
        {
            require_claim_contains(binding.span, child, path, errors, "import binding child");
        }
        if let Some(as_span) = binding.as_span {
            require_claim_order(
                &[binding.imported.span, as_span, binding.local.span],
                path,
                errors,
                "import binding tokens",
            );
        }
        ordered_children.push(binding.span);
    }
    let spec = &raw.specifier;
    let Some(token_span) =
        checked_span(spec.token_span, file, path, sources, errors, "module specifier token")
    else {
        return;
    };
    let Some(value_span) =
        checked_span(spec.value_span, file, path, sources, errors, "module specifier value")
    else {
        return;
    };
    if span_text(value_span, sources) != Some(spec.text.as_str()) || !valid_specifier(&spec.text) {
        errors.node(path, "module specifier is not canonical explicit-relative .zry syntax");
    }
    let double = format!("\"{}\"", spec.text);
    let single = format!("'{}'", spec.text);
    if !span_text(token_span, sources).is_some_and(|text| text == double || text == single) {
        errors.node(path, "module specifier token disagrees with authoritative source");
    }
    require_claim_contains(raw.span, spec.token_span, path, errors, "module specifier token");
    require_claim_contains(
        spec.token_span,
        spec.value_span,
        path,
        errors,
        "module specifier value",
    );
    ordered_children.extend([raw.from_span, spec.token_span, raw.semicolon_span]);
    for child in &ordered_children {
        require_claim_contains(raw.span, *child, path, errors, "import child");
    }
    require_claim_order(&ordered_children, path, errors, "import children");
}

pub(in crate::v4) fn valid_specifier(text: &str) -> bool {
    (text.starts_with("./") || text.starts_with("../"))
        && text.ends_with(".zry")
        && text.len() <= 1024
        && text.is_ascii()
        && !text.contains("//")
        && !text.contains(['\\', '?', '#'])
        && !text.contains("://")
}
