//! Private build-only dispatch retains source capabilities through the shared transaction.

use super::{
    ArtifactOutputRoot, BuildRequest, CommandFailure, CommandFailureKind, CommandKind,
    CommandSuccess, ControlFlowBuildRequest, control_flow, ensure_absent, failure,
    module_closure_failure, request_error, scalar, validate_architecture, validate_request,
};
use crate::native_frontend::NativeBuildRequest;
use crate::{NativeSourceSnapshot, WorkspaceSourceRoot, capture_native_workspace_sources};
use std::{cell::RefCell, path::PathBuf};
use zryna_diagnostics::Diagnostic;
use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

fn request(input: &NativeBuildRequest) -> BuildRequest {
    BuildRequest {
        workspace_root: input.workspace_root.clone(),
        entrypoint: input.entrypoint.clone(),
        artifact_stem: input.artifact_stem.clone(),
        targets: input.targets,
        // Legacy request storage only: this build route never discovers or issues a Node capability.
        node_runtime: PathBuf::new(),
    }
}

fn capture<'root>(
    root: &'root WorkspaceSourceRoot,
    entry: &str,
) -> Result<NativeSourceSnapshot<'root>, CommandFailure> {
    let path = NormalizedSourcePath::new(entry.to_owned()).map_err(|error| {
        failure(CommandFailureKind::Request, Diagnostic::from_source_error(&error))
    })?;
    capture_native_workspace_sources(root, path).map_err(|error| module_closure_failure(&error))
}

fn output(request: &BuildRequest) -> Result<(ArtifactOutputRoot, PathBuf), CommandFailure> {
    validate_request(request, CommandKind::Build)?;
    validate_architecture(&request.workspace_root)?;
    let root = ArtifactOutputRoot::prepare_for_workspace(&request.workspace_root)
        .map_err(|item| failure(CommandFailureKind::Preparation, item))?;
    let final_bundle = root.path().join(format!("{}.build", request.artifact_stem));
    ensure_absent(&final_bundle)?;
    Ok((root, final_bundle))
}

pub(crate) fn scalar(input: &NativeBuildRequest) -> Result<CommandSuccess, CommandFailure> {
    let request = request(input);
    let (output, final_bundle) = output(&request)?;
    let root = WorkspaceSourceRoot::capture(&request.workspace_root)
        .map_err(|item| failure(CommandFailureKind::Source, item))?;
    // M1 retains only its entry source, preserving its single-file source authority.
    let path = NormalizedSourcePath::new(request.entrypoint.clone()).map_err(|error| {
        failure(CommandFailureKind::Request, Diagnostic::from_source_error(&error))
    })?;
    let mut session =
        root.begin_discovery().map_err(|item| failure(CommandFailureKind::Source, item))?;
    let source =
        session.read_source(&path).map_err(|item| failure(CommandFailureKind::Source, item))?;
    let sources = SourceMap::build(vec![SourceFileInput {
        path: request.entrypoint.clone(),
        text: source.text.clone(),
    }])
    .map_err(|error| failure(CommandFailureKind::Source, Diagnostic::from_source_error(&error)))?;
    let tokens = zryna_frontend::native_lexer::lex(&sources)
        .map_err(|error| failure(CommandFailureKind::Source, error.diagnostic().clone()))?;
    let raw = zryna_frontend::native_parser::parse_v2_recovering_candidate(&sources, &tokens)
        .map_err(|error| failure(CommandFailureKind::Source, error.diagnostic().clone()))?;
    let syntax = zryna_frontend::syntax_v2::verify_snapshot(raw, &sources)
        .map_err(|diagnostics| CommandFailure { kind: CommandFailureKind::Source, diagnostics })?;
    let session = RefCell::new(session);
    let compiled = crate::lower_verified_syntax(&syntax, &sources)
        .map_err(|diagnostics| CommandFailure { kind: CommandFailureKind::Source, diagnostics })?;
    scalar::finish(&request, None, None, &output, &final_bundle, &source.text, &compiled, &|| {
        session
            .try_borrow_mut()
            .map_err(|_| {
                request_error(
                    "ZRYNA-C1010",
                    "retained source session is already borrowed",
                    "report this compiler invariant failure",
                )
            })?
            .revalidate_all()
            .map_err(|item| failure(CommandFailureKind::Source, item))
    })
}

pub(crate) fn control_flow(input: &NativeBuildRequest) -> Result<CommandSuccess, CommandFailure> {
    let request = request(input);
    let (output, final_bundle) = output(&request)?;
    let root = WorkspaceSourceRoot::capture(&request.workspace_root)
        .map_err(|item| failure(CommandFailureKind::Source, item))?;
    let snapshot = capture(&root, &request.entrypoint)?
        .verify_v3()
        .map_err(|error| module_closure_failure(&error))?;
    let request = ControlFlowBuildRequest {
        workspace_root: request.workspace_root,
        entrypoint: request.entrypoint,
        artifact_stem: request.artifact_stem,
        targets: request.targets,
        node_runtime: request.node_runtime,
    };
    control_flow::finish(&request, None, None, &output, &final_bundle, snapshot.closure(), &|_| {
        snapshot.revalidate().map_err(|error| module_closure_failure(&error))
    })
}
