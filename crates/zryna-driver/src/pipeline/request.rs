//! Shared request, architecture and no-follow path validation.

use super::{
    BuildRequest, CommandFailure, CommandFailureKind, CommandKind, entrypoint_error, failure,
    metadata_is_link_or_reparse, request_error, unsupported_component_request,
    validate_artifact_stem,
};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use zryna_diagnostics::Diagnostic;

pub(super) struct ValidatedRequest {
    pub(super) source_path: PathBuf,
}

pub(super) fn validate_architecture(root: &Path) -> Result<(), CommandFailure> {
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

pub(super) fn validate_request(
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

pub(super) fn validate_real_directory(path: &Path) -> Result<(), Diagnostic> {
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
