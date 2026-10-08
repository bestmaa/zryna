use serde::Serialize;
use std::path::Path;
use zryna_diagnostics::Diagnostic;

pub(super) const MAX_SCAN_DIAGNOSTICS: usize = 256;
/// Complete result of one architecture check.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationReport {
    /// Stable diagnostics in deterministic order.
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// Whether the architecture is valid.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

#[derive(Default)]
pub(super) struct ValidationDiagnostics {
    pub(super) values: Vec<Diagnostic>,
    halted: bool,
}

impl ValidationDiagnostics {
    pub(super) fn push(&mut self, diagnostic: Diagnostic) {
        if self.halted {
            return;
        }
        if self.values.len().saturating_add(1) >= MAX_SCAN_DIAGNOSTICS {
            self.halt(architecture_error(
                "ZRYNA-A1204",
                None,
                "architecture validation exceeded its deterministic diagnostic budget",
                "reduce invalid controlled input; incomplete validation never passes",
            ));
            return;
        }
        self.values.push(diagnostic);
    }

    pub(super) fn halt(&mut self, diagnostic: Diagnostic) {
        if self.halted {
            return;
        }
        if self.values.len() >= MAX_SCAN_DIAGNOSTICS {
            self.values.truncate(MAX_SCAN_DIAGNOSTICS.saturating_sub(1));
        }
        self.values.push(diagnostic);
        self.halted = true;
    }

    pub(super) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.values.len()
    }

    pub(super) const fn is_halted(&self) -> bool {
        self.halted
    }

    pub(super) fn into_vec(self) -> Vec<Diagnostic> {
        self.values
    }
}

pub(super) fn validation_report(diagnostics: ValidationDiagnostics) -> ValidationReport {
    let mut values = diagnostics.into_vec();
    values.sort_by(|left, right| {
        (left.code(), left.path(), left.message()).cmp(&(
            right.code(),
            right.path(),
            right.message(),
        ))
    });
    ValidationReport { diagnostics: values }
}

pub(super) fn architecture_error(
    code: &str,
    path: Option<&Path>,
    message: impl Into<String>,
    guidance: impl Into<String>,
) -> Diagnostic {
    Diagnostic::error(
        code,
        path.map(|value| value.to_string_lossy().replace('\\', "/")),
        message,
        guidance,
    )
}
