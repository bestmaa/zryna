use zryna_diagnostics::Severity;
use zryna_source::{FileId, SourceMap};
use zryna_syntax::v5::VerifiedProjectSyntaxV5;

/// Original authenticated inputs for the bounded-generics declaration phase.
///
/// Untrusted syntax cannot enter this boundary.
/// ```compile_fail
/// fn bypass<'a>(raw: &'a zryna_syntax::v5::RawProjectSyntaxSnapshot,
///     sources: &'a zryna_source::SourceMap, entry: zryna_source::FileId) {
///     zryna_semantics::bounded_generics_v1::SemanticInput::try_new(raw, sources, entry);
/// }
/// ```
/// The existing M3 boundary keeps its distinct protocol-v4 input.
/// ```compile_fail
/// fn downgrade(syntax: &zryna_syntax::v5::VerifiedProjectSyntaxV5,
///     sources: &zryna_source::SourceMap, entry: zryna_source::FileId) {
///     zryna_semantics::data_ownership_v1::SemanticInput::try_new(syntax, sources, entry);
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct SemanticInput<'a> {
    syntax: &'a VerifiedProjectSyntaxV5,
    sources: &'a SourceMap,
    entry: FileId,
}

impl<'a> SemanticInput<'a> {
    /// Requires the issuing source map, one original entry and provider success.
    #[must_use]
    pub fn try_new(
        syntax: &'a VerifiedProjectSyntaxV5,
        sources: &'a SourceMap,
        entry: FileId,
    ) -> Option<Self> {
        (syntax.is_bound_to(sources)
            && sources.source(entry).is_some()
            && syntax.files().iter().filter(|file| file.id == entry.index()).count() == 1
            && syntax.advisories().iter().all(|d| d.severity != Severity::Error))
        .then_some(Self { syntax, sources, entry })
    }

    /// Returns the retained complete source-authenticated syntax.
    #[must_use]
    pub const fn syntax(self) -> &'a VerifiedProjectSyntaxV5 {
        self.syntax
    }

    /// Returns the original immutable source authority.
    #[must_use]
    pub const fn sources(self) -> &'a SourceMap {
        self.sources
    }

    /// Returns the independently selected original entry.
    #[must_use]
    pub const fn entry(self) -> FileId {
        self.entry
    }
}
