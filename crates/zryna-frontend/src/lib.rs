//! Replaceable frontend-provider contracts.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zryna_diagnostics::{Diagnostic, Severity};
use zryna_source::{
    FileId, MAX_SOURCE_FILES, NormalizedSourcePath, SourceMap, Span, UntrustedSpan,
};

pub mod native_lexer;
pub mod native_parser;
mod worker;
pub use worker::{
    FrontendCapabilitiesV3, FrontendCapabilitiesV4, MAX_HANDSHAKE_RESPONSE_BYTES,
    MAX_WORKER_REQUEST_BYTES, MAX_WORKER_STDERR_BYTES, MAX_WORKER_STDOUT_BYTES,
    MAX_WORKER_STDOUT_BYTES_V3, MAX_WORKER_STDOUT_BYTES_V4, MAX_WORKER_TIMEOUT, MIN_WORKER_TIMEOUT,
    ProviderExpectation, ProviderExpectationV3, ProviderExpectationV4, ProviderInfoV3,
    ProviderInfoV4, VerifiedFrontendProvider, VerifiedFrontendProviderV3,
    VerifiedFrontendProviderV4, WorkerError, WorkerFailure, WorkerFrontend, WorkerFrontendV3,
    WorkerFrontendV4, WorkerLimits, WorkerLimitsV3, WorkerLimitsV4, WorkerSpec, WorkerSpecV3,
    WorkerSpecV4,
};
/// Provider-neutral executable syntax contract spoken by protocol-v2 providers.
pub use zryna_syntax::v2 as syntax_v2;
/// Provider-neutral M2 syntax contract spoken only by exact protocol-v3 providers.
pub use zryna_syntax::v3 as syntax_v3;
/// Provider-neutral M3 ownership syntax spoken only by exact protocol-v4 providers.
pub use zryna_syntax::v4 as syntax_v4;

/// Current wire contract understood by the compiler.
pub const FRONTEND_PROTOCOL_VERSION: u32 = 1;
/// Maximum provider diagnostics accepted in one snapshot.
pub const MAX_PROVIDER_DIAGNOSTICS: usize = 256;
/// Maximum functions accepted in one source unit for protocol v1.
pub const MAX_FUNCTIONS_PER_FILE: usize = 4_096;
/// Maximum aggregate functions inspected in one provider snapshot.
pub const MAX_FUNCTIONS_PER_SNAPSHOT: usize = 16_384;
/// Maximum aggregate parameters inspected in one provider snapshot.
pub const MAX_PARAMETERS_PER_SNAPSHOT: usize = 262_144;
/// Maximum parameters accepted in one function for protocol v1.
pub const MAX_PARAMETERS_PER_FUNCTION: usize = 256;
/// Maximum UTF-8 bytes in one provider identifier or type spelling.
pub const MAX_PROVIDER_NAME_BYTES: usize = 1_024;
/// Maximum UTF-8 bytes in one provider diagnostic message or guidance value.
pub const MAX_PROVIDER_DIAGNOSTIC_TEXT_BYTES: usize = 4_096;
/// Maximum validation diagnostics retained for one hostile snapshot.
pub const MAX_SNAPSHOT_VALIDATION_ERRORS: usize = 256;
/// Maximum serialized bytes accepted from one provider response.
pub const MAX_PROVIDER_RESPONSE_BYTES: usize = 16 * 1_024 * 1_024;

/// Capabilities advertised by a frontend provider.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrontendCapabilities {
    /// Whether the provider resolves module specifiers.
    pub module_resolution: bool,
    /// Whether the provider supplies basic semantic diagnostics.
    pub semantic_diagnostics: bool,
}

/// Provider identity returned during handshake.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderInfo {
    /// Stable provider name.
    pub provider: String,
    /// Exact upstream provider version.
    pub provider_version: String,
    /// ZRYNA-owned protocol version.
    pub protocol_version: u32,
    /// Explicit provider capabilities.
    pub capabilities: FrontendCapabilities,
}

/// One source file supplied for analysis.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInput {
    /// Normalized workspace-relative path.
    pub path: String,
    /// UTF-8 source text.
    pub text: String,
}

/// Project analysis request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyzeRequest {
    /// Request schema version.
    pub schema_version: u32,
    /// Complete bounded source set.
    pub files: Vec<SourceInput>,
}

/// Normalized type spelling retained for the Zryna semantic checker.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TypeSyntax {
    /// No explicit annotation was present.
    Inferred,
    /// A named type reference such as `i32`.
    Named(String),
}

