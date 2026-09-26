//! Atomic one-entrypoint build and run orchestration.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use cap_std::{ambient_authority, fs::Dir};
use same_file::Handle;
use serde::Serialize;
use sha2::{Digest, Sha256};
use zryna_abi::{ScalarHostErrorCode, ScalarOutcome, ScalarTarget, ScalarValue};
use zryna_diagnostics::Diagnostic;
use zryna_frontend::VerifiedFrontendProviderV3;
use zryna_frontend::{ProviderExpectationV3, WorkerFrontendV3, WorkerLimitsV3, WorkerSpecV3};
use zryna_source::{MAX_SOURCE_FILE_BYTES, NormalizedSourcePath, SourceFileInput, SourceMap};

use crate::{
    ArtifactOutputRoot, NativeProcessLimits, SourceToIrError, WorkspaceSourceRoot,
    discover_linux_native_toolchain, discover_module_closure,
    javascript::validate_artifact_stem,
    native::{
        prepare_control_flow_native_invocation_from_verified,
        prepare_native_invocation_from_verified, run_prepared_native_invocation,
    },
    pipeline_runtime::{
        invoke_scalar_esm, normalize_carrier, normalize_frame, render_javascript_harness,
        render_webassembly_harness,
    },
    runtime::{NodeRuntimeCapability, node_compatible_path},
};

mod artifacts;
mod control_flow;
mod preparation;
mod project;
mod scalar;
use artifacts::write_prepared_artifacts;
pub(crate) use control_flow::execute_installed as execute_installed_control_flow;
use preparation::{PreparedArtifacts, analyze, configured_frontend};
pub(crate) use project::{build_project_request, run_project_request};
pub(crate) use scalar::execute_installed as execute_installed_scalar;

const MANIFEST_NAME: &str = "zryna-manifest-v1.json";
const MANIFEST_PROFILE: &str = "zryna-m1-cli-v1";
const BROWSER_MANIFEST_NAME: &str = "zryna-browser-manifest-v1.json";
const BROWSER_MANIFEST_PROFILE: &str = "zryna-browser-component-v1";
const CONTROL_FLOW_MANIFEST_NAME: &str = "zryna-manifest-v2.json";
const CONTROL_FLOW_MANIFEST_PROFILE: &str = "zryna-control-flow-v1";
const MAX_CONTROL_FLOW_MANIFEST_BYTES: usize = 32 * 1_024 * 1_024;
const NATIVE_TARGET: &str = "x86_64-unknown-linux-gnu";
const TRANSACTION_PREFIX: &str = ".zryna-transaction-";
static NEXT_TRANSACTION: AtomicU64 = AtomicU64::new(0);

/// Closed CLI target selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetSelection {
    /// Direct ECMAScript module.
    JavaScript,
    /// Import-free core WebAssembly module.
    WebAssembly,
    /// Linux x86-64 native object or invocation executable.
    Native,
    /// Audited WebAssembly Component Model artifact for the default scalar profile.
    Component,
    /// Explicit capability-free browser bundle with generated scalar bindings.
    BrowserComponent,
    /// Every target in canonical order.
    All,
}

impl TargetSelection {
    pub(crate) fn javascript(self) -> bool {
        matches!(self, Self::JavaScript | Self::All)
    }

    pub(crate) fn webassembly(self) -> bool {
        matches!(self, Self::WebAssembly | Self::All)
    }

    pub(crate) fn native(self) -> bool {
        matches!(self, Self::Native | Self::All)
    }

    pub(crate) fn component(self) -> bool {
        matches!(self, Self::Component | Self::BrowserComponent)
    }

    fn ordered(self) -> Vec<ManifestTarget> {
        let mut targets = Vec::with_capacity(if self == Self::All { 3 } else { 1 });
        if self.javascript() {
            targets.push(ManifestTarget::JavaScript);
        }
        if self.webassembly() {
            targets.push(ManifestTarget::WebAssembly);
        }
        if self.native() {
            targets.push(ManifestTarget::Native);
        }
        if self.component() {
            targets.push(ManifestTarget::Component);
        }
        targets
    }
}

impl fmt::Display for TargetSelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::JavaScript => "javascript",
            Self::WebAssembly => "webassembly",
            Self::Native => "native",
            Self::Component | Self::BrowserComponent => "component",
            Self::All => "all",
        })
    }
}

/// One build request after CLI parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildRequest {
    /// Absolute canonical workspace root.
    pub workspace_root: PathBuf,
    /// Portable workspace-relative `.zry` path.
    pub entrypoint: String,
    /// Portable output stem.
    pub artifact_stem: String,
    /// Explicit target selection.
    pub targets: TargetSelection,
    /// Absolute direct Node.js executable used by the frontend and target runtimes.
    pub node_runtime: PathBuf,
}

/// One run request after CLI parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunRequest {
    /// Shared build configuration.
    pub build: BuildRequest,
    /// Exact logical scalar export.
    pub logical_export: String,
    /// Ordered typed arguments.
    pub arguments: Vec<ScalarValue>,
}

/// One explicit multi-file `ControlFlowV1` build request.
///
/// This remains separate from [`BuildRequest`] so existing M1 Rust callers and struct literals do
/// not acquire a new field or silently change profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlFlowBuildRequest {
    /// Absolute canonical workspace root.
    pub workspace_root: PathBuf,
    /// Portable workspace-relative `.zry` entry module.
    pub entrypoint: String,
    /// Portable output stem.
    pub artifact_stem: String,
    /// Explicit target selection.
    pub targets: TargetSelection,
    /// Absolute direct Node.js executable used by the frontend and target runtimes.
    pub node_runtime: PathBuf,
}

/// One explicit multi-file `ControlFlowV1` run request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlFlowRunRequest {
    /// Shared M2 build configuration.
    pub build: ControlFlowBuildRequest,
    /// Exact logical scalar export.
    pub logical_export: String,
    /// Ordered canonical typed arguments.
    pub arguments: Vec<ScalarValue>,
}

/// Executed command kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandKind {
    /// Artifact-only build.
    Build,
    /// Artifact build plus typed execution.
    Run,
}

impl CommandKind {
    const fn suffix(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Run => "run",
        }
    }
}

/// Stable coarse command failure category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandFailureKind {
    /// Mandatory workspace architecture failed.
    Architecture,
    /// Request paths, names, or configuration failed validation.
    Request,
    /// Frontend, source, semantic, ABI, or IR verification rejected the input.
    Source,
    /// Backend, toolchain, audit, transaction, or commit failed.
    Preparation,
    /// Target execution or result framing failed.
    Execution,
    /// Process-tree or transaction cleanup could not be confirmed.
    Cleanup,
}

impl CommandFailureKind {
    /// Returns the stable process exit status.
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Architecture => 1,
            Self::Request => 2,
            Self::Source => 3,
            Self::Preparation => 4,
            Self::Execution => 5,
            Self::Cleanup => 6,
        }
    }
}

/// Failed command with deterministic diagnostics.
#[derive(Clone, Debug)]
pub struct CommandFailure {
    pub(crate) kind: CommandFailureKind,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl CommandFailure {
    /// Returns the stable coarse failure kind.
    #[must_use]
    pub const fn kind(&self) -> CommandFailureKind {
        self.kind
    }

    /// Returns deterministic diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

impl fmt::Display for CommandFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Zryna command failed with {} diagnostic(s)", self.diagnostics.len())
    }
}

impl std::error::Error for CommandFailure {}

/// One artifact recorded in the committed manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublishedTargetArtifact {
    target: ManifestTarget,
    kind: &'static str,
    path: String,
    bytes: u64,
    sha256: String,
}

impl PublishedTargetArtifact {
    /// Returns the target label.
    #[must_use]
    pub const fn target(&self) -> &'static str {
        self.target.as_str()
    }

    /// Returns the bundle-relative artifact path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// One ordered typed target observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetResult {
    target: ManifestTarget,
    outcome: ScalarOutcome,
}

impl TargetResult {
    /// Returns the target label.
    #[must_use]
    pub const fn target(&self) -> &'static str {
        self.target.as_str()
    }

    /// Returns the normalized typed outcome.
    #[must_use]
    pub const fn outcome(&self) -> ScalarOutcome {
        self.outcome
    }
}

/// Successful atomic build or run.
#[derive(Clone, Debug)]
pub struct CommandSuccess {
    command: CommandKind,
    manifest_path: PathBuf,
    manifest_portable_path: String,
    artifacts: Vec<PublishedTargetArtifact>,
    results: Vec<TargetResult>,
    diagnostics: Vec<Diagnostic>,
}

impl CommandSuccess {
    /// Returns the completed command kind.
    #[must_use]
    pub const fn command(&self) -> CommandKind {
        self.command
    }

    /// Returns the absolute committed manifest path.
    #[must_use]
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// Returns the portable workspace-relative committed manifest path.
    #[must_use]
    pub fn manifest_portable_path(&self) -> &str {
        &self.manifest_portable_path
    }

    /// Returns canonical target artifacts.
    #[must_use]
    pub fn artifacts(&self) -> &[PublishedTargetArtifact] {
        &self.artifacts
    }

    /// Returns canonical typed observations; builds return an empty slice.
    #[must_use]
    pub fn results(&self) -> &[TargetResult] {
        &self.results
    }

    /// Returns non-fatal diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
enum ManifestTarget {
    JavaScript,
    WebAssembly,
    Native,
    Component,
}

impl ManifestTarget {
    const fn as_str(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::WebAssembly => "webassembly",
            Self::Native => "native",
            Self::Component => "component",
        }
    }
}

#[derive(Serialize)]
struct Manifest<'a> {
    version: u8,
    profile: &'static str,
    command: CommandKind,
    entrypoint: &'a str,
    source_sha256: String,
    stem: &'a str,
    targets: Vec<ManifestTarget>,
    artifacts: &'a [PublishedTargetArtifact],
    invocation: Option<ManifestInvocation<'a>>,
    results: Vec<ManifestResult>,
    diagnostics: &'a [Diagnostic],
    #[serde(skip_serializing_if = "Option::is_none")]
    browser: Option<BrowserManifest>,
}

#[derive(Serialize)]
struct BrowserManifest {
    revision: &'static str,
    world: &'static str,
    wit_source_sha256: String,
    component_sha256: String,
    core_sha256: String,
    interface_sha256: String,
    core_offset: usize,
}

#[derive(Serialize)]
struct ManifestInvocation<'a> {
    export: &'a str,
    arguments: &'a [ScalarValue],
}

#[derive(Serialize)]
struct ManifestResult {
    target: ManifestTarget,
    outcome: ManifestOutcome,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum ManifestOutcome {
    Returned {
        #[serde(rename = "type")]
        ty: &'static str,
        value: ManifestScalar,
    },
    Trapped {
        code: &'static str,
    },
    HostError {
        code: &'static str,
    },
}

#[derive(Serialize)]
#[serde(untagged)]
enum ManifestScalar {
    Bool(bool),
    I32(i32),
}

#[derive(Serialize)]
struct ControlFlowManifest<'a> {
    version: u8,
    profile: &'static str,
    command: CommandKind,
    entrypoint: &'a str,
    graph_sha256: String,
    sources: Vec<ControlFlowManifestSource<'a>>,
    edges: Vec<ControlFlowManifestEdge<'a>>,
    stem: &'a str,
    targets: Vec<ManifestTarget>,
    artifacts: &'a [PublishedTargetArtifact],
    invocation: Option<ManifestInvocation<'a>>,
    results: Vec<ManifestResult>,
    diagnostics: &'a [Diagnostic],
}

#[derive(Serialize)]
struct ControlFlowManifestSource<'a> {
    id: u32,
    path: &'a str,
    sha256: String,
}

#[derive(Serialize)]
struct ControlFlowManifestEdge<'a> {
    importer: &'a str,
    target: &'a str,
    specifier: &'a str,
    imported: &'a str,
    local: &'a str,
}

