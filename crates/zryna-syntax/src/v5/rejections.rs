//! Provider rejection text is advisory and cannot replace complete source verification.

use super::{DeclarationError, RawProjectSyntaxSnapshot, arena};
use crate::v4::RawDiagnosticLocation;
use zryna_source::SourceMap;

pub(super) fn validate_advisories(
    sources: &SourceMap,
    raw: &RawProjectSyntaxSnapshot,
) -> Result<(), DeclarationError> {
    for diagnostic in &raw.diagnostics {
        for (text, maximum) in
            [(&diagnostic.code, 1_024), (&diagnostic.message, 4_096), (&diagnostic.guidance, 4_096)]
        {
            if text.is_empty()
                || text.len() > maximum * 4
                || text.chars().take(maximum + 1).count() > maximum
            {
                return Err(arena::malformed());
            }
        }
        if diagnostic.code.chars().any(char::is_control) {
            return Err(arena::malformed());
        }
        if let RawDiagnosticLocation::Source { span } = &diagnostic.location {
            sources.verify_span(*span).map_err(|_| arena::malformed())?;
        }
    }
    Ok(())
}
