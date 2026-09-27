use std::time::{Duration, Instant};

use zryna_frontend::{VerifiedFrontendProviderV3, native_lexer, native_parser, syntax_v3};
use zryna_source::{NormalizedSourcePath, SourceMap};

use super::{ModuleClosureError, VerifiedModuleClosure, discover_module_closure_with_clock};
use crate::WorkspaceSourceRoot;

/// Discovers, authenticates, and seals one bounded deterministic M2 module closure.
///
/// The provider receives only immutable source bytes and normalized portable paths. It never
/// receives the workspace capability or chooses a resolved host path. Intermediate snapshots are
/// discarded; only one final full-map snapshot is returned.
///
/// # Errors
///
/// Returns a fail-closed frontend or deterministic driver rejection before semantic analysis or
/// artifact creation.
pub fn discover_module_closure<Provider: VerifiedFrontendProviderV3 + ?Sized>(
    root: &WorkspaceSourceRoot,
    entrypoint: NormalizedSourcePath,
    frontend: &Provider,
) -> Result<VerifiedModuleClosure, ModuleClosureError> {
    discover_module_closure_with_clock(root, entrypoint, frontend, Instant::now)
}

/// Discovers an internal import-only native v3 closure without selecting a public provider.
///
/// Every source in the graph must contain only named imports and trivia. The native parser
/// constructs an untrusted candidate; the existing v3 verifier authenticates it before the
/// driver resolves any import. The returned closure has no compilation or target authority.
///
/// # Errors
///
/// Rejects unsupported syntax, failed verification, or any existing source-closure failure.
pub fn discover_native_import_only_closure(
    root: &WorkspaceSourceRoot,
    entrypoint: NormalizedSourcePath,
) -> Result<VerifiedModuleClosure, ModuleClosureError> {
    discover_module_closure_with_clock(root, entrypoint, &NativeImportFrontend, Instant::now)
}

/// Discovers an internal native v3 closure of named imports and straight-line functions.
///
/// Every source must contain an import prefix followed by only the function form supported by
/// the native v3 candidate. The existing verifier authenticates each candidate before the
/// driver resolves imports or seals the final source map. This entry selects no public frontend.
///
/// # Errors
///
/// Rejects unsupported syntax, failed verification, or any existing source-closure failure.
pub fn discover_native_straight_line_closure(
    root: &WorkspaceSourceRoot,
    entrypoint: NormalizedSourcePath,
) -> Result<VerifiedModuleClosure, ModuleClosureError> {
    discover_module_closure_with_clock(root, entrypoint, &NativeStraightLineFrontend, Instant::now)
}

pub(crate) trait ClosureFrontendV3 {
    fn minimum_analysis_timeout(&self) -> Duration;
    fn analyze(
        &self,
        sources: &SourceMap,
        timeout: Duration,
    ) -> Result<syntax_v3::ProjectSyntaxSnapshot, ModuleClosureError>;
}

impl<Provider: VerifiedFrontendProviderV3 + ?Sized> ClosureFrontendV3 for Provider {
    fn minimum_analysis_timeout(&self) -> Duration {
        VerifiedFrontendProviderV3::minimum_analysis_timeout(self)
    }

    fn analyze(
        &self,
        sources: &SourceMap,
        timeout: Duration,
    ) -> Result<syntax_v3::ProjectSyntaxSnapshot, ModuleClosureError> {
        self.analyze_verified_v3_with_timeout(sources, timeout)
            .map_err(ModuleClosureError::Frontend)
    }
}

struct NativeImportFrontend;

impl ClosureFrontendV3 for NativeImportFrontend {
    fn minimum_analysis_timeout(&self) -> Duration {
        Duration::ZERO
    }

    fn analyze(
        &self,
        sources: &SourceMap,
        _timeout: Duration,
    ) -> Result<syntax_v3::ProjectSyntaxSnapshot, ModuleClosureError> {
        let lexed = native_lexer::lex(sources)
            .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))?;
        let raw = native_parser::v3::parse_v3_import_candidate(sources, &lexed)
            .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))?;
        syntax_v3::verify_snapshot(raw, sources).map_err(ModuleClosureError::Rejected)
    }
}

struct NativeStraightLineFrontend;

impl ClosureFrontendV3 for NativeStraightLineFrontend {
    fn minimum_analysis_timeout(&self) -> Duration {
        Duration::ZERO
    }

    fn analyze(
        &self,
        sources: &SourceMap,
        _timeout: Duration,
    ) -> Result<syntax_v3::ProjectSyntaxSnapshot, ModuleClosureError> {
        let lexed = native_lexer::lex(sources)
            .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))?;
        let raw = native_parser::v3::parse_v3_straight_line_candidate(sources, &lexed)
            .map_err(|error| ModuleClosureError::Rejected(vec![error.diagnostic().clone()]))?;
        syntax_v3::verify_snapshot(raw, sources).map_err(ModuleClosureError::Rejected)
    }
}

#[cfg(test)]
mod native_tests;