struct PreparedControlFlowArtifacts {
    javascript: Option<crate::scalar_adapter_interface::VerifiedScalarEsm>,
    webassembly: Option<zryna_backend_webassembly::ValidatedWebAssemblyArtifact>,
    native_object:
        Option<zryna_backend_native::control_flow_v1::ValidatedControlFlowNativeObjectArtifact>,
    native_executable: Option<crate::native::PreparedNativeExecutable>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlFlowPhase {
    Discovery,
    Semantics,
    JavaScriptBackend,
    WebAssemblyBackend,
    NativeMir,
    NativeObject,
    NativeLink,
    JavaScriptExecution,
    WebAssemblyExecution,
    NativeExecution,
    ArtifactStaging,
    ManifestRendering,
    Publication,
}

type ControlFlowCheckpoint<'a> = &'a dyn Fn(ControlFlowPhase) -> Result<(), CommandFailure>;

#[allow(clippy::unnecessary_wraps)]
fn allow_control_flow_phase(_phase: ControlFlowPhase) -> Result<(), CommandFailure> {
    Ok(())
}

/// Builds one entrypoint and atomically commits one complete target bundle.
///
/// # Errors
///
/// Returns stable categorized diagnostics without advertising a partial bundle.
pub fn build_workspace(request: &BuildRequest) -> Result<CommandSuccess, CommandFailure> {
    execute(request, None)
}

/// Builds, executes, and atomically commits one complete target bundle.
///
/// # Errors
///
/// Returns stable categorized diagnostics without advertising a partial bundle.
pub fn run_workspace(request: RunRequest) -> Result<CommandSuccess, CommandFailure> {
    let invocation =
        RunInvocation { logical_export: request.logical_export, arguments: request.arguments };
    execute(&request.build, Some(&invocation))
}

/// Builds one authenticated M2 module graph and atomically commits one complete target bundle.
///
/// # Errors
///
/// Returns stable categorized diagnostics without advertising a partial bundle.
pub fn build_control_flow_workspace(
    request: &ControlFlowBuildRequest,
) -> Result<CommandSuccess, CommandFailure> {
    execute_control_flow(request, None)
}

/// Builds and executes one authenticated M2 module graph, then atomically commits its bundle.
///
/// # Errors
///
/// Returns stable categorized diagnostics without advertising a partial bundle.
pub fn run_control_flow_workspace(
    request: ControlFlowRunRequest,
) -> Result<CommandSuccess, CommandFailure> {
    let invocation =
        RunInvocation { logical_export: request.logical_export, arguments: request.arguments };
    execute_control_flow(&request.build, Some(&invocation))
}

#[allow(clippy::too_many_lines)]
fn execute_control_flow(
    request: &ControlFlowBuildRequest,
    run: Option<&RunInvocation>,
) -> Result<CommandSuccess, CommandFailure> {
    execute_control_flow_with_frontend_factory(request, run, configured_frontend_v3)
}

#[allow(clippy::too_many_lines)]
fn execute_control_flow_with_frontend_factory<Provider, Factory>(
    request: &ControlFlowBuildRequest,
    run: Option<&RunInvocation>,
    frontend_factory: Factory,
) -> Result<CommandSuccess, CommandFailure>
where
    Provider: VerifiedFrontendProviderV3,
    Factory: FnOnce(
        &ControlFlowBuildRequest,
        &NodeRuntimeCapability,
    ) -> Result<Provider, CommandFailure>,
{
    execute_control_flow_with_frontend_factory_and_checkpoint(
        request,
        run,
        frontend_factory,
        &allow_control_flow_phase,
    )
}

#[allow(clippy::too_many_lines)]
fn execute_control_flow_with_frontend_factory_and_checkpoint<Provider, Factory>(
    request: &ControlFlowBuildRequest,
    run: Option<&RunInvocation>,
    frontend_factory: Factory,
    checkpoint: ControlFlowCheckpoint<'_>,
) -> Result<CommandSuccess, CommandFailure>
where
    Provider: VerifiedFrontendProviderV3,
    Factory: FnOnce(
        &ControlFlowBuildRequest,
        &NodeRuntimeCapability,
    ) -> Result<Provider, CommandFailure>,
{
    let command = if run.is_some() { CommandKind::Run } else { CommandKind::Build };
    validate_architecture(&request.workspace_root)?;
    if request.targets.component() {
        return Err(unsupported_component_request(
            "component emission currently accepts only the default scalar profile",
        ));
    }
    let compatibility_request = BuildRequest {
        workspace_root: request.workspace_root.clone(),
        entrypoint: request.entrypoint.clone(),
        artifact_stem: request.artifact_stem.clone(),
        targets: request.targets,
        node_runtime: request.node_runtime.clone(),
    };
    validate_request(&compatibility_request, command)?;
    let output_root = ArtifactOutputRoot::prepare_for_workspace(&request.workspace_root)
        .map_err(|diagnostic| failure(CommandFailureKind::Preparation, diagnostic))?;
    let final_bundle =
        output_root.path().join(format!("{}.{}", request.artifact_stem, command.suffix()));
    ensure_absent(&final_bundle)?;

    let entrypoint = NormalizedSourcePath::new(request.entrypoint.clone()).map_err(|error| {
        failure(CommandFailureKind::Request, Diagnostic::from_source_error(&error))
    })?;
    let source_root = WorkspaceSourceRoot::capture(&request.workspace_root)
        .map_err(|diagnostic| failure(CommandFailureKind::Source, diagnostic))?;
    let node = NodeRuntimeCapability::discover(&request.node_runtime, &request.workspace_root)
        .map_err(|diagnostic| failure(CommandFailureKind::Preparation, diagnostic))?;
    let frontend = frontend_factory(request, &node)?;
    checkpoint(ControlFlowPhase::Discovery)?;
    let closure = discover_module_closure(&source_root, entrypoint, &frontend)
        .map_err(|error| module_closure_failure(&error))?;
    control_flow::finish(request, run, &node, &output_root, &final_bundle, &closure, checkpoint)
}

fn compile_control_flow_selected(
    source: &crate::scalar_adapter_interface::VerifiedScalarSource,
    targets: TargetSelection,
    checkpoint: ControlFlowCheckpoint<'_>,
) -> Result<PreparedControlFlowArtifacts, CommandFailure> {
    let program = source.program();
    let javascript = if targets.javascript() {
        checkpoint(ControlFlowPhase::JavaScriptBackend)?;
        Some(source.prepare_node_interface().map_err(preparation_failure)?)
    } else {
        None
    };
    let webassembly = if targets.webassembly() {
        checkpoint(ControlFlowPhase::WebAssemblyBackend)?;
        Some(zryna_backend_webassembly::emit_control_flow(program).map_err(preparation_failure)?)
    } else {
        None
    };
    let native_object = if targets.native() {
        checkpoint(ControlFlowPhase::NativeMir)?;
        let target =
            crate::select_native_object_target(NATIVE_TARGET).map_err(preparation_failure)?;
        let mir = zryna_native_mir::control_flow_v1::lower(program).map_err(|diagnostics| {
            CommandFailure { kind: CommandFailureKind::Preparation, diagnostics }
        })?;
        checkpoint(ControlFlowPhase::NativeObject)?;
        Some(
            zryna_backend_native::control_flow_v1::emit_object(&mir, target)
                .map_err(preparation_failure)?,
        )
    } else {
        None
    };
    Ok(PreparedControlFlowArtifacts {
        javascript,
        webassembly,
        native_object,
        native_executable: None,
    })
}

fn module_closure_failure(error: &crate::ModuleClosureError) -> CommandFailure {
    let diagnostics = match error {
        crate::ModuleClosureError::Frontend(worker) if worker.diagnostics().is_empty() => {
            vec![Diagnostic::error(
                worker.code(),
                None,
                worker.to_string(),
                "verify the pinned Node.js runtime and TypeScript frontend, then retry",
            )]
        }
        _ => error.diagnostics().to_vec(),
    };
    CommandFailure { kind: CommandFailureKind::Source, diagnostics }
}

struct RunInvocation {
    logical_export: String,
    arguments: Vec<ScalarValue>,
}

fn execute(
    request: &BuildRequest,
    run: Option<&RunInvocation>,
) -> Result<CommandSuccess, CommandFailure> {
    execute_with_roots(request, run, &request.workspace_root, None)
}

fn execute_with_roots(
    request: &BuildRequest,
    run: Option<&RunInvocation>,
    compiler_root: &Path,
    admission: Option<&crate::project::ProjectAdmission>,
) -> Result<CommandSuccess, CommandFailure> {
    validate_architecture(compiler_root)?;
    execute_after_architecture(request, run, compiler_root, admission)
}

fn execute_after_architecture(
    request: &BuildRequest,
    run: Option<&RunInvocation>,
    compiler_root: &Path,
    admission: Option<&crate::project::ProjectAdmission>,
) -> Result<CommandSuccess, CommandFailure> {
    let command = if run.is_some() { CommandKind::Run } else { CommandKind::Build };
    let validated = validate_request(request, command)?;
    let output_root = ArtifactOutputRoot::prepare_for_workspace(&request.workspace_root)
        .map_err(|diagnostic| failure(CommandFailureKind::Preparation, diagnostic))?;
    let final_bundle =
        output_root.path().join(format!("{}.{}", request.artifact_stem, command.suffix()));
    ensure_absent(&final_bundle)?;

    let source_text = match admission {
        Some(admission) => admission.entrypoint_text()?,
        None => read_entrypoint(&validated.source_path)?,
    };
    let sources = SourceMap::build(vec![SourceFileInput {
        path: request.entrypoint.clone(),
        text: source_text.clone(),
    }])
    .map_err(|error| failure(CommandFailureKind::Source, Diagnostic::from_source_error(&error)))?;
    let node = NodeRuntimeCapability::discover(&request.node_runtime, compiler_root)
        .map_err(|diagnostic| failure(CommandFailureKind::Preparation, diagnostic))?;
    let frontend = configured_frontend(compiler_root, &node)?;
    let compiled = analyze(&frontend, &sources)?;
    if let Some(admission) = admission {
        admission.revalidate()?;
    }
    scalar::finish(
        request,
        run,
        &node,
        &output_root,
        &final_bundle,
        &source_text,
        &compiled,
        &|| match admission {
            Some(admission) => admission.revalidate(),
            None => Ok(()),
        },
    )
}

struct ValidatedRequest {
    source_path: PathBuf,
}

fn validate_architecture(root: &Path) -> Result<(), CommandFailure> {
    let report = crate::check_workspace(root);
    if report.is_valid() {
        Ok(())
    } else {
        Err(CommandFailure {
            kind: CommandFailureKind::Architecture,
            diagnostics: report.diagnostics,
        })
    }
}

fn validate_request(
    request: &BuildRequest,
    command: CommandKind,
) -> Result<ValidatedRequest, CommandFailure> {
    if !request.workspace_root.is_absolute() {
        return Err(request_error(
            "ZRYNA-C1001",
            "workspace root must be absolute",
            "resolve --root to an absolute real directory before dispatch",
        ));
    }
    if command == CommandKind::Run && request.targets.component() {
        return Err(unsupported_component_request(
            "component artifacts are build-only until a reviewed host profile is activated",
        ));
    }
    validate_artifact_stem(&request.artifact_stem)
        .map_err(|diagnostic| failure(CommandFailureKind::Request, diagnostic))?;
    if request.entrypoint.contains('\\') {
        return Err(entrypoint_error("entrypoint must use portable forward slashes"));
    }
    let has_noncanonical_segment = request
        .entrypoint
        .split('/')
        .any(|segment| segment.is_empty() || matches!(segment, "." | ".."));
    let entry = Path::new(&request.entrypoint);
    if has_noncanonical_segment
        || entry.is_absolute()
        || entry.extension().and_then(|value| value.to_str()) != Some("zry")
        || entry.components().any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(entrypoint_error(
            "entrypoint must be one portable workspace-relative .zry path",
        ));
    }
    validate_real_directory(&request.workspace_root)
        .map_err(|diagnostic| failure(CommandFailureKind::Request, diagnostic))?;
    let source_path = request.workspace_root.join(entry);
    validate_source_chain(&request.workspace_root, &source_path)?;
    Ok(ValidatedRequest { source_path })
}

fn validate_source_chain(root: &Path, source: &Path) -> Result<(), CommandFailure> {
    let parent = source.parent().ok_or_else(|| entrypoint_error("entrypoint has no parent"))?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| entrypoint_error("entrypoint escapes the workspace root"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(entrypoint_error("entrypoint contains an unsafe path component"));
        };
        current.push(component);
        validate_real_directory(&current)
            .map_err(|diagnostic| failure(CommandFailureKind::Request, diagnostic))?;
    }
    let metadata = fs::symlink_metadata(source)
        .map_err(|_| entrypoint_error("entrypoint could not be inspected"))?;
    if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
        return Err(entrypoint_error("entrypoint is not a real regular file"));
    }
    Ok(())
}