/// Untrusted function declaration received from a provider.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawFunctionSyntax {
    /// Function name.
    pub name: String,
    /// Parameter names and annotations.
    pub parameters: Vec<(String, TypeSyntax)>,
    /// Declared return type.
    pub return_type: TypeSyntax,
    /// Source range for diagnostics.
    pub span: UntrustedSpan,
}

/// Untrusted source unit received from a provider.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawSourceUnit {
    /// Untrusted snapshot-local identifier.
    pub id: u32,
    /// Untrusted path spelling.
    pub path: String,
    /// Provider declarations currently supported by protocol v1.
    pub functions: Vec<RawFunctionSyntax>,
}

/// Untrusted provider diagnostic for protocol v1.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawProviderDiagnostic {
    /// Provider-selected stable code.
    pub code: String,
    /// Provider severity.
    pub severity: Severity,
    /// Optional source path.
    pub path: Option<String>,
    /// Short problem statement.
    pub message: String,
    /// Concrete remediation guidance.
    pub guidance: String,
}

/// Complete untrusted protocol-v1 provider response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawProjectSyntaxSnapshot {
    /// Snapshot schema version.
    pub schema_version: u32,
    /// Claimed complete analyzed file set.
    pub files: Vec<RawSourceUnit>,
    /// Untrusted provider diagnostics; Zryna semantics remain authoritative.
    pub diagnostics: Vec<RawProviderDiagnostic>,
}

/// Verified provider-neutral function declaration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionSyntax {
    /// Function name.
    name: String,
    /// Parameter names and annotations.
    parameters: Vec<(String, TypeSyntax)>,
    /// Declared return type.
    return_type: TypeSyntax,
    /// Authoritative source range.
    span: Span,
}

impl FunctionSyntax {
    /// Returns the verified function name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the verified parameter declarations.
    #[must_use]
    pub fn parameters(&self) -> &[(String, TypeSyntax)] {
        &self.parameters
    }

    /// Returns the verified result annotation.
    #[must_use]
    pub const fn return_type(&self) -> &TypeSyntax {
        &self.return_type
    }

    /// Returns the authoritative function span.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }
}

/// Verified provider-neutral source unit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceUnit {
    /// Authoritative snapshot-local identifier.
    id: FileId,
    /// Authoritative normalized path.
    path: NormalizedSourcePath,
    /// Bounded declarations currently supported by protocol v1.
    functions: Vec<FunctionSyntax>,
}

impl SourceUnit {
    /// Returns the authoritative source identity.
    #[must_use]
    pub const fn id(&self) -> FileId {
        self.id
    }

    /// Returns the authoritative source path.
    #[must_use]
    pub const fn path(&self) -> &NormalizedSourcePath {
        &self.path
    }

    /// Returns verified function declarations.
    #[must_use]
    pub fn functions(&self) -> &[FunctionSyntax] {
        &self.functions
    }
}

/// Provider-neutral immutable project snapshot accepted by compiler phases.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSyntaxSnapshot {
    /// Snapshot schema version.
    schema_version: u32,
    /// Exact verified file set.
    files: Vec<SourceUnit>,
    /// Bounded and path-validated provider diagnostics.
    diagnostics: Vec<Diagnostic>,
}

impl ProjectSyntaxSnapshot {
    /// Returns the verified protocol version.
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the exact verified source set.
    #[must_use]
    pub fn files(&self) -> &[SourceUnit] {
        &self.files
    }

    /// Returns bounded provider diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Frontend-provider failure.
#[derive(Debug, Error)]
pub enum FrontendError {
    /// Provider speaks an incompatible protocol.
    #[error("frontend protocol mismatch: expected {expected}, received {actual}")]
    ProtocolMismatch {
        /// Required protocol.
        expected: u32,
        /// Received protocol.
        actual: u32,
    },
    /// Provider transport or execution failed.
    #[error("frontend provider failed: {0}")]
    Provider(String),
}

/// Replaceable source-analysis provider.
pub trait FrontendProvider: Send + Sync {
    /// Negotiates provider identity and capabilities.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot start or complete its handshake.
    fn handshake(&self) -> Result<ProviderInfo, FrontendError>;

