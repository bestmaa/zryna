//! Internal candidate dispatch for authenticated `DataOwnershipV1` builds and runs.

use std::{fs, path::PathBuf};

pub(crate) mod installed;
mod preparation;

use zryna_abi::{Invocation, ScalarValue};
use zryna_backend_native::data_ownership_v1::ValidatedDataOwnershipObjectArtifact;
use zryna_diagnostics::Diagnostic;
use zryna_frontend::{
    ProviderExpectationV4, VerifiedFrontendProviderV4, WorkerFrontendV4, WorkerLimitsV4,
    WorkerSpecV4,
};
use zryna_source::NormalizedSourcePath;

use crate::{
    ArtifactOutputRoot, CommandFailure, CommandFailureKind, NativeProcessLimits,
    PreparedDataOwnershipExecutable, TargetSelection, VerifiedOwnershipModuleClosure,
    WorkspaceSourceRoot, check_workspace, discover_linux_native_toolchain,
    discover_ownership_module_closure, prepare_data_ownership_executable,
    runtime::{NodeRuntimeCapability, node_compatible_path},
};

/// Exact profile identity retained by every internal candidate request.
pub const DATA_OWNERSHIP_CANDIDATE_PROFILE: &str = "zryna-data-ownership-v1";
const NATIVE_TARGET: &str = "x86_64-unknown-linux-gnu";

/// One explicit internal `DataOwnershipV1` build request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataOwnershipBuildRequest {
    /// Absolute canonical workspace root.
    pub workspace_root: PathBuf,
    /// Portable workspace-relative `.zry` entry module.
    pub entrypoint: String,
    /// Portable artifact stem retained by manifest and publication layers.
    pub artifact_stem: String,
    /// Exact selected target set.
    pub targets: TargetSelection,
    /// Absolute direct Node.js executable used by the authenticated frontend.
    pub node_runtime: PathBuf,
}

/// One explicit internal `DataOwnershipV1` run request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataOwnershipRunRequest {
    /// Shared authenticated build request.
    pub build: DataOwnershipBuildRequest,
    /// Exact scalar export selected after semantic and ABI verification.
    pub logical_export: String,
    /// Ordered typed scalar arguments.
    pub arguments: Vec<ScalarValue>,
}

/// Backend bytes prepared in canonical JavaScript, WebAssembly, native order.
#[derive(Clone, Debug, Default)]
pub(crate) struct PreparedDataOwnershipArtifacts {
    javascript: Option<zryna_backend_javascript::JavaScriptArtifact>,
    webassembly: Option<zryna_backend_webassembly::ValidatedWebAssemblyArtifact>,
    native_object: Option<ValidatedDataOwnershipObjectArtifact>,
    native_executable: Option<PreparedDataOwnershipExecutable>,
}

impl PreparedDataOwnershipArtifacts {
    /// Returns prepared JavaScript when selected.
    #[must_use]
    pub(crate) const fn javascript(&self) -> Option<&zryna_backend_javascript::JavaScriptArtifact> {
        self.javascript.as_ref()
    }
    /// Returns prepared core WebAssembly when selected.
    #[must_use]
    pub(crate) const fn webassembly(
        &self,
    ) -> Option<&zryna_backend_webassembly::ValidatedWebAssemblyArtifact> {
        self.webassembly.as_ref()
    }
    /// Returns the audited native object for a build request.
    #[must_use]
    pub(crate) const fn native_object(&self) -> Option<&ValidatedDataOwnershipObjectArtifact> {
        self.native_object.as_ref()
    }
    /// Returns the linked audited native invocation for a run request.
    #[must_use]
    pub(crate) const fn native_executable(&self) -> Option<&PreparedDataOwnershipExecutable> {
        self.native_executable.as_ref()
    }
}

/// One fully authenticated but unpublished candidate result.
#[derive(Debug)]
pub(crate) struct DataOwnershipCandidateSuccess {
    workspace_root: PathBuf,
    node_runtime: PathBuf,
    closure: VerifiedOwnershipModuleClosure,
    program: zryna_semantics::data_ownership_v1::VerifiedProgram,
    artifacts: PreparedDataOwnershipArtifacts,
    artifact_stem: String,
    logical_export: Option<String>,
    arguments: Vec<ScalarValue>,
    diagnostics: Vec<Diagnostic>,
}