fn validate_real_directory(path: &Path) -> Result<(), Diagnostic> {
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        Diagnostic::error(
            "ZRYNA-C1002",
            None,
            "workspace path could not be inspected",
            "use an existing real directory without links or reparse points",
        )
    })?;
    if metadata.is_dir() && !metadata_is_link_or_reparse(&metadata) {
        Ok(())
    } else {
        Err(Diagnostic::error(
            "ZRYNA-C1002",
            None,
            "workspace path is not a real directory",
            "use an existing real directory without links or reparse points",
        ))
    }
}

fn read_entrypoint(path: &Path) -> Result<String, CommandFailure> {
    read_entrypoint_impl(path, || {})
}

#[cfg(all(test, unix))]
fn read_entrypoint_with_after_read(
    path: &Path,
    after_read: impl FnOnce(),
) -> Result<String, CommandFailure> {
    read_entrypoint_impl(path, after_read)
}

fn read_entrypoint_impl(path: &Path, after_read: impl FnOnce()) -> Result<String, CommandFailure> {
    let file = open_regular_no_follow(path)
        .map_err(|_| entrypoint_error("entrypoint could not be opened safely"))?;
    let mut handle = Handle::from_file(file)
        .map_err(|_| entrypoint_error("entrypoint identity could not be established"))?;
    let opened = handle
        .as_file()
        .metadata()
        .map_err(|_| entrypoint_error("entrypoint metadata could not be read"))?;
    if !opened.is_file() || metadata_is_link_or_reparse(&opened) {
        return Err(entrypoint_error("entrypoint changed before it could be read"));
    }
    if opened.len() > u64::try_from(MAX_SOURCE_FILE_BYTES).unwrap_or(u64::MAX) {
        return Err(source_size_error());
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(opened.len()).unwrap_or(MAX_SOURCE_FILE_BYTES).min(MAX_SOURCE_FILE_BYTES),
    );
    handle
        .as_file_mut()
        .take(u64::try_from(MAX_SOURCE_FILE_BYTES).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| entrypoint_error("entrypoint could not be read completely"))?;
    if bytes.len() > MAX_SOURCE_FILE_BYTES {
        return Err(source_size_error());
    }
    after_read();
    let after = handle
        .as_file()
        .metadata()
        .map_err(|_| entrypoint_error("entrypoint changed while it was read"))?;
    let current_file = open_regular_no_follow(path)
        .map_err(|_| entrypoint_error("entrypoint changed while it was read"))?;
    let current = Handle::from_file(current_file)
        .map_err(|_| entrypoint_error("entrypoint identity could not be revalidated"))?;
    let current_metadata = current
        .as_file()
        .metadata()
        .map_err(|_| entrypoint_error("entrypoint metadata could not be revalidated"))?;
    if handle != current
        || !same_file_state(&opened, &after)
        || !same_file_state(&opened, &current_metadata)
    {
        return Err(entrypoint_error("entrypoint changed while it was read"));
    }
    String::from_utf8(bytes).map_err(|_| entrypoint_error("entrypoint is not valid UTF-8"))
}

#[cfg(unix)]
fn open_regular_no_follow(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = fs::OpenOptions::new();
    options.read(true).custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK);
    options.open(path)
}

#[cfg(windows)]
fn open_regular_no_follow(path: &Path) -> io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    let mut options = fs::OpenOptions::new();
    options.read(true).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).share_mode(FILE_SHARE_READ);
    options.open(path)
}

#[cfg(not(any(unix, windows)))]
fn open_regular_no_follow(_path: &Path) -> io::Result<fs::File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no fail-closed no-follow strategy exists for this platform",
    ))
}