    /// Produces a provider-neutral immutable syntax snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when transport, provider execution, or snapshot production fails.
    fn analyze(&self, request: &AnalyzeRequest) -> Result<RawProjectSyntaxSnapshot, FrontendError>;
}

/// Replaceable provider that implements the executable protocol-v2 syntax contract.
pub trait FrontendProviderV2: Send + Sync {
    /// Negotiates provider identity and capabilities.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot complete its handshake.
    fn handshake(&self) -> Result<ProviderInfo, FrontendError>;

    /// Produces serialized untrusted executable syntax under the protocol-v2 transport boundary.
    ///
    /// # Errors
    ///
    /// Returns an error when transport, provider execution, or snapshot production fails.
    fn analyze_v2(&self, request: &AnalyzeRequest) -> Result<Vec<u8>, FrontendError>;
}

/// Verifies that a provider can be used by this compiler.
///
/// # Errors
///
/// Returns a protocol mismatch when the provider does not speak the required Zryna contract.
pub fn verify_provider(info: &ProviderInfo) -> Result<(), FrontendError> {
    if info.protocol_version != FRONTEND_PROTOCOL_VERSION {
        return Err(FrontendError::ProtocolMismatch {
            expected: FRONTEND_PROTOCOL_VERSION,
            actual: info.protocol_version,
        });
    }
    Ok(())
}

/// Verifies that a provider speaks the executable protocol-v2 contract.
///
/// # Errors
///
/// Returns a protocol mismatch when the provider does not advertise version 2 exactly.
pub fn verify_provider_v2(info: &ProviderInfo) -> Result<(), FrontendError> {
    if info.protocol_version != syntax_v2::PROTOCOL_VERSION {
        return Err(FrontendError::ProtocolMismatch {
            expected: syntax_v2::PROTOCOL_VERSION,
            actual: info.protocol_version,
        });
    }
    Ok(())
}

/// Decodes a provider response only after enforcing the transport byte limit.
///
/// # Errors
///
/// Returns a provider error for an oversized, malformed, or schema-incompatible JSON value.
pub fn decode_snapshot(bytes: &[u8]) -> Result<RawProjectSyntaxSnapshot, FrontendError> {
    if bytes.len() > MAX_PROVIDER_RESPONSE_BYTES {
        return Err(FrontendError::Provider(format!(
            "frontend response contains {} bytes; the limit is {MAX_PROVIDER_RESPONSE_BYTES}",
            bytes.len()
        )));
    }
    serde_json::from_slice(bytes).map_err(|error| {
        FrontendError::Provider(format!("invalid frontend snapshot JSON: {error}"))
    })
}

/// Converts a complete untrusted provider response into the only snapshot compiler phases accept.
///
/// # Errors
///
/// Returns deterministic diagnostics for version mismatch, missing/duplicate/unknown files,
/// path disagreement, unbounded data, or a span rejected by the authoritative source map.
pub fn verify_snapshot(
    mut raw: RawProjectSyntaxSnapshot,
    sources: &SourceMap,
) -> Result<ProjectSyntaxSnapshot, Vec<Diagnostic>> {
    if let Some(error) = snapshot_budget_error(&raw) {
        return Err(vec![error]);
    }
    let mut diagnostics = Vec::new();
    if raw.schema_version != FRONTEND_PROTOCOL_VERSION {
        diagnostics.push(snapshot_error(
            None,
            format!(
                "frontend snapshot uses schema version {}; expected {FRONTEND_PROTOCOL_VERSION}",
                raw.schema_version
            ),
            "return the exact negotiated frontend protocol version",
        ));
    }
    if raw.files.len() != sources.len() {
        diagnostics.push(snapshot_error(
            None,
            format!(
                "frontend snapshot contains {} files; the authoritative source map contains {}",
                raw.files.len(),
                sources.len()
            ),
            "return every requested source file exactly once and no additional files",
        ));
    }
    let files = verify_files(std::mem::take(&mut raw.files), sources, &mut diagnostics);

    let provider_diagnostics =
        verify_provider_diagnostics(raw.diagnostics, sources, &mut diagnostics);

    diagnostics.sort_by(|left, right| {
        (left.path(), left.code(), left.message()).cmp(&(
            right.path(),
            right.code(),
            right.message(),
        ))
    });
    if diagnostics.is_empty() {
        Ok(ProjectSyntaxSnapshot {
            schema_version: FRONTEND_PROTOCOL_VERSION,
            files,
            diagnostics: provider_diagnostics,
        })
    } else {
        Err(diagnostics)
    }
}

fn snapshot_budget_error(raw: &RawProjectSyntaxSnapshot) -> Option<Diagnostic> {
    if raw.files.len() > MAX_SOURCE_FILES {
        return Some(snapshot_error(
            None,
            format!(
                "frontend snapshot contains {} source units; the limit is {MAX_SOURCE_FILES}",
                raw.files.len()
            ),
            "return only the complete bounded source set from the request",
        ));
    }
    let functions =
        raw.files.iter().try_fold(0_usize, |total, file| total.checked_add(file.functions.len()));
    if functions.is_none_or(|count| count > MAX_FUNCTIONS_PER_SNAPSHOT) {
        return Some(snapshot_error(
            None,
            "frontend snapshot exceeds the aggregate function budget",
            "reduce provider declarations before returning the snapshot",
        ));
    }
    let parameters = raw
        .files
        .iter()
        .flat_map(|file| &file.functions)
        .try_fold(0_usize, |total, function| total.checked_add(function.parameters.len()));
    if parameters.is_none_or(|count| count > MAX_PARAMETERS_PER_SNAPSHOT) {
        return Some(snapshot_error(
            None,
            "frontend snapshot exceeds the aggregate parameter budget",
            "reduce provider parameters before returning the snapshot",
        ));
    }
    (raw.diagnostics.len() > MAX_PROVIDER_DIAGNOSTICS).then(|| {
        snapshot_error(
            None,
            format!(
                "frontend snapshot contains {} diagnostics; the limit is {MAX_PROVIDER_DIAGNOSTICS}",
                raw.diagnostics.len()
            ),
            "reduce provider diagnostics before returning the snapshot",
        )
    })
}

fn verify_files(
    mut raw_files: Vec<RawSourceUnit>,
    sources: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<SourceUnit> {
    raw_files.sort_by(|left, right| (&left.id, &left.path).cmp(&(&right.id, &right.path)));
    let mut seen = BTreeSet::new();
    raw_files
        .into_iter()
        .filter_map(|raw| verify_file(raw, sources, &mut seen, diagnostics))
        .collect()
}

fn verify_file(
    raw: RawSourceUnit,
    sources: &SourceMap,
    seen: &mut BTreeSet<u32>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<SourceUnit> {
    if !seen.insert(raw.id) {
        push_snapshot_error(
            diagnostics,
            snapshot_error(
                None,
                format!("frontend snapshot repeats file identifier {}", raw.id),
                "return each authoritative file identifier exactly once",
            ),
        );
        return None;
    }
    let Ok(file_id) = sources.verify_file_id(raw.id) else {
        push_snapshot_error(
            diagnostics,
            snapshot_error(
                None,
                format!("frontend snapshot returned unknown file identifier {}", raw.id),
                "echo only identifiers issued by the authoritative source map",
            ),
        );
        return None;
    };
    let Ok(path) = NormalizedSourcePath::new(raw.path) else {
        push_snapshot_error(
            diagnostics,
            snapshot_error(
                None,
                format!("frontend snapshot returned an unsafe path for file {}", raw.id),
                "echo the exact normalized path from the analysis request",
            ),
        );
        return None;
    };
    if sources.source(file_id).is_none_or(|source| source.path() != &path) {
        push_snapshot_error(
            diagnostics,
            snapshot_error(
                Some(path.as_str().to_owned()),
                format!("frontend path disagrees with the authoritative path for file {}", raw.id),
                "echo the exact identifier and path pair from the analysis request",
            ),
        );
        return None;
    }
    if raw.functions.len() > MAX_FUNCTIONS_PER_FILE {
        push_snapshot_error(
            diagnostics,
            snapshot_error(
                Some(path.as_str().to_owned()),
                format!(
                    "frontend source unit contains {} functions; the limit is {MAX_FUNCTIONS_PER_FILE}",
                    raw.functions.len()
                ),
                "reduce provider declarations before returning the snapshot",
            ),
        );
        return None;
    }
    let functions = raw
        .functions
        .into_iter()
        .enumerate()
        .filter_map(|(index, function)| {
            verify_function(function, index, raw.id, &path, sources, diagnostics)
        })
        .collect();
    Some(SourceUnit { id: file_id, path, functions })
}

fn verify_function(
    raw: RawFunctionSyntax,
    index: usize,
    containing_file: u32,
    path: &NormalizedSourcePath,
    sources: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FunctionSyntax> {
    if raw.span.file != containing_file {
        push_snapshot_error(
            diagnostics,
            snapshot_error(
                Some(path.as_str().to_owned()),
                format!(
                    "function {index} references file {} instead of containing file {containing_file}",
                    raw.span.file
                ),
                "keep every declaration span inside its containing source unit",
            ),
        );
        return None;
    }
    let Ok(span) = sources.verify_span(raw.span) else {
        push_snapshot_error(
            diagnostics,
            snapshot_error(
                Some(path.as_str().to_owned()),
                format!("function {index} contains an invalid source span"),
                "return ordered in-range UTF-8 byte boundaries for the authoritative file",
            ),
        );
        return None;
    };
    if !bounded_name(&raw.name)
        || raw.parameters.len() > MAX_PARAMETERS_PER_FUNCTION
        || raw.parameters.iter().any(|(name, ty)| !bounded_name(name) || !bounded_type(ty))
        || !bounded_type(&raw.return_type)
    {
        push_snapshot_error(
            diagnostics,
            snapshot_error(
                Some(path.as_str().to_owned()),
                format!("function {index} exceeds a protocol-v1 name, type, or parameter limit"),
                "keep identifiers, type spellings, and parameter counts within the documented bounds",
            ),
        );
        return None;
    }
    Some(FunctionSyntax {
        name: raw.name,
        parameters: raw.parameters,
        return_type: raw.return_type,
        span,
    })
}

fn verify_provider_diagnostics(
    mut raw: Vec<RawProviderDiagnostic>,
    sources: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    raw.sort_by(|left, right| {
        (&left.path, &left.code, &left.message, &left.guidance).cmp(&(
            &right.path,
            &right.code,
            &right.message,
            &right.guidance,
        ))
    });
    raw.into_iter()
        .filter_map(|raw| verify_provider_diagnostic(raw, sources, diagnostics))
        .collect()
}

fn verify_provider_diagnostic(
    raw: RawProviderDiagnostic,
    sources: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Diagnostic> {
    if !bounded_name(&raw.code)
        || raw.message.len() > MAX_PROVIDER_DIAGNOSTIC_TEXT_BYTES
        || raw.guidance.len() > MAX_PROVIDER_DIAGNOSTIC_TEXT_BYTES
    {
        push_snapshot_error(
            diagnostics,
            snapshot_error(
                None,
                "frontend diagnostic exceeds a protocol-v1 text limit",
                "keep diagnostic codes, messages, and guidance within the documented bounds",
            ),
        );
        return None;
    }
    let path = match raw.path {
        Some(path) => Some(canonical_provider_path(path, sources, diagnostics)?),
        None => None,
    };
    Some(match raw.severity {
        Severity::Error => Diagnostic::error(raw.code, path, raw.message, raw.guidance),
        Severity::Warning => Diagnostic::warning(raw.code, path, raw.message, raw.guidance),
    })
}

fn canonical_provider_path(
    raw: String,
    sources: &SourceMap,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let Ok(path) = NormalizedSourcePath::new(raw) else {
        push_snapshot_error(
            diagnostics,
            snapshot_error(
                None,
                "frontend diagnostic references an unknown or unsafe source path",
                "reference only a normalized path from the authoritative source map",
            ),
        );
        return None;
    };
    if let Some(source) = sources.file_id(&path).and_then(|id| sources.source(id))
        && source.path() == &path
    {
        return Some(source.path().as_str().to_owned());
    }
    push_snapshot_error(
        diagnostics,
        snapshot_error(
            None,
            "frontend diagnostic references a non-canonical source path",
            "reference the exact normalized path from the authoritative source map",
        ),
    );
    None
}

fn bounded_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PROVIDER_NAME_BYTES
        && !value.chars().any(char::is_control)
}

fn bounded_type(ty: &TypeSyntax) -> bool {
    match ty {
        TypeSyntax::Inferred => true,
        TypeSyntax::Named(name) => bounded_name(name),
    }
}

fn snapshot_error(
    path: Option<String>,
    message: impl Into<String>,
    guidance: impl Into<String>,
) -> Diagnostic {
    Diagnostic::error("ZRYNA-F1002", path, message, guidance)
}

fn push_snapshot_error(diagnostics: &mut Vec<Diagnostic>, diagnostic: Diagnostic) {
    if diagnostics.len() < MAX_SNAPSHOT_VALIDATION_ERRORS {
        diagnostics.push(diagnostic);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zryna_source::SourceFileInput;

    fn sources() -> SourceMap {
        SourceMap::build(vec![
            SourceFileInput {
                path: "src/z.zry".to_owned(),
                text: "export function z(): i32 { return 1; }".to_owned(),
            },
            SourceFileInput {
                path: "src/a.zry".to_owned(),
                text: "export function a(): i32 { return 1; }".to_owned(),
            },
        ])
        .expect("fixture source map must be valid")
    }

    fn raw_file(id: u32, path: &str, end: u32) -> RawSourceUnit {
        RawSourceUnit {
            id,
            path: path.to_owned(),
            functions: vec![RawFunctionSyntax {
                name: "value".to_owned(),
                parameters: Vec::new(),
                return_type: TypeSyntax::Named("i32".to_owned()),
                span: UntrustedSpan { file: id, start: 0, end },
            }],
        }
    }

    #[test]
    fn verifies_the_exact_file_set_and_authoritative_spans() {
        let sources = sources();
        let raw = RawProjectSyntaxSnapshot {
            schema_version: FRONTEND_PROTOCOL_VERSION,
            files: vec![raw_file(1, "src/z.zry", 38), raw_file(0, "src/a.zry", 38)],
            diagnostics: Vec::new(),
        };
        let snapshot = verify_snapshot(raw, &sources).expect("complete snapshot must verify");
        assert_eq!(
            snapshot
                .files
                .iter()
                .map(|file| (file.id.index(), file.path.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "src/a.zry"), (1, "src/z.zry")]
        );
        for file in &snapshot.files {
            assert!(sources.resolve(file.functions[0].span).is_ok());
        }
    }

    #[test]
    fn rejects_duplicate_missing_unknown_and_malformed_provider_data() {
        let sources = sources();
        let raw = RawProjectSyntaxSnapshot {
            schema_version: FRONTEND_PROTOCOL_VERSION,
            files: vec![raw_file(0, "src/a.zry", 38), raw_file(0, "bad\npath.zry", u32::MAX)],
            diagnostics: Vec::new(),
        };
        let diagnostics = verify_snapshot(raw, &sources).expect_err("duplicate snapshot must fail");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.message().contains("repeats")));
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.path() != Some("bad\npath.zry")));

        let raw = RawProjectSyntaxSnapshot {
            schema_version: FRONTEND_PROTOCOL_VERSION,
            files: vec![raw_file(0, "src/a.zry", u32::MAX), raw_file(7, "src/z.zry", 0)],
            diagnostics: Vec::new(),
        };
        let diagnostics =
            verify_snapshot(raw, &sources).expect_err("unknown and invalid spans must fail");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message().contains("invalid source span"))
        );
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.message().contains("unknown file")));
    }

    #[test]
    fn rejects_noncanonical_diagnostic_paths_and_bounded_wire_failures() {
        let sources = sources();
        let raw = RawProjectSyntaxSnapshot {
            schema_version: FRONTEND_PROTOCOL_VERSION,
            files: vec![raw_file(0, "src/a.zry", 38), raw_file(1, "src/z.zry", 38)],
            diagnostics: vec![RawProviderDiagnostic {
                code: "TS1000".to_owned(),
                severity: Severity::Error,
                path: Some("SRC/A.ZRY".to_owned()),
                message: "provider error".to_owned(),
                guidance: "fix it".to_owned(),
            }],
        };
        let diagnostics =
            verify_snapshot(raw, &sources).expect_err("case-variant provider path must fail");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message().contains("non-canonical source path") })
        );

        let oversized = vec![b' '; MAX_PROVIDER_RESPONSE_BYTES + 1];
        assert!(decode_snapshot(&oversized).is_err());
        assert!(
            decode_snapshot(br#"{"schema_version":1,"files":[],"diagnostics":[],"extra":true}"#)
                .is_err()
        );
        assert!(decode_snapshot(br#"{"schema_version":1,"files":[{"id":-1,"path":"a.zry","functions":[]}],"diagnostics":[]}"#).is_err());
    }

    #[test]
    fn protocol_v2_requires_an_exact_explicit_handshake() {
        let mut info = ProviderInfo {
            provider: "fixture".to_owned(),
            provider_version: "1.0.0".to_owned(),
            protocol_version: syntax_v2::PROTOCOL_VERSION,
            capabilities: FrontendCapabilities {
                module_resolution: false,
                semantic_diagnostics: false,
            },
        };
        assert!(verify_provider_v2(&info).is_ok());
        assert!(verify_provider(&info).is_err());

        info.protocol_version = FRONTEND_PROTOCOL_VERSION;
        assert!(verify_provider(&info).is_ok());
        assert!(verify_provider_v2(&info).is_err());
    }
}