impl DataOwnershipCandidateSuccess {
    /// Returns the validated absolute workspace root retained for publication.
    #[must_use]
    pub(crate) fn workspace_root(&self) -> &std::path::Path {
        &self.workspace_root
    }
    /// Returns the authenticated direct Node.js executable requested for this command.
    #[must_use]
    pub(crate) fn node_runtime(&self) -> &std::path::Path {
        &self.node_runtime
    }
    /// Returns the single final source/syntax authority shared by every selected backend.
    #[must_use]
    pub(crate) const fn closure(&self) -> &VerifiedOwnershipModuleClosure {
        &self.closure
    }
    /// Returns the single verifier-sealed semantic authority shared by every target.
    #[must_use]
    pub(crate) const fn program(&self) -> &zryna_semantics::data_ownership_v1::VerifiedProgram {
        &self.program
    }
    /// Returns canonical prepared target artifacts.
    #[must_use]
    pub(crate) const fn artifacts(&self) -> &PreparedDataOwnershipArtifacts {
        &self.artifacts
    }
    /// Returns the authenticated portable artifact stem.
    #[must_use]
    pub(crate) fn artifact_stem(&self) -> &str {
        &self.artifact_stem
    }
    /// Returns the selected run export, or `None` for build requests.
    #[must_use]
    pub(crate) fn logical_export(&self) -> Option<&str> {
        self.logical_export.as_deref()
    }
    /// Returns typed run arguments in declaration order.
    #[must_use]
    pub(crate) fn arguments(&self) -> &[ScalarValue] {
        &self.arguments
    }
    /// Returns authenticated provider warnings.
    #[must_use]
    pub(crate) fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DispatchPhase {
    JavaScript,
    WebAssembly,
    Native,
}

type Checkpoint<'a> = &'a dyn Fn(DispatchPhase) -> Result<(), CommandFailure>;

#[cfg(test)]
pub(crate) static OWNERSHIP_ROUTE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Authenticates and prepares one internal candidate build without publishing it.
///
/// # Errors
/// Returns stable phase-owned diagnostics before any final output becomes visible.
pub(crate) fn prepare_data_ownership_build(
    request: &DataOwnershipBuildRequest,
) -> Result<DataOwnershipCandidateSuccess, CommandFailure> {
    execute(request, None, configured_frontend_v4, &|_| Ok(()), validate_request)
}

/// Authenticates and prepares one internal candidate run without publishing it.
///
/// # Errors
/// Returns stable source, ABI, backend, toolchain, or audit diagnostics.
pub(crate) fn prepare_data_ownership_run(
    request: DataOwnershipRunRequest,
) -> Result<DataOwnershipCandidateSuccess, CommandFailure> {
    execute(
        &request.build,
        Some((request.logical_export, request.arguments)),
        configured_frontend_v4,
        &|_| Ok(()),
        validate_request,
    )
}

#[cfg(test)]
pub(crate) fn prepare_data_ownership_for_test(
    request: &DataOwnershipBuildRequest,
    run: Option<(String, Vec<ScalarValue>)>,
) -> Result<DataOwnershipCandidateSuccess, CommandFailure> {
    execute(request, run, configured_test_frontend_v4, &|_| Ok(()), validate_request_shape)
}

fn execute<Provider, Factory>(
    request: &DataOwnershipBuildRequest,
    run: Option<(String, Vec<ScalarValue>)>,
    frontend_factory: Factory,
    checkpoint: Checkpoint<'_>,
    validator: fn(&DataOwnershipBuildRequest) -> Result<(), CommandFailure>,
) -> Result<DataOwnershipCandidateSuccess, CommandFailure>
where
    Provider: VerifiedFrontendProviderV4,
    Factory: FnOnce(
        &DataOwnershipBuildRequest,
        &NodeRuntimeCapability,
    ) -> Result<Provider, CommandFailure>,
{
    validator(request)?;
    let source_root = WorkspaceSourceRoot::capture(&request.workspace_root)
        .map_err(|item| failure(CommandFailureKind::Source, item))?;
    let entrypoint = NormalizedSourcePath::new(request.entrypoint.clone()).map_err(|error| {
        failure(CommandFailureKind::Request, Diagnostic::from_source_error(&error))
    })?;
    let node = NodeRuntimeCapability::discover(&request.node_runtime, &request.workspace_root)
        .map_err(|item| failure(CommandFailureKind::Preparation, item))?;
    let frontend = frontend_factory(request, &node)?;
    let closure = discover_ownership_module_closure(&source_root, entrypoint, &frontend)
        .map_err(|error| closure_failure(&error))?;
    preparation::prepare_closure(request, closure, run, checkpoint, &|| {
        node.revalidate().map_err(|item| failure(CommandFailureKind::Preparation, item))
    })
}

fn configured_frontend_v4(
    request: &DataOwnershipBuildRequest,
    node: &NodeRuntimeCapability,
) -> Result<WorkerFrontendV4, CommandFailure> {
    let adapter = request.workspace_root.join("adapters/typescript-6");
    configured_frontend_at(&adapter, node)
}

#[cfg(test)]
fn configured_test_frontend_v4(
    _request: &DataOwnershipBuildRequest,
    node: &NodeRuntimeCapability,
) -> Result<WorkerFrontendV4, CommandFailure> {
    configured_frontend_at(&test_support::adapter_root(), node)
}

pub(crate) fn configured_frontend_at(
    adapter: &std::path::Path,
    node: &NodeRuntimeCapability,
) -> Result<WorkerFrontendV4, CommandFailure> {
    let worker = adapter.join("src/worker-v4.mjs");
    for (path, label) in [(adapter, "adapter directory"), (worker.as_path(), "worker entrypoint")] {
        let metadata = fs::symlink_metadata(path)
            .map_err(|_| preparation_error(format!("DataOwnershipV1 {label} is unavailable")))?;
        if (path == adapter && !metadata.is_dir())
            || (path == worker && !metadata.is_file())
            || is_link_like(&metadata)
        {
            return Err(preparation_error(format!("DataOwnershipV1 {label} is not a real path")));
        }
    }
    let expected =
        ProviderExpectationV4::new("typescript-6", "6.0.3").map_err(|error| CommandFailure {
            kind: CommandFailureKind::Preparation,
            diagnostics: error.diagnostics().to_vec(),
        })?;
    let spec = WorkerSpecV4::new(
        node.executable().map_err(|item| failure(CommandFailureKind::Preparation, item))?,
        vec![node_compatible_path(&worker).into_os_string()],
        node_compatible_path(adapter),
        expected,
        WorkerLimitsV4::default(),
    )
    .map_err(|error| CommandFailure {
        kind: CommandFailureKind::Preparation,
        diagnostics: error.diagnostics().to_vec(),
    })?;
    Ok(WorkerFrontendV4::new(spec))
}

fn validate_request(request: &DataOwnershipBuildRequest) -> Result<(), CommandFailure> {
    validate_request_shape(request)?;
    let report = check_workspace(&request.workspace_root);
    if !report.is_valid() {
        return Err(CommandFailure {
            kind: CommandFailureKind::Architecture,
            diagnostics: report.diagnostics,
        });
    }
    Ok(())
}

fn validate_request_shape(request: &DataOwnershipBuildRequest) -> Result<(), CommandFailure> {
    if request.targets.component() {
        return Err(request_error(
            "component emission currently accepts only the default scalar profile",
        ));
    }
    if !request.workspace_root.is_absolute() {
        return Err(request_error("candidate workspace root must be absolute"));
    }
    NormalizedSourcePath::new(request.entrypoint.clone()).map_err(|error| {
        failure(CommandFailureKind::Request, Diagnostic::from_source_error(&error))
    })?;
    crate::javascript::validate_artifact_stem(&request.artifact_stem)
        .map_err(|item| failure(CommandFailureKind::Request, item))?;
    Ok(())
}

fn closure_failure(error: &ModuleClosureError) -> CommandFailure {
    let diagnostics = match &error {
        ModuleClosureError::Frontend(worker) if worker.diagnostics().is_empty() => {
            vec![Diagnostic::error(
                worker.code(),
                None,
                worker.to_string(),
                "verify the pinned Node.js runtime and exact-v4 TypeScript frontend",
            )]
        }
        _ => error.diagnostics().to_vec(),
    };
    CommandFailure { kind: CommandFailureKind::Source, diagnostics }
}

fn request_error(message: impl Into<String>) -> CommandFailure {
    failure(
        CommandFailureKind::Request,
        Diagnostic::error(
            "ZRYNA-C3001",
            None,
            message,
            "use the exact internal DataOwnershipV1 candidate configuration",
        ),
    )
}

fn preparation_error(message: impl Into<String>) -> CommandFailure {
    let mut error = request_error(message);
    error.kind = CommandFailureKind::Preparation;
    error
}

fn failure(kind: CommandFailureKind, item: Diagnostic) -> CommandFailure {
    CommandFailure { kind, diagnostics: vec![item] }
}

#[cfg(unix)]
fn is_link_like(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
fn is_link_like(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(any(unix, windows)))]
fn is_link_like(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

use crate::ModuleClosureError;

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests;