#[cfg(unix)]
fn same_file_state(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    left.size() == right.size()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

#[cfg(windows)]
fn same_file_state(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    left.file_attributes() == right.file_attributes()
        && left.creation_time() == right.creation_time()
        && left.last_write_time() == right.last_write_time()
        && left.file_size() == right.file_size()
}

#[cfg(not(any(unix, windows)))]
fn same_file_state(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

fn source_size_error() -> CommandFailure {
    failure(
        CommandFailureKind::Source,
        Diagnostic::error(
            "ZRYNA-S1003",
            None,
            "source file exceeds the fixed per-file byte budget",
            "reduce the source file before analysis",
        ),
    )
}

fn configured_frontend_v3(
    request: &ControlFlowBuildRequest,
    node: &NodeRuntimeCapability,
) -> Result<WorkerFrontendV3, CommandFailure> {
    let adapter_root = request.workspace_root.join("adapters/typescript-6");
    validate_real_directory(&adapter_root)
        .map_err(|diagnostic| failure(CommandFailureKind::Preparation, diagnostic))?;
    let worker_entrypoint = adapter_root.join("src/worker-v3.mjs");
    let worker_metadata = fs::symlink_metadata(&worker_entrypoint).map_err(|_| {
        entrypoint_error("TypeScript frontend worker entrypoint is unavailable")
            .with_kind(CommandFailureKind::Preparation)
    })?;
    if !worker_metadata.is_file() || metadata_is_link_or_reparse(&worker_metadata) {
        return Err(entrypoint_error(
            "TypeScript frontend worker entrypoint is not a real regular file",
        )
        .with_kind(CommandFailureKind::Preparation));
    }
    let node_adapter_root = node_compatible_path(&adapter_root);
    let node_worker_entrypoint = node_compatible_path(&worker_entrypoint);
    let expected =
        ProviderExpectationV3::new("typescript-6", "6.0.3").map_err(|error| CommandFailure {
            kind: CommandFailureKind::Preparation,
            diagnostics: error.diagnostics().to_vec(),
        })?;
    let spec = WorkerSpecV3::new(
        node.executable().map_err(preparation_failure)?,
        vec![node_worker_entrypoint.into_os_string()],
        node_adapter_root,
        expected,
        WorkerLimitsV3::default(),
    )
    .map_err(|error| CommandFailure {
        kind: CommandFailureKind::Preparation,
        diagnostics: error.diagnostics().to_vec(),
    })?;
    Ok(WorkerFrontendV3::new(spec))
}

#[allow(clippy::too_many_arguments)]
fn commit_control_flow_prepared(
    request: &ControlFlowBuildRequest,
    command: CommandKind,
    run: Option<&RunInvocation>,
    invocation: Option<&zryna_abi::VerifiedInvocation<'_>>,
    node: &NodeRuntimeCapability,
    output_root: &ArtifactOutputRoot,
    final_bundle: &Path,
    closure: &crate::VerifiedModuleClosure,
    prepared: &PreparedControlFlowArtifacts,
    diagnostics: Vec<Diagnostic>,
    checkpoint: ControlFlowCheckpoint<'_>,
) -> Result<CommandSuccess, CommandFailure> {
    checkpoint(ControlFlowPhase::ArtifactStaging)?;
    let mut transaction = Transaction::create(output_root)?;
    let operation: Result<CommandSuccess, CommandFailure> = (|| {
        let artifacts = write_control_flow_artifacts(&transaction, request, command, prepared)?;
        let results = if let Some(invocation) = invocation {
            execute_control_flow_targets(
                request,
                node,
                output_root,
                &transaction,
                prepared,
                invocation,
                checkpoint,
            )?
        } else {
            Vec::new()
        };
        let sources = closure
            .modules()
            .iter()
            .map(|module| ControlFlowManifestSource {
                id: module.id(),
                path: module.path().as_str(),
                sha256: hex_sha256(module.source_sha256()),
            })
            .collect::<Vec<_>>();
        let edges = closure
            .edges()
            .iter()
            .map(|edge| ControlFlowManifestEdge {
                importer: edge.importer().as_str(),
                target: edge.target().as_str(),
                specifier: edge.specifier(),
                imported: edge.imported(),
                local: edge.local(),
            })
            .collect::<Vec<_>>();
        let manifest = ControlFlowManifest {
            version: 2,
            profile: CONTROL_FLOW_MANIFEST_PROFILE,
            command,
            entrypoint: closure.entrypoint().as_str(),
            graph_sha256: hex_sha256(closure.graph_sha256()),
            sources,
            edges,
            stem: &request.artifact_stem,
            targets: request.targets.ordered(),
            artifacts: &artifacts,
            invocation: run.map(|value| ManifestInvocation {
                export: &value.logical_export,
                arguments: &value.arguments,
            }),
            results: results.iter().map(manifest_result).collect(),
            diagnostics: &diagnostics,
        };
        checkpoint(ControlFlowPhase::ManifestRendering)?;
        let manifest_bytes = serialize_control_flow_manifest(&manifest)?;
        transaction.write_manifest(CONTROL_FLOW_MANIFEST_NAME, &manifest_bytes)?;
        checkpoint(ControlFlowPhase::Publication)?;
        transaction.commit(output_root, final_bundle)?;
        Ok(CommandSuccess {
            command,
            manifest_path: final_bundle.join(CONTROL_FLOW_MANIFEST_NAME),
            manifest_portable_path: format!(
                ".zryna/out/{}.{}/{CONTROL_FLOW_MANIFEST_NAME}",
                request.artifact_stem,
                command.suffix()
            ),
            artifacts,
            results,
            diagnostics,
        })
    })();
    match operation {
        Ok(success) => Ok(success),
        Err(mut failure) => {
            if let Err(cleanup) = transaction.cleanup(output_root) {
                failure.kind = CommandFailureKind::Cleanup;
                failure.diagnostics.extend(cleanup.diagnostics);
            }
            Err(failure)
        }
    }
}

fn write_control_flow_artifacts(
    transaction: &Transaction,
    request: &ControlFlowBuildRequest,
    command: CommandKind,
    prepared: &PreparedControlFlowArtifacts,
) -> Result<Vec<PublishedTargetArtifact>, CommandFailure> {
    let mut artifacts = Vec::with_capacity(request.targets.ordered().len());
    if let Some(artifact) = &prepared.javascript {
        artifacts.push(transaction.write_artifact(
            ManifestTarget::JavaScript,
            "ecmascript-module",
            &request.artifact_stem,
            "mjs",
            artifact.javascript_source().map_err(preparation_failure)?.as_bytes(),
        )?);
    }
    if let Some(artifact) = &prepared.webassembly {
        artifacts.push(transaction.write_artifact(
            ManifestTarget::WebAssembly,
            "core-webassembly-module",
            &request.artifact_stem,
            "wasm",
            artifact.bytes(),
        )?);
    }
    if request.targets.native() {
        let (kind, extension, bytes): (&str, &str, &[u8]) = if command == CommandKind::Run {
            let executable = prepared.native_executable.as_ref().ok_or_else(|| {
                request_error(
                    "ZRYNA-C1010",
                    "M2 native executable preparation was not completed",
                    "report this compiler invariant failure",
                )
            })?;
            ("linux-x86-64-invocation-executable", "elf", executable.bytes())
        } else {
            let object = prepared.native_object.as_ref().ok_or_else(|| {
                request_error(
                    "ZRYNA-C1010",
                    "M2 native object preparation was not completed",
                    "report this compiler invariant failure",
                )
            })?;
            ("linux-x86-64-relocatable-object", "o", object.bytes())
        };
        artifacts.push(transaction.write_artifact(
            ManifestTarget::Native,
            kind,
            &request.artifact_stem,
            extension,
            bytes,
        )?);
    }
    Ok(artifacts)
}

fn execute_control_flow_targets(
    request: &ControlFlowBuildRequest,
    node: &NodeRuntimeCapability,
    output_root: &ArtifactOutputRoot,
    transaction: &Transaction,
    prepared: &PreparedControlFlowArtifacts,
    invocation: &zryna_abi::VerifiedInvocation<'_>,
    checkpoint: ControlFlowCheckpoint<'_>,
) -> Result<Vec<TargetResult>, CommandFailure> {
    let mut results = Vec::with_capacity(request.targets.ordered().len());
    if request.targets.javascript() {
        checkpoint(ControlFlowPhase::JavaScriptExecution)?;
        let outcome =
            invoke_scalar_esm(prepared.javascript.as_ref(), node, transaction.path(), invocation)?;
        results.push(TargetResult { target: ManifestTarget::JavaScript, outcome });
    }
    if request.targets.webassembly() {
        checkpoint(ControlFlowPhase::WebAssemblyExecution)?;
        let harness = render_webassembly_harness(&request.artifact_stem, invocation)?;
        let artifact = prepared.webassembly.as_ref().ok_or_else(|| {
            request_error(
                "ZRYNA-C1010",
                "M2 WebAssembly preparation was not completed",
                "report this compiler invariant failure",
            )
        })?;
        let frame = node
            .run_webassembly_module(&harness, artifact.bytes(), transaction.path())
            .map_err(runtime_failure)?;
        results.push(TargetResult {
            target: ManifestTarget::WebAssembly,
            outcome: normalize_frame(
                ScalarTarget::CoreWebAssembly,
                invocation.export().result(),
                frame,
            ),
        });
    }
    if request.targets.native() {
        checkpoint(ControlFlowPhase::NativeExecution)?;
        let executable = prepared.native_executable.as_ref().ok_or_else(|| {
            request_error(
                "ZRYNA-C1010",
                "M2 native executable preparation was not completed",
                "report this compiler invariant failure",
            )
        })?;
        let outcome =
            run_prepared_native_invocation(executable, output_root, NativeProcessLimits::default())
                .map_err(|error| native_runtime_failure(error.diagnostic().clone()))?;
        results.push(TargetResult { target: ManifestTarget::Native, outcome });
    }
    Ok(results)
}

fn serialize_control_flow_manifest(
    manifest: &ControlFlowManifest<'_>,
) -> Result<Vec<u8>, CommandFailure> {
    let mut writer = BoundedManifestWriter::new(MAX_CONTROL_FLOW_MANIFEST_BYTES);
    serde_json::to_writer_pretty(&mut writer, manifest).map_err(|_| {
        request_error(
            "ZRYNA-C1011",
            "bounded M2 manifest serialization failed",
            "reduce the module graph below the documented manifest budget",
        )
    })?;
    writer.finish_line().map_err(|_| {
        request_error(
            "ZRYNA-C1011",
            "bounded M2 manifest serialization failed",
            "reduce the module graph below the documented manifest budget",
        )
    })
}

struct BoundedManifestWriter {
    bytes: Vec<u8>,
    maximum: usize,
}

impl BoundedManifestWriter {
    fn new(maximum: usize) -> Self {
        Self { bytes: Vec::new(), maximum }
    }

    fn finish_line(mut self) -> io::Result<Vec<u8>> {
        self.write_all(b"\n")?;
        Ok(self.bytes)
    }
}

impl Write for BoundedManifestWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self.bytes.len().checked_add(bytes.len()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::OutOfMemory, "M2 manifest length overflow")
        })?;
        if next > self.maximum {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "M2 manifest byte budget exceeded",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn hex_sha256(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn commit_prepared(
    request: &BuildRequest,
    command: CommandKind,
    run: Option<&RunInvocation>,
    invocation: Option<&zryna_abi::VerifiedInvocation<'_>>,
    node: &NodeRuntimeCapability,
    output_root: &ArtifactOutputRoot,
    final_bundle: &Path,
    source_text: &str,
    prepared: &PreparedArtifacts,
    diagnostics: Vec<Diagnostic>,
    checkpoint: &dyn Fn() -> Result<(), CommandFailure>,
) -> Result<CommandSuccess, CommandFailure> {
    checkpoint()?;
    let mut transaction = Transaction::create(output_root)?;
    let operation: Result<CommandSuccess, CommandFailure> = (|| {
        checkpoint()?;
        let (artifacts, core_offset) =
            write_prepared_artifacts(&transaction, request, command, prepared)?;
        if request.targets == TargetSelection::BrowserComponent && core_offset.is_none() {
            return Err(transaction_error("browser component bindings were not prepared"));
        }

        let results = if let Some(invocation) = invocation {
            execute_targets(
                request,
                node,
                output_root,
                &transaction,
                prepared,
                invocation,
                checkpoint,
            )?
        } else {
            Vec::new()
        };
        let manifest_results = results.iter().map(manifest_result).collect::<Vec<_>>();
        let source_sha256 = sha256(source_text.as_bytes());
        checkpoint()?;
        let manifest = Manifest {
            version: 1,
            profile: if request.targets == TargetSelection::BrowserComponent {
                BROWSER_MANIFEST_PROFILE
            } else {
                MANIFEST_PROFILE
            },
            command,
            entrypoint: &request.entrypoint,
            source_sha256,
            stem: &request.artifact_stem,
            targets: request.targets.ordered(),
            artifacts: &artifacts,
            invocation: run.map(|value| ManifestInvocation {
                export: &value.logical_export,
                arguments: &value.arguments,
            }),
            results: manifest_results,
            diagnostics: &diagnostics,
            browser: if let (Some(component), Some(core_offset)) =
                (&prepared.component, core_offset)
            {
                Some(BrowserManifest {
                    revision: crate::browser_component::REVISION,
                    world: component.world_identity(),
                    wit_source_sha256: crate::browser_component::hex(component.wit_source_digest()),
                    component_sha256: crate::browser_component::hex(component.digest()),
                    core_sha256: crate::browser_component::hex(component.core_digest()),
                    interface_sha256: crate::browser_component::hex(component.interface_digest()),
                    core_offset,
                })
            } else {
                None
            },
        };
        let mut manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|_| {
            request_error(
                "ZRYNA-C1011",
                "manifest serialization failed",
                "report this compiler invariant failure",
            )
        })?;
        manifest_bytes.push(b'\n');
        let manifest_name = if request.targets == TargetSelection::BrowserComponent {
            BROWSER_MANIFEST_NAME
        } else {
            MANIFEST_NAME
        };
        transaction.write_manifest(manifest_name, &manifest_bytes)?;
        checkpoint()?;
        transaction.commit(output_root, final_bundle)?;
        Ok(CommandSuccess {
            command,
            manifest_path: final_bundle.join(manifest_name),
            manifest_portable_path: format!(
                ".zryna/out/{}.{}/{manifest_name}",
                request.artifact_stem,
                command.suffix()
            ),
            artifacts,
            results,
            diagnostics,
        })
    })();
    match operation {
        Ok(success) => Ok(success),
        Err(mut failure) => {
            if let Err(cleanup) = transaction.cleanup(output_root) {
                failure.kind = CommandFailureKind::Cleanup;
                failure.diagnostics.extend(cleanup.diagnostics);
            }
            Err(failure)
        }
    }
}

fn execute_targets(
    request: &BuildRequest,
    node: &NodeRuntimeCapability,
    output_root: &ArtifactOutputRoot,
    transaction: &Transaction,
    prepared: &PreparedArtifacts,
    invocation: &zryna_abi::VerifiedInvocation<'_>,
    checkpoint: &dyn Fn() -> Result<(), CommandFailure>,
) -> Result<Vec<TargetResult>, CommandFailure> {
    let mut results = Vec::with_capacity(request.targets.ordered().len());
    if request.targets.javascript() {
        checkpoint()?;
        let harness = render_javascript_harness(&request.artifact_stem, invocation)?;
        let harness_path = transaction.write_runtime_harness("javascript", &harness)?;
        let result_type = invocation.export().result();
        let carrier = node
            .run_javascript_module(&harness_path, transaction.path(), result_type)
            .map_err(runtime_failure)?;
        results.push(TargetResult {
            target: ManifestTarget::JavaScript,
            outcome: normalize_carrier(ScalarTarget::JavaScript, result_type, carrier),
        });
    }
    if request.targets.webassembly() {
        checkpoint()?;
        let harness = render_webassembly_harness(&request.artifact_stem, invocation)?;
        let artifact = prepared.webassembly.as_ref().ok_or_else(|| {
            request_error(
                "ZRYNA-C1010",
                "WebAssembly preparation was not completed",
                "report this compiler invariant failure",
            )
        })?;
        let frame = node
            .run_webassembly_module(&harness, artifact.bytes(), transaction.path())
            .map_err(runtime_failure)?;
        results.push(TargetResult {
            target: ManifestTarget::WebAssembly,
            outcome: normalize_frame(
                ScalarTarget::CoreWebAssembly,
                invocation.export().result(),
                frame,
            ),
        });
    }
    if request.targets.native() {
        checkpoint()?;
        let executable = prepared.native_executable.as_ref().ok_or_else(|| {
            request_error(
                "ZRYNA-C1010",
                "native executable preparation was not completed",
                "report this compiler invariant failure",
            )
        })?;
        let outcome =
            run_prepared_native_invocation(executable, output_root, NativeProcessLimits::default())
                .map_err(|error| native_runtime_failure(error.diagnostic().clone()))?;
        results.push(TargetResult { target: ManifestTarget::Native, outcome });
    }
    checkpoint()?;
    Ok(results)
}

fn unsupported_component_request(message: &'static str) -> CommandFailure {
    request_error(
        "ZRYNA-C1012",
        message,
        "use build --target component without an explicit profile",
    )
}

pub(crate) struct Transaction {
    path: PathBuf,
    stage_name: String,
    identity: Handle,
    output_directory: Dir,
    artifacts: RefCell<BTreeMap<ManifestTarget, BTreeMap<String, StagedArtifact>>>,
    harnesses: RefCell<BTreeSet<String>>,
    manifest: RefCell<Option<StagedPrivateFile>>,
    committed: bool,
}

struct StagedArtifact {
    filename: String,
    bytes: u64,
    sha256: String,
    executable: bool,
}

struct StagedPrivateFile {
    filename: String,
    bytes: u64,
    sha256: String,
}

impl Transaction {
    pub(crate) fn create(output_root: &ArtifactOutputRoot) -> Result<Self, CommandFailure> {
        output_root.revalidate().map_err(preparation_failure)?;
        let output_directory = Dir::open_ambient_dir(output_root.path(), ambient_authority())
            .map_err(|_| transaction_error("could not retain the output-root capability"))?;
        for _ in 0..64 {
            let sequence = NEXT_TRANSACTION.fetch_add(1, Ordering::Relaxed);
            let stage_name = format!("{TRANSACTION_PREFIX}{}-{sequence}", std::process::id());
            let path = output_root.path().join(&stage_name);
            match create_private_directory(&path) {
                Ok(()) => {
                    let Ok(identity) = Handle::from_path(&path) else {
                        let _ = fs::remove_dir(&path);
                        return Err(transaction_error(
                            "could not establish private stage identity",
                        ));
                    };
                    let transaction = Self {
                        path,
                        stage_name,
                        identity,
                        output_directory,
                        artifacts: RefCell::new(BTreeMap::new()),
                        harnesses: RefCell::new(BTreeSet::new()),
                        manifest: RefCell::new(None),
                        committed: false,
                    };
                    transaction.revalidate_stage()?;
                    return Ok(transaction);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(transaction_error("could not create private stage")),
            }
        }
        Err(transaction_error("could not allocate a unique private stage"))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    fn write_artifact(
        &self,
        target: ManifestTarget,
        kind: &'static str,
        stem: &str,
        extension: &str,
        bytes: &[u8],
    ) -> Result<PublishedTargetArtifact, CommandFailure> {
        self.revalidate_stage()?;
        let directory = self.path.join(target.as_str());
        if !self.artifacts.borrow().contains_key(&target) {
            create_owned_directory(&directory)?;
        }
        let filename = format!("{stem}.{extension}");
        let expected_bytes = u64::try_from(bytes.len()).map_err(|_| {
            transaction_error("artifact length could not be represented in the manifest")
        })?;
        let expected_sha256 = sha256(bytes);
        let mut artifacts = self.artifacts.borrow_mut();
        let staged = artifacts.entry(target).or_default();
        if (!staged.is_empty() && target != ManifestTarget::Component)
            || staged.contains_key(&filename)
        {
            return Err(transaction_error("target artifact was staged more than once"));
        }
        staged.insert(
            filename.clone(),
            StagedArtifact {
                filename: filename.clone(),
                bytes: expected_bytes,
                sha256: expected_sha256.clone(),
                executable: target == ManifestTarget::Native && extension == "elf",
            },
        );
        drop(artifacts);
        let path = directory.join(&filename);
        write_complete_file(&path, bytes)?;
        if target == ManifestTarget::Native && extension == "elf" {
            prepare_executable_file(&path)?;
        }
        self.revalidate_stage()?;
        Ok(PublishedTargetArtifact {
            target,
            kind,
            path: format!("{}/{filename}", target.as_str()),
            bytes: expected_bytes,
            sha256: expected_sha256,
        })
    }

    pub(crate) fn write_ownership_artifact(
        &self,
        target: crate::OwnershipTarget,
        kind: &'static str,
        stem: &str,
        extension: &str,
        bytes: &[u8],
    ) -> Result<(), CommandFailure> {
        let target = match target {
            crate::OwnershipTarget::JavaScript => ManifestTarget::JavaScript,
            crate::OwnershipTarget::WebAssembly => ManifestTarget::WebAssembly,
            crate::OwnershipTarget::Native => ManifestTarget::Native,
        };
        self.write_artifact(target, kind, stem, extension, bytes).map(|_| ())
    }

    pub(crate) fn write_runtime_harness(
        &self,
        target: &str,
        bytes: &[u8],
    ) -> Result<PathBuf, CommandFailure> {
        self.revalidate_stage()?;
        let name = format!(".{target}-runtime.mjs");
        if !matches!(name.as_str(), ".javascript-runtime.mjs" | ".webassembly-runtime.mjs")
            || !self.harnesses.borrow_mut().insert(name.clone())
        {
            return Err(transaction_error("runtime harness name is not a closed unique target"));
        }
        let path = self.path.join(&name);
        write_complete_file(&path, bytes)?;
        self.revalidate_stage()?;
        Ok(path)
    }

    pub(crate) fn write_manifest(
        &self,
        manifest_name: &str,
        bytes: &[u8],
    ) -> Result<(), CommandFailure> {
        self.revalidate_stage()?;
        if !matches!(
            manifest_name,
            MANIFEST_NAME
                | BROWSER_MANIFEST_NAME
                | CONTROL_FLOW_MANIFEST_NAME
                | crate::OWNERSHIP_MANIFEST_NAME
        ) || self.manifest.borrow().is_some()
        {
            return Err(transaction_error("manifest name is not a closed unique version"));
        }
        let harnesses = self.harnesses.borrow().iter().cloned().collect::<Vec<_>>();
        for target in harnesses {
            let path = self.path.join(&target);
            match fs::remove_file(&path) {
                Ok(()) => {
                    self.harnesses.borrow_mut().remove(&target);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    self.harnesses.borrow_mut().remove(&target);
                }
                Err(_) => {
                    return Err(transaction_error("could not remove private runtime harness"));
                }
            }
        }
        self.audit_inventory(true)?;
        write_complete_file(&self.path.join(manifest_name), bytes)?;
        self.manifest.borrow_mut().replace(StagedPrivateFile {
            filename: manifest_name.to_owned(),
            bytes: u64::try_from(bytes.len())
                .map_err(|_| transaction_error("manifest length could not be represented"))?,
            sha256: sha256(bytes),
        });
        sync_directory(&self.path)?;
        self.revalidate_stage()?;
        self.audit_inventory(true)
    }

    pub(crate) fn commit(
        &mut self,
        output_root: &ArtifactOutputRoot,
        final_bundle: &Path,
    ) -> Result<(), CommandFailure> {
        self.commit_with_rename_hooks(output_root, final_bundle, || {}, || {})
    }

    #[cfg(test)]
    fn commit_with_before_rename(
        &mut self,
        output_root: &ArtifactOutputRoot,
        final_bundle: &Path,
        before_rename: impl FnOnce(),
    ) -> Result<(), CommandFailure> {
        self.commit_with_rename_hooks(output_root, final_bundle, before_rename, || {})
    }

    fn commit_with_rename_hooks(
        &mut self,
        output_root: &ArtifactOutputRoot,
        final_bundle: &Path,
        before_final_audit: impl FnOnce(),
        after_final_audit: impl FnOnce(),
    ) -> Result<(), CommandFailure> {
        output_root.revalidate().map_err(preparation_failure)?;
        self.revalidate_stage()?;
        self.audit_inventory(true)?;
        ensure_absent(final_bundle)?;
        before_final_audit();
        self.revalidate_stage()?;
        self.audit_inventory(true)?;
        after_final_audit();
        let final_name = final_bundle
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|_| final_bundle.parent() == Some(output_root.path()))
            .ok_or_else(|| transaction_error("final bundle escaped the retained output root"))?;
        let staged_path = self.path.clone();
        rename_create_only(&self.output_directory, &self.stage_name, final_name)?;
        self.path = final_bundle.to_path_buf();
        if let Err(post_commit) = self.revalidate_stage().and_then(|()| self.audit_inventory(true))
        {
            let rollback = rename_create_only(&self.output_directory, final_name, &self.stage_name);
            self.path = staged_path;
            if rollback.is_err() {
                return Err(cleanup_failure(
                    "published bundle identity changed and rollback could not be completed",
                ));
            }
            return Err(post_commit);
        }
        self.committed = true;
        Ok(())
    }

    pub(crate) fn cleanup(
        &mut self,
        output_root: &ArtifactOutputRoot,
    ) -> Result<(), CommandFailure> {
        if self.committed {
            return Ok(());
        }
        output_root
            .revalidate()
            .map_err(|diagnostic| failure(CommandFailureKind::Cleanup, diagnostic))?;
        self.revalidate_stage().map_err(|failure| CommandFailure {
            kind: CommandFailureKind::Cleanup,
            diagnostics: failure.diagnostics,
        })?;
        let Some(name) = self.path.file_name().and_then(|value| value.to_str()) else {
            return Err(cleanup_failure("private stage name could not be validated"));
        };
        if self.path.parent() != Some(output_root.path()) || !name.starts_with(TRANSACTION_PREFIX) {
            return Err(cleanup_failure("private stage escaped the validated output root"));
        }
        self.audit_inventory(false).map_err(|failure| CommandFailure {
            kind: CommandFailureKind::Cleanup,
            diagnostics: failure.diagnostics,
        })?;
        match fs::remove_dir_all(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(cleanup_failure("private stage could not be fully removed")),
        }
    }

    fn revalidate_stage(&self) -> Result<(), CommandFailure> {
        let metadata = fs::symlink_metadata(&self.path)
            .map_err(|_| transaction_error("private stage identity could not be inspected"))?;
        if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
            return Err(transaction_error("private stage is not a real directory"));
        }
        let current = Handle::from_path(&self.path)
            .map_err(|_| transaction_error("private stage identity could not be opened"))?;
        if current != self.identity {
            return Err(transaction_error("private stage identity changed during the operation"));
        }
        let after = fs::symlink_metadata(&self.path)
            .map_err(|_| transaction_error("private stage identity could not be revalidated"))?;
        if !after.is_dir() || metadata_is_link_or_reparse(&after) {
            return Err(transaction_error("private stage changed during revalidation"));
        }
        Ok(())
    }

    fn audit_inventory(&self, complete: bool) -> Result<(), CommandFailure> {
        let artifacts = self.artifacts.borrow();
        let harnesses = self.harnesses.borrow();
        let manifest = self.manifest.borrow();
        let mut expected_root =
            artifacts.keys().map(|target| target.as_str().to_owned()).collect::<BTreeSet<_>>();
        expected_root.extend(harnesses.iter().cloned());
        expected_root.extend(manifest.iter().map(|file| file.filename.clone()));
        let actual_root = read_directory_names(&self.path)?;
        if (complete && actual_root != expected_root)
            || (!complete && !actual_root.is_subset(&expected_root))
        {
            return Err(transaction_error("private stage inventory changed unexpectedly"));
        }
        for (target, files) in artifacts.iter() {
            let directory = self.path.join(target.as_str());
            if !directory.exists() && !complete {
                continue;
            }
            let metadata = fs::symlink_metadata(&directory)
                .map_err(|_| transaction_error("staged target directory is unavailable"))?;
            if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
                return Err(transaction_error("staged target path is not a real directory"));
            }
            let expected = files.keys().cloned().collect::<BTreeSet<_>>();
            let actual = read_directory_names(&directory)?;
            if (complete && actual != expected) || (!complete && !actual.is_subset(&expected)) {
                return Err(transaction_error("staged target inventory changed unexpectedly"));
            }
            for artifact in files.values() {
                let path = directory.join(&artifact.filename);
                if !path.exists() && !complete {
                    continue;
                }
                let metadata = fs::symlink_metadata(&path)
                    .map_err(|_| transaction_error("staged artifact is unavailable"))?;
                if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
                    return Err(transaction_error("staged artifact is not a real regular file"));
                }
                if complete {
                    let bytes = fs::read(&path)
                        .map_err(|_| transaction_error("staged artifact could not be audited"))?;
                    if u64::try_from(bytes.len()).ok() != Some(artifact.bytes)
                        || sha256(&bytes) != artifact.sha256
                    {
                        return Err(transaction_error(
                            "staged artifact bytes changed before commit",
                        ));
                    }
                    if artifact.executable && !is_executable_file(&metadata) {
                        return Err(transaction_error("staged native executable mode changed"));
                    }
                }
            }
        }
        for name in harnesses.iter() {
            let metadata = fs::symlink_metadata(self.path.join(name))
                .map_err(|_| transaction_error("staged private file is unavailable"))?;
            if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
                return Err(transaction_error("staged private path is not a real regular file"));
            }
        }
        if let Some(manifest) = manifest.as_ref() {
            let path = self.path.join(&manifest.filename);
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| transaction_error("staged manifest is unavailable"))?;
            if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
                return Err(transaction_error("staged manifest is not a real regular file"));
            }
            if complete {
                let bytes = fs::read(&path)
                    .map_err(|_| transaction_error("staged manifest could not be audited"))?;
                if u64::try_from(bytes.len()).ok() != Some(manifest.bytes)
                    || sha256(&bytes) != manifest.sha256
                {
                    return Err(transaction_error("staged manifest bytes changed before commit"));
                }
            }
        }
        Ok(())
    }
}

