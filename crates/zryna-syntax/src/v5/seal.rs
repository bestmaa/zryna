//! Immutable syntax authority for this source map only; no semantic or executable profile.

use super::{RawProjectSyntaxSnapshot, RawSourceUnit, verification::CompleteSyntaxProof};
use zryna_source::{SourceMap, SourceMapIdentity};

#[derive(Debug)]
pub struct VerifiedProjectSyntaxV5 {
    identity: SourceMapIdentity,
    raw: RawProjectSyntaxSnapshot,
}

impl VerifiedProjectSyntaxV5 {
    pub(super) fn admitted(
        raw: RawProjectSyntaxSnapshot,
        sources: &SourceMap,
        _proof: CompleteSyntaxProof,
    ) -> Self {
        Self { identity: sources.identity(), raw }
    }

    #[must_use]
    pub fn is_bound_to(&self, sources: &SourceMap) -> bool {
        self.identity == sources.identity()
    }

    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        super::PROTOCOL_VERSION
    }

    /// Read-only source-authenticated syntax; names and generic arguments remain unresolved.
    #[must_use]
    pub fn files(&self) -> &[RawSourceUnit] {
        &self.raw.files
    }

    /// Source-authenticated locations with advisory provider text, never compiler authority.
    #[must_use]
    pub fn advisories(&self) -> &[crate::v4::RawProviderDiagnostic] {
        &self.raw.diagnostics
    }
}
