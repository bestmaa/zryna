//! Installed H1 retains installation, source and private-input authority through publication.

use std::path::PathBuf;

use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

use super::{InstalledCompiler, InstalledExecution, admission_error};
use crate::{
    ArtifactOutputRoot, CommandFailure, CommandFailureKind, PublishedCommandH1Bundle,
    WorkspaceSourceRoot,
    command_h1_runtime::prepare_approved_request,
    command_h1_workspace::{publish_command_h1, source_failure},
};

/// One source file below a project directory, without dependency packages or runtime overrides.
#[derive(Clone, Debug)]
pub struct InstalledCommandH1Request {
    /// Absolute real project directory outside the entire installation.
    pub project_root: PathBuf,
    /// One normalized project-relative source file.
    pub entrypoint: String,
    /// Portable create-only output stem.
    pub artifact_stem: String,
    /// Absolute owner-private request file; omission authorizes only a pure command.
    pub grant_file: Option<PathBuf>,
}

impl InstalledCompiler {
    /// Runs the sole H1 `main` export with this installation's authenticated provider/runtime.
    /// Existing frozen package profiles do not grant command authority or dependency support.
    ///
    /// # Errors
    /// Rejects overlapping roots, changed installation/source/input proofs, invalid grants,
    /// invalid command source, and publication failures without replacing existing output.
    pub fn run_command_h1(
        &self,
        request: &InstalledCommandH1Request,
    ) -> Result<PublishedCommandH1Bundle, CommandFailure> {
        let fail = |diagnostic| CommandFailure {
            kind: CommandFailureKind::Preparation,
            diagnostics: vec![diagnostic],
        };
        self.revalidate().map_err(fail)?;
        if !request.project_root.is_absolute()
            || request.project_root.starts_with(&self.root)
            || self.root.starts_with(&request.project_root)
            || request.grant_file.as_ref().is_some_and(|path| !path.is_absolute())
        {
            return Err(fail(admission_error("command project must be outside the installation")));
        }
        let path = NormalizedSourcePath::new(request.entrypoint.clone())
            .map_err(|error| fail(zryna_diagnostics::Diagnostic::from_source_error(&error)))?;
        crate::javascript::validate_artifact_stem(&request.artifact_stem).map_err(fail)?;
        let root = WorkspaceSourceRoot::capture(&request.project_root).map_err(fail)?;
        let project = std::fs::canonicalize(&request.project_root)
            .map_err(|_| fail(admission_error("command project path cannot be resolved")))?;
        let installation = std::fs::canonicalize(&self.root)
            .map_err(|_| fail(admission_error("installation path cannot be resolved")))?;
        if project.starts_with(&installation) || installation.starts_with(&project) {
            return Err(fail(admission_error("command project must be outside the installation")));
        }
        let execution = InstalledExecution::prepare(self)?;
        // Create outputs before retaining the source directory's entry snapshot.
        let output =
            ArtifactOutputRoot::prepare_for_workspace(&request.project_root).map_err(fail)?;
        let mut session = root.begin_discovery().map_err(fail)?;
        let source = session.read_source(&path).map_err(fail)?;
        let sources = SourceMap::build(vec![SourceFileInput {
            path: request.entrypoint.clone(),
            text: source.text,
        }])
        .map_err(|error| fail(zryna_diagnostics::Diagnostic::from_source_error(&error)))?;
        let frontend = execution.frontend_v4()?;
        session.validate_provider_batch(&sources).map_err(fail)?;
        execution.revalidate()?;
        let prepared = prepare_approved_request(
            &frontend,
            &sources,
            &zryna_backend_webassembly::pinned_wit_sources(),
            request.grant_file.as_deref(),
        )
        .map_err(source_failure)?;
        publish_command_h1(&output, &request.artifact_stem, prepared, &mut || {
            session.revalidate_all().map_err(fail)?;
            execution.revalidate()
        })
    }
}
