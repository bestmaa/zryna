//! Zryna compiler-phase orchestration.
//!
//! Legacy protocol-v1 syntax cannot enter the protocol-v2 semantic boundary:
//!
//! ```compile_fail
//! fn bypass(
//!     legacy: &zryna_frontend::ProjectSyntaxSnapshot,
//!     sources: &zryna_source::SourceMap,
//! ) {
//!     let _ = zryna_semantics::SemanticInput::try_new(legacy, sources);
//! }
//! ```
//!
//! Private scalar adapter authorities are intentionally absent from the external crate surface:
//!
//! ```compile_fail
//! let _ = zryna_driver::VerifiedScalarEsm;
//! ```
//!
//! ```compile_fail
//! fn accept_raw(_: zryna_driver::scalar_adapter_interface::raw::Interface) {}
//! ```

#![forbid(unsafe_code)]

mod browser_component;
mod command_runtime;
pub mod diagnostic_sessions;
pub mod distribution;
mod javascript;
mod module_closure;
#[cfg(test)]
mod module_closure_tests;
mod native;
mod ownership_api;
mod ownership_closure;
mod ownership_commands;
mod ownership_manifest;
mod ownership_pipeline;
mod ownership_publication;
mod ownership_runtime_v1;
mod package_build;
mod package_resolution;
mod pipeline;
mod pipeline_runtime;
mod profile_composition;
mod project;
mod runtime;
mod scalar_adapter_interface;
mod source_api;
mod source_session;
mod webassembly;
mod workspace_source;

use std::path::Path;

use zryna_architecture::ValidationReport;
use zryna_diagnostics::{Diagnostic, Severity};
use zryna_ir::VerifiedProgram;
use zryna_source::SourceMap;

pub use command_runtime::{CommandHostPolicy, PreparedCommand, prepare_command_self_check};
pub use javascript::{
    ArtifactOutputRoot, JAVASCRIPT_ARTIFACT_EXTENSION, JavaScriptBuildError,
    JavaScriptBuildSuccess, JavaScriptOutputRoot, MAX_ARTIFACT_STEM_BYTES,
    MAX_JAVASCRIPT_ARTIFACT_STEM_BYTES, PublishedJavaScriptArtifact, compile_javascript,
    publish_javascript,
};
pub use module_closure::{
    MAX_MODULE_DIRECTORY_ENTRIES, MAX_MODULE_DISCOVERY_ROUNDS, MAX_MODULE_DISCOVERY_WALL_TIME,
    MAX_MODULE_EDGE_MANIFEST_BYTES, MAX_MODULE_FILES, MAX_MODULE_IMPORT_DECLARATIONS,
    MAX_MODULE_IMPORT_EDGES, MAX_MODULE_PROVIDER_CALLS, MAX_MODULE_PROVIDER_SOURCE_BYTES,
    MAX_MODULE_SOURCE_BYTES, ModuleClosureError, ModuleEdge, ModuleRecord, VerifiedModuleClosure,
    discover_module_closure,
};
pub use native::{
    LinuxX8664LinkToolchain, MAX_NATIVE_EXECUTABLE_BYTES, MAX_NATIVE_LINK_TIMEOUT,
    MAX_NATIVE_OBJECT_ARTIFACT_STEM_BYTES, MAX_NATIVE_PROBE_TIMEOUT, MAX_NATIVE_RUN_STDERR_BYTES,
    MAX_NATIVE_RUN_TIMEOUT, MAX_NATIVE_TOOL_OUTPUT_BYTES, NATIVE_EXECUTABLE_ARTIFACT_EXTENSION,
    NATIVE_OBJECT_ARTIFACT_EXTENSION, NativeExecutableBuildError, NativeExecutableBuildSuccess,
    NativeObjectBuildError, NativeObjectBuildSuccess, NativeObjectOutputRoot, NativeProcessLimits,
    NativeRunError, PublishedNativeExecutableArtifact, PublishedNativeObjectArtifact,
    compile_native_invocation, compile_native_object, discover_linux_native_toolchain,
    publish_native_object, run_native_invocation, select_native_object_target,
};
pub use ownership_api::*;
pub use package_build::*;
pub use package_resolution::{
    AuthenticatedPackageFile, AuthenticatedPackageSources, PackageLockMode,
    PackageResolutionRequest, PackageResolutionSuccess, resolve_package,
};
pub use pipeline::{
    BuildRequest, CommandFailure, CommandFailureKind, CommandKind, CommandSuccess,
    ControlFlowBuildRequest, ControlFlowRunRequest, PublishedTargetArtifact, RunRequest,
    TargetResult, TargetSelection, build_control_flow_workspace, build_workspace,
    run_control_flow_workspace, run_workspace,
};
pub use project::{ProjectBuildRequest, ProjectRunRequest, build_project, run_project};
pub use source_api::{DualTargetArtifacts, SourceToIrError, SourceToIrSuccess};
pub use webassembly::{
    MAX_WEBASSEMBLY_ARTIFACT_STEM_BYTES, PublishedWebAssemblyArtifact,
    WEBASSEMBLY_ARTIFACT_EXTENSION, WebAssemblyBuildError, WebAssemblyBuildSuccess,
    WebAssemblyOutputRoot, compile_webassembly, publish_webassembly,
};
pub use workspace_source::WorkspaceSourceRoot;

