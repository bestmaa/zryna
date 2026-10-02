//! Source-checkout command execution retains source and private input through atomic publication.

use std::path::{Path, PathBuf};

use zryna_diagnostics::Diagnostic;
use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

use crate::{
    ArtifactOutputRoot, CommandFailure, CommandFailureKind, CommandH1ExecutionRecord,
    SourceToIrError, WorkspaceSourceRoot, check_workspace,
    command_h1_runtime::prepare_approved_request, pipeline::Transaction,
    runtime::NodeRuntimeCapability,
};

/// One explicit source-checkout invocation of the sole command `main` export.
#[derive(Clone)]
pub struct CommandH1RunRequest {
    /// Absolute real compiler workspace root.
    pub workspace_root: PathBuf,
    /// One portable workspace-relative source file; dependencies are rejected by admission.
    pub entrypoint: String,
    /// Portable create-only output stem.
    pub artifact_stem: String,
    /// Exact direct pinned Node executable used only by the syntax provider.
    pub node_runtime: PathBuf,
    /// Explicit caller-owned private request file; omission approves only a pure command.
    pub grant_file: Option<PathBuf>,
}

/// A complete command execution record committed with its exact component.
pub struct PublishedCommandH1Bundle {
    path: PathBuf,
    manifest_path: PathBuf,
    component_path: PathBuf,
    record: CommandH1ExecutionRecord,
    diagnostics: Vec<Diagnostic>,
}

impl PublishedCommandH1Bundle {
    /// Returns the create-only final bundle directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Returns the complete execution manifest path.
    #[must_use]
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }
    /// Returns the independently audited command component path.
    #[must_use]
    pub fn component_path(&self) -> &Path {
        &self.component_path
    }
    /// Returns the actual execution and teardown observations.
    #[must_use]
    pub const fn record(&self) -> &CommandH1ExecutionRecord {
        &self.record
    }
    /// Returns authenticated non-fatal syntax diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Authenticates one source, captures its explicit root-approved request, executes and commits.
/// A failed run is still a complete record with absent or declared error return, never success.
///
/// # Errors
/// Rejects request, source, grant, artifact, architecture or publication failures without a
/// partial final bundle. Existing output is never replaced. Installed package profiles are
/// independently versioned and do not enter this source-checkout route.
pub fn run_command_h1_workspace(
    request: &CommandH1RunRequest,
) -> Result<PublishedCommandH1Bundle, CommandFailure> {
    if !request.workspace_root.is_absolute() || !request.node_runtime.is_absolute() {
        return Err(failure(CommandFailureKind::Request, invalid_request()));
    }
    let entrypoint = NormalizedSourcePath::new(request.entrypoint.clone()).map_err(|error| {
        failure(CommandFailureKind::Request, Diagnostic::from_source_error(&error))
    })?;
    crate::javascript::validate_artifact_stem(&request.artifact_stem)
        .map_err(|error| failure(CommandFailureKind::Request, error))?;
    let report = check_workspace(&request.workspace_root);
    if !report.is_valid() {
        return Err(CommandFailure {
            kind: CommandFailureKind::Architecture,
            diagnostics: report.diagnostics,
        });
    }
    // Create the declared output directories before retaining the source directory snapshot.
    let output = ArtifactOutputRoot::prepare_for_workspace(&request.workspace_root)
        .map_err(|error| failure(CommandFailureKind::Preparation, error))?;
    let source_root = WorkspaceSourceRoot::capture(&request.workspace_root)
        .map_err(|error| failure(CommandFailureKind::Source, error))?;
    let mut session = source_root
        .begin_discovery()
        .map_err(|error| failure(CommandFailureKind::Source, error))?;
    let source = session
        .read_source(&entrypoint)
        .map_err(|error| failure(CommandFailureKind::Source, error))?;
    let sources = SourceMap::build(vec![SourceFileInput {
        path: request.entrypoint.clone(),
        text: source.text,
    }])
    .map_err(|error| failure(CommandFailureKind::Source, Diagnostic::from_source_error(&error)))?;
    let node = NodeRuntimeCapability::discover(&request.node_runtime, &request.workspace_root)
        .map_err(|error| failure(CommandFailureKind::Preparation, error))?;
    let frontend = crate::ownership_pipeline::configured_frontend_at(
        &request.workspace_root.join("adapters/typescript-6"),
        &node,
    )?;
    session
        .validate_provider_batch(&sources)
        .map_err(|error| failure(CommandFailureKind::Source, error))?;
    let prepared = prepare_approved_request(
        &frontend,
        &sources,
        &zryna_backend_webassembly::pinned_wit_sources(),
        request.grant_file.as_deref(),
    )
    .map_err(source_failure)?;
    session.revalidate_all().map_err(|error| failure(CommandFailureKind::Source, error))?;
    node.revalidate().map_err(|error| failure(CommandFailureKind::Preparation, error))?;
    let diagnostics = prepared.diagnostics().to_vec();
    let policy = prepared.approved_policy();
    let mut transaction = Transaction::create(&output)?;
    let bundle = output.path().join(format!("{}.wasi-command-run", request.artifact_stem));
    let operation: Result<PublishedCommandH1Bundle, CommandFailure> = (|| {
        transaction
            .write_command_h1_artifact(&request.artifact_stem, prepared.artifact().bytes())?;
        let executed = prepared.execute(&policy).map_err(|diagnostics| CommandFailure {
            kind: CommandFailureKind::Preparation,
            diagnostics,
        })?;
        session.revalidate_all().map_err(|error| failure(CommandFailureKind::Source, error))?;
        node.revalidate().map_err(|error| failure(CommandFailureKind::Preparation, error))?;
        let manifest = executed
            .manifest_bytes(&request.artifact_stem)
            .map_err(|error| failure(CommandFailureKind::Preparation, error))?;
        transaction.write_manifest(crate::COMMAND_H1_MANIFEST_NAME, &manifest)?;
        session.revalidate_all().map_err(|error| failure(CommandFailureKind::Source, error))?;
        transaction.commit(&output, &bundle)?;
        let result = PublishedCommandH1Bundle {
            manifest_path: bundle.join(crate::COMMAND_H1_MANIFEST_NAME),
            component_path: bundle
                .join("wasi-command")
                .join(format!("{}.wasm", request.artifact_stem)),
            path: bundle,
            record: executed.record().clone(),
            diagnostics,
        };
        // `executed` holds the original private file and captured value until after the commit.
        drop(executed);
        Ok(result)
    })();
    match operation {
        Ok(bundle) => Ok(bundle),
        Err(mut error) => {
            if let Err(cleanup) = transaction.cleanup(&output) {
                error.kind = CommandFailureKind::Cleanup;
                error.diagnostics.extend(cleanup.diagnostics);
            }
            Err(error)
        }
    }
}

fn failure(kind: CommandFailureKind, diagnostic: Diagnostic) -> CommandFailure {
    CommandFailure { kind, diagnostics: vec![diagnostic] }
}

fn invalid_request() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-C4103",
        None,
        "Command paths must be absolute.",
        "Use one real compiler workspace and its exact pinned Node executable.",
    )
}

fn source_failure(error: SourceToIrError) -> CommandFailure {
    let diagnostics = match error {
        SourceToIrError::Frontend(error) if error.diagnostics().is_empty() => {
            vec![Diagnostic::error(
                error.code(),
                None,
                error.to_string(),
                "Use the exact pinned protocol-v4 syntax provider.",
            )]
        }
        error => error.diagnostics().to_vec(),
    };
    CommandFailure { kind: CommandFailureKind::Source, diagnostics }
}
