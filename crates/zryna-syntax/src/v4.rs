//! Fail-closed provider-neutral syntax protocol version 4.

#![allow(missing_docs)]
#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::drop_non_drop,
    clippy::manual_let_else,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match_else,
    clippy::too_many_lines
)]

use std::{collections::BTreeSet, fmt, marker::PhantomData};

use serde::{
    Deserialize, Deserializer, Serialize,
    de::{Error as _, IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use zryna_diagnostics::{Diagnostic, Severity};
use zryna_source::{
    FileId, MAX_SOURCE_FILES, NormalizedSourcePath, SourceMap, SourceMapIdentity, Span,
    UntrustedSpan,
};

#[path = "v4_lexical_bindings.rs"]
mod lexical_bindings;

mod decode;
mod limits;
mod raw;
mod structure;
mod verify;

pub use decode::{SyntaxDecodeError, decode_snapshot};
pub use limits::*;
pub use raw::*;

use structure::{
    contains_claim, require_claim_contains, require_claim_order, verify_file_structure,
};
use verify::{check_budgets, verify_file, verify_provider_diagnostics};

/// One source unit whose complete v4 claim has been authenticated.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceUnit {
    id: FileId,
    path: NormalizedSourcePath,
    raw: RawSourceUnit,
}
impl SourceUnit {
    #[must_use]
    pub const fn id(&self) -> FileId {
        self.id
    }
    #[must_use]
    pub const fn path(&self) -> &NormalizedSourcePath {
        &self.path
    }
    #[must_use]
    pub fn imports(&self) -> &[RawImportSyntax] {
        &self.raw.imports
    }
    #[must_use]
    pub fn type_syntax(&self) -> &[RawTypeSyntax] {
        &self.raw.type_syntax
    }
    #[must_use]
    pub fn data_declarations(&self) -> &[RawDataDeclaration] {
        &self.raw.data_declarations
    }
    #[must_use]
    pub fn functions(&self) -> &[RawFunctionSyntax] {
        &self.raw.functions
    }
}

/// Opaque all-or-nothing v4 snapshot bound to one exact source map identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectSyntaxSnapshot {
    #[serde(skip)]
    source_map_identity: SourceMapIdentity,
    schema_version: u32,
    files: Vec<SourceUnit>,
    diagnostics: Vec<Diagnostic>,
}
impl ProjectSyntaxSnapshot {
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
    #[must_use]
    pub fn files(&self) -> &[SourceUnit] {
        &self.files
    }
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    #[must_use]
    pub fn is_bound_to(&self, sources: &SourceMap) -> bool {
        self.source_map_identity == sources.identity()
            && self.files.len() == sources.len()
            && self.files.iter().all(|file| {
                sources.source(file.id).is_some_and(|source| source.path() == &file.path)
            })
    }
}

#[derive(Default)]
struct Errors {
    items: Vec<Diagnostic>,
    truncated: bool,
}
impl Errors {
    fn push(&mut self, diagnostic: Diagnostic) {
        if self.items.len() < MAX_VALIDATION_ERRORS - 1 {
            self.items.push(diagnostic);
        } else {
            self.truncated = true;
        }
    }
    fn protocol(&mut self, path: Option<&str>, message: impl Into<String>) {
        self.push(Diagnostic::error(
            "ZRYNA-Y4001",
            path.map(str::to_owned),
            message,
            "return the exact bounded protocol-v4 contract",
        ));
    }
    fn node(&mut self, path: &NormalizedSourcePath, message: impl Into<String>) {
        self.push(Diagnostic::error(
            "ZRYNA-Y4002",
            Some(path.as_str().to_owned()),
            message,
            "return source-faithful canonical protocol-v4 syntax",
        ));
    }
    fn limit(&mut self, message: impl Into<String>) {
        self.push(limit_error(message));
    }
    fn finish(mut self) -> Vec<Diagnostic> {
        self.items.sort_by_key(ToString::to_string);
        if self.truncated {
            self.items.push(limit_error("validation diagnostics exceeded the deterministic limit"));
        }
        self.items
    }
}
fn limit_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("ZRYNA-F1401", None, message, "reduce the bounded protocol-v4 input")
}

/// Authenticates a bounded raw v4 response against one exact final source map.
///
/// # Errors
///
/// Returns deterministic bounded protocol, source-fidelity, or resource diagnostics. No verified
/// view is returned unless every response claim succeeds.
pub fn verify_snapshot(
    raw: RawProjectSyntaxSnapshot,
    sources: &SourceMap,
) -> Result<ProjectSyntaxSnapshot, Vec<Diagnostic>> {
    let mut errors = Errors::default();
    if raw.schema_version != PROTOCOL_VERSION {
        errors.protocol(None, "snapshot schema version is not exactly 4");
    }
    if raw.files.len() != sources.len() {
        errors.protocol(None, "snapshot file set is not complete");
    }
    if let Err(message) = check_budgets(&raw, sources) {
        return Err(vec![limit_error(message)]);
    }
    let mut verified = Vec::with_capacity(raw.files.len());
    for (position, file) in raw.files.into_iter().enumerate() {
        if let Some(file) = verify_file(file, position, sources, &mut errors) {
            verified.push(file);
        }
    }
    let diagnostics = verify_provider_diagnostics(raw.diagnostics, sources, &mut errors);
    if errors.items.is_empty() {
        Ok(ProjectSyntaxSnapshot {
            source_map_identity: sources.identity(),
            schema_version: PROTOCOL_VERSION,
            files: verified,
            diagnostics,
        })
    } else {
        Err(errors.finish())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