/// Runs the mandatory workspace gate.
#[must_use]
pub fn check_workspace(root: &Path) -> ValidationReport {
    zryna_architecture::validate_workspace(root)
}

/// Runs one authenticated frontend worker and returns only source-map-verified syntax.
///
/// # Errors
///
/// Returns a deterministic worker failure before untrusted syntax can enter later phases.
pub fn analyze_sources<Provider: zryna_frontend::VerifiedFrontendProvider + ?Sized>(
    frontend: &Provider,
    sources: &SourceMap,
) -> Result<zryna_frontend::syntax_v2::ProjectSyntaxSnapshot, zryna_frontend::WorkerError> {
    frontend.analyze_verified(sources)
}

/// Lowers one source-map-bound protocol-v2 snapshot and runs the mandatory IR verifier.
///
/// Provider errors stop before semantic analysis. Provider warnings remain observable on success.
///
/// # Errors
///
/// Returns deterministic diagnostics and never exposes raw IR when semantic or IR verification
/// fails.
pub fn lower_verified_syntax(
    syntax: &zryna_frontend::syntax_v2::ProjectSyntaxSnapshot,
    sources: &SourceMap,
) -> Result<SourceToIrSuccess, Vec<Diagnostic>> {
    if !syntax.is_bound_to(sources) {
        return Err(vec![Diagnostic::error(
            "ZRYNA-D1001",
            None,
            "verified syntax is not bound to the driver's authoritative source map",
            "analyze and lower with the same immutable source map instance",
        )]);
    }
    if syntax.diagnostics().iter().any(|diagnostic| diagnostic.severity() == Severity::Error) {
        return Err(syntax.diagnostics().to_vec());
    }
    let Some(input) = zryna_semantics::SemanticInput::try_new(syntax, sources) else {
        return Err(vec![Diagnostic::error(
            "ZRYNA-D1002",
            None,
            "verified syntax could not enter semantic analysis",
            "report this compiler invariant failure with the smallest reproducible source",
        )]);
    };
    let program = zryna_semantics::lower(input)?;
    let program = zryna_ir::verify(program, sources)?;
    let diagnostics = syntax
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Warning)
        .cloned()
        .collect();
    Ok(SourceToIrSuccess { program, diagnostics })
}

/// Authenticates a frontend, analyzes real source, lowers strict semantics, and verifies IR.
///
/// # Errors
///
/// Returns the exact frontend failure or bounded rejection diagnostics. No backend can observe an
/// unverified program through this path.
pub fn compile_to_verified_ir<Provider: zryna_frontend::VerifiedFrontendProvider + ?Sized>(
    frontend: &Provider,
    sources: &SourceMap,
) -> Result<SourceToIrSuccess, SourceToIrError> {
    let syntax = analyze_sources(frontend, sources).map_err(SourceToIrError::Frontend)?;
    lower_verified_syntax(&syntax, sources).map_err(SourceToIrError::Rejected)
}

/// Emits both current backend artifacts from one verified program.
///
/// Raw Universal IR cannot enter driver emission:
///
/// ```compile_fail
/// let raw = zryna_ir::Program::default();
/// let _ = zryna_driver::emit_verified(&raw);
/// ```
///
/// # Errors
///
/// Returns bounded diagnostics when native MIR verification or either target emission fails.
pub fn emit_verified(program: &VerifiedProgram) -> Result<DualTargetArtifacts, Vec<Diagnostic>> {
    let javascript = zryna_backend_javascript::emit(program).map_err(|error| vec![error])?;
    let mir = zryna_native_mir::lower(program)?;
    let llvm_ir = zryna_backend_native::emit_llvm_ir(&mir).map_err(|error| vec![error])?;
    Ok(DualTargetArtifacts { javascript, llvm_ir })
}

#[cfg(test)]
mod tests;
