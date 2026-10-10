//! Exact public source selection and create-only output admission.

use super::{CommandFailure, CommandFailureKind, ServerRunRequest, failure};
use crate::ArtifactOutputRoot;
use std::path::PathBuf;
use zryna_source::NormalizedSourcePath;

pub(super) fn validate_selection(
    request: &ServerRunRequest,
) -> Result<NormalizedSourcePath, CommandFailure> {
    if !request.workspace_root.is_absolute()
        || !request.node_runtime.is_absolute()
        || !request.configuration.is_absolute()
        || !request.listener_approval.is_absolute()
        || request.export.is_empty()
        || request.export.len() > 128
        || !request.export.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(failure(
            "ZRYNA-C4201",
            "Server selection requires exact absolute authority paths.",
        ));
    }
    crate::javascript::validate_artifact_stem(&request.artifact_stem).map_err(|diagnostic| {
        CommandFailure { kind: CommandFailureKind::Request, diagnostics: vec![diagnostic] }
    })?;
    let entry = NormalizedSourcePath::new(request.entrypoint.clone()).map_err(|_| {
        failure("ZRYNA-C4201", "Server source must be a portable workspace-relative file.")
    })?;
    let report = crate::check_workspace(&request.workspace_root);
    if !report.is_valid() {
        return Err(CommandFailure {
            kind: CommandFailureKind::Architecture,
            diagnostics: report.diagnostics,
        });
    }
    Ok(entry)
}

pub(super) fn prepare_output(
    request: &ServerRunRequest,
) -> Result<(ArtifactOutputRoot, PathBuf), CommandFailure> {
    let output = ArtifactOutputRoot::prepare_for_workspace(&request.workspace_root)
        .map_err(|_| failure("ZRYNA-C4205", "Server output root could not be retained."))?;
    let bundle = output.path().join(format!("{}.wasi-server-run", request.artifact_stem));
    match std::fs::symlink_metadata(&bundle) {
        Ok(_) => return Err(failure("ZRYNA-C4205", "Server output already exists.")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            return Err(failure("ZRYNA-C4205", "Server output could not be inspected."));
        }
    }
    Ok((output, bundle))
}
