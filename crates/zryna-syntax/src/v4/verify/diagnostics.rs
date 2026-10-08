use super::{
    Diagnostic, Errors, RawDiagnosticLocation, RawProviderDiagnostic, Severity, SourceMap,
};

pub(in crate::v4) fn verify_provider_diagnostics(
    raw: Vec<RawProviderDiagnostic>,
    sources: &SourceMap,
    errors: &mut Errors,
) -> Vec<Diagnostic> {
    let mut output = Vec::new();
    for value in raw {
        if value.code.is_empty()
            || value.code.chars().count() > 1024
            || value.message.is_empty()
            || value.message.chars().count() > 4096
            || value.guidance.is_empty()
            || value.guidance.chars().count() > 4096
        {
            errors.limit("provider diagnostic text exceeds its protocol-v4 limit");
            continue;
        }
        let span = match value.location {
            RawDiagnosticLocation::Global => None,
            RawDiagnosticLocation::Source { span } => match sources.verify_span(span) {
                Ok(span) => Some(span),
                Err(_) => {
                    errors.protocol(None, "provider diagnostic span is invalid");
                    continue;
                }
            },
        };
        output.push(match (value.severity, span) {
            (Severity::Error, Some(span)) => {
                Diagnostic::error_at(value.code, span, value.message, value.guidance)
            }
            (Severity::Warning, Some(span)) => {
                Diagnostic::warning_at(value.code, span, value.message, value.guidance)
            }
            (Severity::Error, None) => {
                Diagnostic::error(value.code, None, value.message, value.guidance)
            }
            (Severity::Warning, None) => {
                Diagnostic::warning(value.code, None, value.message, value.guidance)
            }
        });
    }
    output.sort_by_key(ToString::to_string);
    output
}
