use super::{Errors, NormalizedSourcePath, RawIdentifierSyntax, SourceMap, Span, UntrustedSpan};

pub(in crate::v4) fn ordered(
    raw: UntrustedSpan,
    previous_end: &mut u32,
    path: &NormalizedSourcePath,
    errors: &mut Errors,
    label: &str,
) {
    if raw.start < *previous_end {
        errors.node(path, format!("{label} is not in canonical source order"));
    }
    *previous_end = raw.end;
}

pub(in crate::v4) fn checked_span(
    raw: UntrustedSpan,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    errors: &mut Errors,
    label: &str,
) -> Option<Span> {
    if raw.file != file {
        errors.node(path, format!("{label} uses the wrong file id"));
        return None;
    }
    match sources.verify_span(raw) {
        Ok(span) => Some(span),
        Err(_) => {
            errors.node(path, format!("{label} span is invalid"));
            None
        }
    }
}

pub(in crate::v4) fn span_text(span: Span, sources: &SourceMap) -> Option<&str> {
    let resolved = sources.resolve(span).ok()?;
    let start = usize::try_from(span.start()).ok()?;
    let end = usize::try_from(span.end()).ok()?;
    resolved.source().text().get(start..end)
}

pub(in crate::v4) fn token(
    raw: UntrustedSpan,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    errors: &mut Errors,
    expected: &str,
    label: &str,
) -> Option<Span> {
    let span = checked_span(raw, file, path, sources, errors, label)?;
    if span_text(span, sources) != Some(expected) {
        errors.node(path, format!("{label} spelling disagrees with authoritative source"));
    }
    Some(span)
}

pub(in crate::v4) fn identifier(
    raw: &RawIdentifierSyntax,
    file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    errors: &mut Errors,
    label: &str,
) {
    let Some(span) = checked_span(raw.span, file, path, sources, errors, label) else {
        return;
    };
    if !valid_identifier(&raw.text) || is_sensitive(&raw.text) {
        errors.node(path, format!("{label} is forbidden or exceeds its bound"));
    }
    if span_text(span, sources) != Some(raw.text.as_str()) {
        errors.node(path, format!("{label} spelling disagrees with authoritative source"));
    }
}

pub(in crate::v4) fn is_sensitive(text: &str) -> bool {
    matches!(text, "__proto__" | "prototype" | "constructor")
}

pub(in crate::v4) fn valid_identifier(text: &str) -> bool {
    if text.is_empty() || text.len() > 128 || !text.is_ascii() {
        return false;
    }
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
