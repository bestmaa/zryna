use super::{Deserialize, Serialize, Severity, UntrustedSpan};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawProviderDiagnostic {
    pub code: String,
    pub severity: Severity,
    pub location: RawDiagnosticLocation,
    pub message: String,
    pub guidance: String,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RawDiagnosticLocation {
    Global,
    Source { span: UntrustedSpan },
}