fn read_directory_names(path: &Path) -> Result<BTreeSet<String>, CommandFailure> {
    fs::read_dir(path)
        .map_err(|_| transaction_error("private stage directory could not be enumerated"))?
        .map(|entry| {
            entry
                .map_err(|_| transaction_error("private stage entry could not be inspected"))?
                .file_name()
                .into_string()
                .map_err(|_| transaction_error("private stage entry name is not portable UTF-8"))
        })
        .collect()
}

#[cfg(unix)]
fn is_executable_file(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 == 0o111
}

#[cfg(not(unix))]
fn is_executable_file(_metadata: &fs::Metadata) -> bool {
    true
}

fn create_owned_directory(path: &Path) -> Result<(), CommandFailure> {
    create_private_directory(path)
        .map_err(|_| transaction_error("could not create secure target stage directory"))
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

fn write_complete_file(path: &Path, bytes: &[u8]) -> Result<(), CommandFailure> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| transaction_error("could not create staged file"))?;
    file.write_all(bytes)
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all())
        .map_err(|_| transaction_error("could not write and synchronize staged file"))
}

#[cfg(unix)]
fn prepare_executable_file(path: &Path) -> Result<(), CommandFailure> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .and_then(|()| fs::File::open(path)?.sync_all())
        .map_err(|_| transaction_error("could not apply and synchronize executable mode"))
}

#[cfg(not(unix))]
fn prepare_executable_file(path: &Path) -> Result<(), CommandFailure> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| transaction_error("could not revalidate executable artifact"))?;
    if metadata.is_file() && !metadata_is_link_or_reparse(&metadata) {
        Ok(())
    } else {
        Err(transaction_error("executable artifact is not a real regular file"))
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), CommandFailure> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| transaction_error("could not synchronize transaction directory"))
}

#[cfg(not(unix))]
fn sync_directory(path: &Path) -> Result<(), CommandFailure> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| transaction_error("could not revalidate transaction directory"))?;
    if metadata.is_dir() && !metadata_is_link_or_reparse(&metadata) {
        Ok(())
    } else {
        Err(transaction_error("transaction path is not a real directory"))
    }
}

#[cfg(target_os = "linux")]
fn rename_create_only(root: &Dir, source: &str, destination: &str) -> Result<(), CommandFailure> {
    use nix::fcntl::{RenameFlags, renameat2};
    use std::os::fd::AsFd;

    renameat2(root.as_fd(), source, root.as_fd(), destination, RenameFlags::RENAME_NOREPLACE)
        .map_err(|_| transaction_error("create-only bundle commit failed"))
}

#[cfg(windows)]
fn rename_create_only(root: &Dir, source: &str, destination: &str) -> Result<(), CommandFailure> {
    root.rename(source, root, destination)
        .map_err(|_| transaction_error("create-only bundle commit failed"))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn rename_create_only(
    _root: &Dir,
    _source: &str,
    _destination: &str,
) -> Result<(), CommandFailure> {
    Err(transaction_error("create-only bundle commit is unsupported on this platform"))
}

fn ensure_absent(path: &Path) -> Result<(), CommandFailure> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(transaction_error("create-only output bundle already exists")),
        Err(_) => Err(transaction_error("output bundle destination could not be inspected")),
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn manifest_result(result: &TargetResult) -> ManifestResult {
    let outcome = match result.outcome {
        ScalarOutcome::Returned { value: ScalarValue::Bool(value) } => {
            ManifestOutcome::Returned { ty: "bool", value: ManifestScalar::Bool(value) }
        }
        ScalarOutcome::Returned { value: ScalarValue::I32(value) } => {
            ManifestOutcome::Returned { ty: "i32", value: ManifestScalar::I32(value) }
        }
        ScalarOutcome::Trapped { code } => ManifestOutcome::Trapped { code: code.as_str() },
        ScalarOutcome::HostError { code } => ManifestOutcome::HostError {
            code: match code {
                ScalarHostErrorCode::UnknownExport => "unknown-export",
                ScalarHostErrorCode::InvalidInvocation => "invalid-invocation",
                ScalarHostErrorCode::InvalidTargetResult => "invalid-target-result",
                ScalarHostErrorCode::TargetUnavailable => "target-unavailable",
            },
        },
    };
    ManifestResult { target: result.target, outcome }
}

fn source_failure(error: &SourceToIrError) -> CommandFailure {
    let diagnostics = match error {
        SourceToIrError::Frontend(frontend) if frontend.diagnostics().is_empty() => {
            vec![Diagnostic::error(
                frontend.code(),
                None,
                frontend.to_string(),
                "verify the pinned frontend runtime and adapter, then retry",
            )]
        }
        SourceToIrError::Frontend(frontend) => frontend.diagnostics().to_vec(),
        SourceToIrError::Rejected(diagnostics) => diagnostics.clone(),
    };
    CommandFailure { kind: CommandFailureKind::Source, diagnostics }
}

fn preparation_failure(diagnostic: Diagnostic) -> CommandFailure {
    failure(CommandFailureKind::Preparation, diagnostic)
}

fn execution_failure(diagnostic: Diagnostic) -> CommandFailure {
    failure(CommandFailureKind::Execution, diagnostic)
}

pub(super) fn runtime_failure(diagnostic: Diagnostic) -> CommandFailure {
    if diagnostic.code() == "ZRYNA-R3007" {
        failure(CommandFailureKind::Cleanup, diagnostic)
    } else {
        execution_failure(diagnostic)
    }
}

fn native_runtime_failure(diagnostic: Diagnostic) -> CommandFailure {
    if matches!(diagnostic.code(), "ZRYNA-N4009" | "ZRYNA-N4016") {
        failure(CommandFailureKind::Cleanup, diagnostic)
    } else {
        execution_failure(diagnostic)
    }
}

fn native_preparation_failure(diagnostics: Vec<Diagnostic>) -> CommandFailure {
    let kind = if diagnostics.iter().any(|diagnostic| diagnostic.code() == "ZRYNA-N4016") {
        CommandFailureKind::Cleanup
    } else {
        CommandFailureKind::Preparation
    };
    CommandFailure { kind, diagnostics }
}

pub(crate) fn invariant_failure(_error: serde_json::Error) -> CommandFailure {
    request_error(
        "ZRYNA-C1011",
        "runtime harness serialization failed",
        "report this compiler invariant failure",
    )
}

fn transaction_error(message: &'static str) -> CommandFailure {
    request_error(
        "ZRYNA-C1009",
        message,
        "use a writable real output filesystem and a fresh artifact stem",
    )
    .with_kind(CommandFailureKind::Preparation)
}

fn cleanup_failure(message: &'static str) -> CommandFailure {
    request_error(
        "ZRYNA-C1012",
        message,
        "inspect and remove only the exact reported transaction directory before retrying",
    )
    .with_kind(CommandFailureKind::Cleanup)
}

fn entrypoint_error(message: &'static str) -> CommandFailure {
    request_error(
        "ZRYNA-C1003",
        message,
        "use one existing portable workspace-relative .zry file without links",
    )
}

pub(super) fn request_error(
    code: &'static str,
    message: &'static str,
    guidance: &'static str,
) -> CommandFailure {
    failure(CommandFailureKind::Request, Diagnostic::error(code, None, message, guidance))
}

fn failure(kind: CommandFailureKind, diagnostic: Diagnostic) -> CommandFailure {
    CommandFailure { kind, diagnostics: vec![diagnostic] }
}

trait FailureKindExt {
    fn with_kind(self, kind: CommandFailureKind) -> Self;
}

impl FailureKindExt for CommandFailure {
    fn with_kind(mut self, kind: CommandFailureKind) -> Self {
        self.kind = kind;
        self
    }
}

#[cfg(windows)]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        io::Write as _,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use zryna_abi::{
        Invocation, RawHostScalar, ScalarOutcome, ScalarTarget, ScalarType, ScalarValue,
    };
    use zryna_diagnostics::Diagnostic;
    use zryna_frontend::{
        VerifiedFrontendProvider, VerifiedFrontendProviderV3, WorkerError, WorkerFrontendV3,
        syntax_v2, syntax_v3,
    };
    use zryna_ir::{Type, control_flow_v1};
    use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

    use super::preparation::compile_selected;
    #[cfg(unix)]
    use super::read_entrypoint_with_after_read;
    use super::{
        BoundedManifestWriter, BuildRequest, CommandFailure, CommandFailureKind, CommandKind,
        ControlFlowBuildRequest, MANIFEST_NAME, ManifestTarget, TargetSelection, Transaction,
        configured_frontend_v3, execute_control_flow_with_frontend_factory,
        native_preparation_failure, native_runtime_failure, normalize_carrier,
        render_javascript_harness, render_webassembly_harness, runtime_failure, validate_request,
    };
    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    use super::{
        ControlFlowPhase, RunInvocation, execute_control_flow_with_frontend_factory_and_checkpoint,
        failure,
    };
    use crate::{ArtifactOutputRoot, runtime::NodeRuntimeCapability};

    static NEXT_ROOT: AtomicUsize = AtomicUsize::new(0);

    struct CountingFrontendV3 {
        inner: WorkerFrontendV3,
        calls: Arc<AtomicUsize>,
    }

    impl VerifiedFrontendProviderV3 for CountingFrontendV3 {
        fn minimum_analysis_timeout(&self) -> Duration {
            self.inner.minimum_analysis_timeout()
        }

        fn analyze_verified_v3(
            &self,
            sources: &SourceMap,
        ) -> Result<syntax_v3::ProjectSyntaxSnapshot, WorkerError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.inner.analyze_verified_v3(sources)
        }

        fn analyze_verified_v3_with_timeout(
            &self,
            sources: &SourceMap,
            timeout: Duration,
        ) -> Result<syntax_v3::ProjectSyntaxSnapshot, WorkerError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.inner.analyze_verified_v3_with_timeout(sources, timeout)
        }
    }

    #[test]
    fn bounded_manifest_writer_accepts_exact_limit_and_rejects_plus_one() {
        let mut exact = BoundedManifestWriter::new(8);
        exact.write_all(b"12345678").expect("the exact manifest budget must be accepted");
        let mut plus_one = BoundedManifestWriter::new(8);
        assert!(plus_one.write_all(b"123456789").is_err());
    }

    #[test]
    fn public_m2_pipeline_discovers_each_batch_once_then_authenticates_one_final_graph() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repository root must resolve");
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let stem = format!("m2_counting_frontend_{}_{sequence}", std::process::id());
        let bundle = root.join(".zryna/out").join(format!("{stem}.build"));
        let request = ControlFlowBuildRequest {
            workspace_root: root,
            entrypoint: "examples/control-flow/main.zry".to_owned(),
            artifact_stem: stem,
            targets: TargetSelection::JavaScript,
            node_runtime: node_executable(),
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let result = execute_control_flow_with_frontend_factory(&request, None, |request, node| {
            configured_frontend_v3(request, node)
                .map(|inner| CountingFrontendV3 { inner, calls: observed })
        });
        result.expect("counted public M2 build must succeed");
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        fs::remove_dir_all(bundle).expect("test-owned counted bundle must be removable");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    #[test]
    fn every_m2_pipeline_phase_failure_leaves_no_final_bundle() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repository root must resolve");
        let phases = [
            ControlFlowPhase::Discovery,
            ControlFlowPhase::Semantics,
            ControlFlowPhase::JavaScriptBackend,
            ControlFlowPhase::WebAssemblyBackend,
            ControlFlowPhase::NativeMir,
            ControlFlowPhase::NativeObject,
            ControlFlowPhase::NativeLink,
            ControlFlowPhase::JavaScriptExecution,
            ControlFlowPhase::WebAssemblyExecution,
            ControlFlowPhase::NativeExecution,
            ControlFlowPhase::ArtifactStaging,
            ControlFlowPhase::ManifestRendering,
            ControlFlowPhase::Publication,
        ];
        for (index, injected) in phases.into_iter().enumerate() {
            let stem = format!("m2_phase_failure_{}_{}", std::process::id(), index);
            let bundle = root.join(".zryna/out").join(format!("{stem}.run"));
            let request = ControlFlowBuildRequest {
                workspace_root: root.clone(),
                entrypoint: "examples/control-flow/main.zry".to_owned(),
                artifact_stem: stem,
                targets: TargetSelection::All,
                node_runtime: node_executable(),
            };
            let invocation = RunInvocation {
                logical_export: "choose".to_owned(),
                arguments: vec![ScalarValue::Bool(true), ScalarValue::I32(21)],
            };
            let result = execute_control_flow_with_frontend_factory_and_checkpoint(
                &request,
                Some(&invocation),
                configured_frontend_v3,
                &|phase| {
                    if phase == injected {
                        Err(failure(
                            CommandFailureKind::Preparation,
                            Diagnostic::error(
                                "ZRYNA-C1099",
                                None,
                                "test-injected M2 pipeline phase failure",
                                "verify atomic failure handling",
                            ),
                        ))
                    } else {
                        Ok(())
                    }
                },
            );
            let error = result.expect_err("the selected phase must fail");
            assert_eq!(error.diagnostics()[0].code(), "ZRYNA-C1099");
            assert!(!bundle.exists(), "{injected:?} must not advertise a final bundle");
        }
    }

    fn node_executable() -> PathBuf {
        let executable = if cfg!(windows) { "node.exe" } else { "node" };
        ["ZRYNA_TEST_NODE", "NODE"]
            .into_iter()
            .filter_map(env::var_os)
            .map(PathBuf::from)
            .chain(
                env::var_os("PATH")
                    .into_iter()
                    .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
                    .map(move |directory| directory.join(executable)),
            )
            .find(|path| path.is_file())
            .expect("Node.js must be installed")
            .canonicalize()
            .expect("Node.js executable must canonicalize")
    }

    struct TemporaryRoot {
        path: PathBuf,
    }

    impl TemporaryRoot {
        fn new(label: &str) -> Self {
            let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir()
                .join(format!("zryna-pipeline-{}-{label}-{sequence}", std::process::id()));
            fs::create_dir(&path).expect("unique test root must be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn output(&self) -> ArtifactOutputRoot {
            ArtifactOutputRoot::prepare_for_workspace(&self.path)
                .expect("test output capability must be prepared")
        }

        fn request(&self, entrypoint: impl Into<String>) -> BuildRequest {
            BuildRequest {
                workspace_root: self.path.clone(),
                entrypoint: entrypoint.into(),
                artifact_stem: "artifact".to_owned(),
                targets: TargetSelection::JavaScript,
                node_runtime: self.path.join("node"),
            }
        }
    }

    impl Drop for TemporaryRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn sealed_runtime_executes_public_m2_javascript_and_webassembly_exports() {
        use control_flow_v1::raw::{
            Block, BlockId, Function, FunctionId, Instruction, InstructionKind, Module, ModuleId,
            Program, SpannedTerminator, Terminator, ValueDefinition, ValueId,
        };

        let root = TemporaryRoot::new("m2-sealed-javascript");
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "main.zry".to_owned(),
            text: "x".to_owned(),
        }])
        .expect("M2 runtime source map");
        let path = NormalizedSourcePath::new("main.zry").expect("M2 runtime path");
        let file = sources.file_id(&path).expect("M2 runtime file");
        let span = sources.span(file, 0, 1).expect("M2 runtime span");
        let value = |id, ty| ValueDefinition { id: ValueId(id), ty, span };
        let ret = |id| vec![SpannedTerminator { span, kind: Terminator::Return(ValueId(id)) }];
        let multiply = Function {
            id: FunctionId { module: ModuleId(0), declaration: 0 },
            entry_export: Some("Math".to_owned()),
            span,
            parameters: vec![value(0, Type::I32), value(1, Type::I32)],
            result: Type::I32,
            blocks: vec![Block {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![Instruction {
                    result: value(2, Type::I32),
                    kind: InstructionKind::I32Mul { lhs: ValueId(0), rhs: ValueId(1) },
                }],
                terminators: ret(2),
            }],
        };
        let boolean = Function {
            id: FunctionId { module: ModuleId(0), declaration: 1 },
            entry_export: Some("Number".to_owned()),
            span,
            parameters: vec![value(0, Type::Bool)],
            result: Type::Bool,
            blocks: vec![Block {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminators: ret(0),
            }],
        };
        let program = control_flow_v1::verify(
            Program {
                entry_module: ModuleId(0),
                modules: vec![Module {
                    id: ModuleId(0),
                    source_file: file,
                    functions: vec![multiply, boolean],
                }],
            },
            &sources,
            file,
        )
        .expect("M2 runtime fixture must verify");
        let artifact = zryna_backend_javascript::emit_control_flow(&program)
            .expect("M2 runtime artifact must emit");
        let webassembly = zryna_backend_webassembly::emit_control_flow(&program)
            .expect("M2 WebAssembly artifact must emit");
        let javascript = root.path().join("javascript");
        fs::create_dir(&javascript).expect("M2 runtime artifact directory");
        fs::write(javascript.join("m2.mjs"), artifact.source).expect("M2 runtime artifact write");
        let runtime = NodeRuntimeCapability::discover(&node_executable(), root.path())
            .expect("pinned Node capability");

        for (invocation, expected_type, expected_javascript, expected_webassembly) in [
            (
                Invocation::new(
                    "Math".to_owned(),
                    vec![ScalarValue::I32(65_537), ScalarValue::I32(65_537)],
                ),
                ScalarType::I32,
                RawHostScalar::JavaScriptNumber(131_073.0),
                RawHostScalar::I32(131_073),
            ),
            (
                Invocation::new("Number".to_owned(), vec![ScalarValue::Bool(true)]),
                ScalarType::Bool,
                RawHostScalar::JavaScriptBool(true),
                RawHostScalar::I32(1),
            ),
        ] {
            let prepared = program.prepare_invocation(invocation).expect("typed M2 invocation");
            let harness = render_javascript_harness("m2", &prepared).expect("typed M2 harness");
            let label = match expected_type {
                ScalarType::Bool => "bool",
                ScalarType::I32 => "i32",
            };
            let harness_path = root.path().join(format!("harness-{label}.mjs"));
            fs::write(&harness_path, harness).expect("typed M2 harness write");
            assert_eq!(
                runtime
                    .run_javascript_module(&harness_path, root.path(), expected_type)
                    .expect("sealed typed M2 execution"),
                expected_javascript
            );
            let webassembly_harness =
                render_webassembly_harness("m2", &prepared).expect("typed M2 Wasm harness");
            let frame = runtime
                .run_webassembly_module(&webassembly_harness, webassembly.bytes(), root.path())
                .expect("sealed typed M2 WebAssembly execution");
            let raw = RawHostScalar::I32(i32::from_le_bytes(frame));
            assert_eq!(raw, expected_webassembly);
            assert!(matches!(
                normalize_carrier(ScalarTarget::CoreWebAssembly, expected_type, raw),
                ScalarOutcome::Returned { .. }
            ));

            if expected_type == ScalarType::Bool {
                let invalid = String::from_utf8(webassembly_harness)
                    .expect("UTF-8 harness")
                    .replace("invoke(1)", "invoke(2)");
                assert_eq!(
                    runtime
                        .run_webassembly_module(
                            invalid.as_bytes(),
                            webassembly.bytes(),
                            root.path(),
                        )
                        .expect_err("noncanonical public bool lane must trap")
                        .code(),
                    "ZRYNA-R3006"
                );
            }
        }
    }

    fn rejected_request(result: Result<super::ValidatedRequest, CommandFailure>) -> CommandFailure {
        match result {
            Ok(_) => panic!("request must be rejected"),
            Err(failure) => failure,
        }
    }

    struct CountingFrontend {
        calls: AtomicUsize,
    }

    impl VerifiedFrontendProvider for CountingFrontend {
        fn analyze_verified(
            &self,
            sources: &SourceMap,
        ) -> Result<syntax_v2::ProjectSyntaxSnapshot, WorkerError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let raw = syntax_v2::decode_snapshot(include_bytes!(
                "../../../tests/fixtures/typescript-adapter-v2-result.json"
            ))
            .expect("checked protocol fixture must decode");
            Ok(syntax_v2::verify_snapshot(raw, sources)
                .expect("checked protocol fixture must bind to the authoritative source map"))
        }
    }

    #[test]
    fn all_analyzes_once_and_emits_every_target_from_one_program() {
        let source = include_str!("../../../examples/universal/add.zry");
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "examples/universal/add.zry".to_owned(),
            text: source.to_owned(),
        }])
        .expect("fixture source map");
        let frontend = CountingFrontend { calls: AtomicUsize::new(0) };

        let (compiled, artifacts) = compile_selected(&frontend, &sources, TargetSelection::All)
            .expect("all targets must prepare from one supported source");

        assert_eq!(frontend.calls.load(Ordering::SeqCst), 1);
        assert_eq!(compiled.program().functions().count(), 1);
        assert!(artifacts.javascript.is_some());
        assert!(artifacts.webassembly.is_some());
        assert!(artifacts.native_object.is_some());
        assert_eq!(
            TargetSelection::All.ordered(),
            vec![ManifestTarget::JavaScript, ManifestTarget::WebAssembly, ManifestTarget::Native]
        );
    }

    #[test]
    fn request_rejects_traversal_and_absolute_entrypoints() {
        let root = TemporaryRoot::new("request-paths");
        let source_directory = root.path().join("source");
        fs::create_dir(&source_directory).expect("source directory must be created");
        let source = source_directory.join("main.zry");
        fs::write(&source, "export function value(): i32 { return 1; }\n")
            .expect("source must be written");

        for entrypoint in [
            "../main.zry".to_owned(),
            "source//main.zry".to_owned(),
            "source/./main.zry".to_owned(),
            "source/main.zry/".to_owned(),
            source.to_string_lossy().into_owned(),
        ] {
            let failure =
                rejected_request(validate_request(&root.request(entrypoint), CommandKind::Build));
            assert_eq!(failure.kind(), CommandFailureKind::Request);
            assert_eq!(failure.diagnostics()[0].code(), "ZRYNA-C1003");
        }
    }

    #[cfg(unix)]
    #[test]
    fn request_rejects_linked_source_components() {
        use std::os::unix::fs::symlink;

        let root = TemporaryRoot::new("request-links");
        let source_directory = root.path().join("source");
        fs::create_dir(&source_directory).expect("source directory must be created");
        fs::write(
            source_directory.join("main.zry"),
            "export function value(): i32 { return 1; }\n",
        )
        .expect("source must be written");
        symlink("main.zry", source_directory.join("linked.zry"))
            .expect("source link must be created");
        symlink(&source_directory, root.path().join("linked-source"))
            .expect("directory link must be created");

        for entrypoint in ["source/linked.zry", "linked-source/main.zry"] {
            let failure =
                rejected_request(validate_request(&root.request(entrypoint), CommandKind::Build));
            assert_eq!(failure.kind(), CommandFailureKind::Request);
            assert!(matches!(failure.diagnostics()[0].code(), "ZRYNA-C1002" | "ZRYNA-C1003"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn entrypoint_read_rejects_same_length_final_replacement() {
        let root = TemporaryRoot::new("source-replacement");
        let source = root.path().join("main.zry");
        let replacement = root.path().join("replacement.zry");
        fs::write(&source, b"original-source\n").expect("source must be written");
        fs::write(&replacement, b"replaced-source\n").expect("replacement must be written");
        assert_eq!(
            fs::metadata(&source).expect("source metadata").len(),
            fs::metadata(&replacement).expect("replacement metadata").len()
        );

        let failure = read_entrypoint_with_after_read(&source, || {
            fs::rename(&replacement, &source).expect("replacement must be installed");
        })
        .expect_err("replaced source identity must be rejected");

        assert_eq!(failure.kind(), CommandFailureKind::Request);
        assert_eq!(failure.diagnostics()[0].code(), "ZRYNA-C1003");
    }

    #[cfg(unix)]
    #[test]
    fn output_root_replacement_fails_closed_and_stage_remains_cleanable() {
        use std::os::unix::fs::symlink;

        let root = TemporaryRoot::new("output-replacement");
        let output = root.output();
        let mut transaction = Transaction::create(&output).expect("transaction must be created");
        let stage = transaction.path().to_path_buf();
        transaction
            .write_artifact(
                ManifestTarget::JavaScript,
                "ecmascript-module",
                "artifact",
                "mjs",
                b"export {};\n",
            )
            .expect("staged artifact must be written");
        let displaced = root.path().join("displaced-output");
        let replacement = root.path().join("replacement-output");
        fs::create_dir(&replacement).expect("replacement directory must be created");
        fs::rename(output.path(), &displaced).expect("output root must be displaced");
        symlink(&replacement, output.path()).expect("linked output replacement must be installed");
        let final_bundle = output.path().join("artifact.build");

        let failure = transaction
            .commit(&output, &final_bundle)
            .expect_err("linked output replacement must fail closed");
        assert_eq!(failure.kind(), CommandFailureKind::Preparation);
        assert_eq!(failure.diagnostics()[0].code(), "ZRYNA-D2002");
        assert!(!replacement.join("artifact.build").exists());

        fs::remove_file(output.path()).expect("replacement link must be removed");
        fs::rename(&displaced, output.path()).expect("validated output root must be restored");
        transaction.cleanup(&output).expect("restored stage must be cleanable");
        assert!(!stage.exists());
        assert!(!final_bundle.exists());
    }

    #[cfg(unix)]
    #[test]
    fn transaction_stage_replacement_cannot_redirect_writes() {
        use std::os::unix::fs::symlink;

        let root = TemporaryRoot::new("stage-replacement");
        let output = root.output();
        let mut transaction = Transaction::create(&output).expect("transaction must be created");
        let stage = transaction.path().to_path_buf();
        let displaced = output.path().join("displaced-stage");
        let attacker = root.path().join("attacker-output");
        fs::create_dir(&attacker).expect("attacker directory must be created");
        fs::rename(&stage, &displaced).expect("private stage must be displaced");
        symlink(&attacker, &stage).expect("replacement stage link must be installed");

        let failure = transaction
            .write_artifact(
                ManifestTarget::JavaScript,
                "ecmascript-module",
                "artifact",
                "mjs",
                b"export {};\n",
            )
            .expect_err("replaced private stage must fail closed");
        assert_eq!(failure.kind(), CommandFailureKind::Preparation);
        assert!(fs::read_dir(&attacker).expect("attacker directory listing").next().is_none());

        fs::remove_file(&stage).expect("replacement link must be removed");
        fs::rename(&displaced, &stage).expect("private stage must be restored");
        transaction.cleanup(&output).expect("restored transaction must be removed");
        assert!(!stage.exists());
    }

    #[test]
    fn real_output_root_replacement_is_detected_or_prevented() {
        let root = TemporaryRoot::new("real-output-replacement");
        let output = root.output();
        let mut transaction = Transaction::create(&output).expect("transaction must be created");
        let stage = transaction.path().to_path_buf();
        transaction
            .write_artifact(
                ManifestTarget::JavaScript,
                "ecmascript-module",
                "artifact",
                "mjs",
                b"export {};\n",
            )
            .expect("staged artifact must be written");
        let displaced = root.path().join("displaced-real-output");
        if let Err(error) = fs::rename(output.path(), &displaced) {
            assert!(
                cfg!(windows)
                    && (error.kind() == std::io::ErrorKind::PermissionDenied
                        || matches!(error.raw_os_error(), Some(5 | 32))),
                "unexpected output replacement failure: {error}"
            );
            let final_bundle = output.path().join("artifact.build");
            assert!(!final_bundle.exists());
            transaction.cleanup(&output).expect("protected stage must be cleanable");
            assert!(!stage.exists());
            return;
        }
        fs::create_dir(output.path()).expect("real replacement output must be installed");
        let final_bundle = output.path().join("artifact.build");

        let failure = transaction
            .commit(&output, &final_bundle)
            .expect_err("displaced transaction must not commit through a replacement root");
        assert_eq!(failure.kind(), CommandFailureKind::Preparation);
        assert!(!final_bundle.exists());

        fs::remove_dir(output.path()).expect("empty replacement output must be removed");
        fs::rename(&displaced, output.path()).expect("original output root must be restored");
        transaction.cleanup(&output).expect("restored stage must be cleanable");
        assert!(!stage.exists());
        assert!(!final_bundle.exists());
    }

    #[test]
    fn post_transaction_failure_cleans_stage_without_advertising_bundle() {
        let root = TemporaryRoot::new("transaction-failure");
        let output = root.output();
        let mut transaction = Transaction::create(&output).expect("transaction must be created");
        let stage = transaction.path().to_path_buf();
        let final_bundle = output.path().join("artifact.build");
        transaction
            .write_artifact(
                ManifestTarget::JavaScript,
                "ecmascript-module",
                "artifact",
                "mjs",
                b"export {};\n",
            )
            .expect("staged artifact must be written");
        fs::write(stage.join(MANIFEST_NAME), b"occupied")
            .expect("manifest collision must be installed");

        let failure = transaction
            .write_manifest(MANIFEST_NAME, b"{}\n")
            .expect_err("post-stage manifest failure must be reported");
        assert_eq!(failure.kind(), CommandFailureKind::Preparation);
        let cleanup = transaction
            .cleanup(&output)
            .expect_err("cleanup must refuse the unexpected manifest entry");
        assert_eq!(cleanup.kind(), CommandFailureKind::Cleanup);
        assert!(stage.exists());
        assert!(!final_bundle.exists());
        assert_eq!(
            fs::read(stage.join(MANIFEST_NAME)).expect("unexpected bytes must remain"),
            b"occupied"
        );
    }

    #[test]
    fn staged_artifact_tamper_fails_before_manifest_and_publishes_nothing() {
        let root = TemporaryRoot::new("artifact-tamper");
        let output = root.output();
        let final_bundle = output.path().join("artifact.build");
        let mut transaction = Transaction::create(&output).expect("transaction must be created");
        let stage = transaction.path().to_path_buf();
        transaction
            .write_artifact(
                ManifestTarget::JavaScript,
                "ecmascript-module",
                "artifact",
                "mjs",
                b"export const value = 1;\n",
            )
            .expect("artifact must be staged");
        fs::write(stage.join("javascript/artifact.mjs"), b"changed")
            .expect("fixture must mutate staged bytes");

        let failure = transaction
            .write_manifest(MANIFEST_NAME, b"{}\n")
            .expect_err("tampered artifact must fail the complete inventory audit");
        assert_eq!(failure.kind(), CommandFailureKind::Preparation);
        assert!(!final_bundle.exists());
        transaction.cleanup(&output).expect("known tampered stage must remain removable");
        assert!(!stage.exists());
    }

    #[test]
    fn unexpected_staged_entry_blocks_commit_and_refuses_recursive_cleanup() {
        let root = TemporaryRoot::new("unexpected-stage-entry");
        let output = root.output();
        let final_bundle = output.path().join("artifact.build");
        let mut transaction = Transaction::create(&output).expect("transaction must be created");
        let stage = transaction.path().to_path_buf();
        fs::write(stage.join("attacker-controlled"), b"do-not-delete")
            .expect("fixture must install an unexpected entry");

        let failure = transaction
            .commit(&output, &final_bundle)
            .expect_err("unexpected stage inventory must block commit");
        assert_eq!(failure.kind(), CommandFailureKind::Preparation);
        assert!(!final_bundle.exists());
        let cleanup = transaction
            .cleanup(&output)
            .expect_err("cleanup must not recursively delete an unexpected entry");
        assert_eq!(cleanup.kind(), CommandFailureKind::Cleanup);
        assert_eq!(
            fs::read(stage.join("attacker-controlled")).expect("entry must remain"),
            b"do-not-delete"
        );
    }

    #[test]
    fn destination_installed_after_absence_check_is_never_replaced() {
        let root = TemporaryRoot::new("destination-race");
        let output = root.output();
        let final_bundle = output.path().join("artifact.build");
        let mut transaction = Transaction::create(&output).expect("transaction must be created");
        let stage = transaction.path().to_path_buf();

        let failure = transaction
            .commit_with_before_rename(&output, &final_bundle, || {
                fs::create_dir(&final_bundle)
                    .expect("racing destination directory must be created");
                fs::write(final_bundle.join("sentinel"), b"prior")
                    .expect("racing destination sentinel must be written");
            })
            .expect_err("create-only rename must reject a destination race");
        assert_eq!(failure.kind(), CommandFailureKind::Preparation);
        assert_eq!(
            fs::read(final_bundle.join("sentinel")).expect("racing destination must remain"),
            b"prior"
        );
        transaction.cleanup(&output).expect("known empty stage must be removable");
        assert!(!stage.exists());
    }

    #[test]
    fn staged_bytes_changed_immediately_before_rename_are_never_published() {
        for relative in ["javascript/artifact.mjs", MANIFEST_NAME] {
            let root = TemporaryRoot::new("before-rename-tamper");
            let output = root.output();
            let final_bundle = output.path().join("artifact.build");
            let mut transaction =
                Transaction::create(&output).expect("transaction must be created");
            let stage = transaction.path().to_path_buf();
            transaction
                .write_artifact(
                    ManifestTarget::JavaScript,
                    "ecmascript-module",
                    "artifact",
                    "mjs",
                    b"export const value = 1;\n",
                )
                .expect("artifact must be staged");
            transaction.write_manifest(MANIFEST_NAME, b"{}\n").expect("manifest must be staged");
            let changed = stage.join(relative);

            let failure = transaction
                .commit_with_before_rename(&output, &final_bundle, || {
                    fs::write(&changed, b"changed immediately before rename")
                        .expect("fixture must mutate a staged file");
                })
                .expect_err("the final pre-rename audit must reject changed bytes");
            assert_eq!(failure.kind(), CommandFailureKind::Preparation);
            assert!(!final_bundle.exists());
            transaction.cleanup(&output).expect("known staged inventory must remain removable");
            assert!(!stage.exists());
        }
    }

    #[test]
    fn stage_root_displaced_after_final_audit_is_detected_and_rolled_back() {
        let root = TemporaryRoot::new("post-audit-stage-displacement");
        let output = root.output();
        let final_bundle = output.path().join("artifact.build");
        let mut transaction = Transaction::create(&output).expect("transaction must be created");
        let stage = transaction.path().to_path_buf();
        let displaced = output.path().join(".test-displaced-stage");
        transaction
            .write_artifact(
                ManifestTarget::JavaScript,
                "ecmascript-module",
                "artifact",
                "mjs",
                b"export const value = 1;\n",
            )
            .expect("artifact must be staged");
        transaction.write_manifest(MANIFEST_NAME, b"{}\n").expect("manifest must be staged");

        let failure = transaction
            .commit_with_rename_hooks(
                &output,
                &final_bundle,
                || {},
                || {
                    fs::rename(&stage, &displaced)
                        .expect("fixture must displace the audited stage");
                    fs::create_dir(&stage).expect("fixture must install a replacement stage");
                },
            )
            .expect_err("a different stage identity must never be committed");
        assert_eq!(failure.kind(), CommandFailureKind::Preparation);
        assert!(!final_bundle.exists());

        fs::remove_dir(&stage).expect("test-owned replacement stage must be removable");
        fs::rename(&displaced, &stage).expect("audited stage must be restored for bounded cleanup");
        transaction.cleanup(&output).expect("restored audited stage must be cleanable");
        assert!(!stage.exists());
    }

    #[cfg(unix)]
    #[test]
    fn destination_link_collision_preserves_prior_bytes() {
        use std::os::unix::fs::symlink;

        let root = TemporaryRoot::new("destination-link");
        let output = root.output();
        let sentinel = root.path().join("sentinel");
        fs::write(&sentinel, b"prior-bytes").expect("sentinel must be written");
        let final_bundle = output.path().join("artifact.build");
        symlink(&sentinel, &final_bundle).expect("destination link must be created");
        let mut transaction = Transaction::create(&output).expect("transaction must be created");
        let stage = transaction.path().to_path_buf();

        let failure = transaction
            .commit(&output, &final_bundle)
            .expect_err("destination link must be treated as a collision");
        assert_eq!(failure.kind(), CommandFailureKind::Preparation);
        assert_eq!(fs::read(&sentinel).expect("sentinel must remain"), b"prior-bytes");
        assert!(
            fs::symlink_metadata(&final_bundle)
                .expect("destination link must remain")
                .file_type()
                .is_symlink()
        );
        transaction.cleanup(&output).expect("failed transaction must be removed");
        assert!(!stage.exists());
    }

    #[test]
    fn exit_categories_and_cleanup_diagnostics_are_stable() {
        for (kind, expected) in [
            (CommandFailureKind::Architecture, 1),
            (CommandFailureKind::Request, 2),
            (CommandFailureKind::Source, 3),
            (CommandFailureKind::Preparation, 4),
            (CommandFailureKind::Execution, 5),
            (CommandFailureKind::Cleanup, 6),
        ] {
            assert_eq!(kind.exit_code(), expected);
        }

        let runtime_cleanup = runtime_failure(Diagnostic::error(
            "ZRYNA-R3007",
            None,
            "runtime cleanup failed",
            "stop the process tree",
        ));
        assert_eq!(runtime_cleanup.kind(), CommandFailureKind::Cleanup);
        assert_eq!(runtime_cleanup.kind().exit_code(), 6);
        let native_cleanup = native_runtime_failure(Diagnostic::error(
            "ZRYNA-N4009",
            None,
            "native cleanup failed",
            "stop the process group",
        ));
        assert_eq!(native_cleanup.kind(), CommandFailureKind::Cleanup);
        assert_eq!(native_cleanup.kind().exit_code(), 6);
        let native_stage_cleanup = native_runtime_failure(Diagnostic::error(
            "ZRYNA-N4016",
            None,
            "native stage cleanup failed",
            "restore the private stage permissions",
        ));
        assert_eq!(native_stage_cleanup.kind(), CommandFailureKind::Cleanup);
        assert_eq!(native_stage_cleanup.kind().exit_code(), 6);
        let native_preparation_cleanup = native_preparation_failure(vec![Diagnostic::warning(
            "ZRYNA-N4016",
            None,
            "native preparation stage cleanup failed",
            "remove the private stage",
        )]);
        assert_eq!(native_preparation_cleanup.kind(), CommandFailureKind::Cleanup);
        assert_eq!(native_preparation_cleanup.kind().exit_code(), 6);
        let execution = runtime_failure(Diagnostic::error(
            "ZRYNA-R3005",
            None,
            "runtime execution failed",
            "retry",
        ));
        assert_eq!(execution.kind(), CommandFailureKind::Execution);
        assert_eq!(execution.kind().exit_code(), 5);
    }
}
