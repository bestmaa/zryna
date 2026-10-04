//! Default-profile execution over one verified program and retained command proof.

use super::{
    ArtifactOutputRoot, BuildRequest, CommandFailure, CommandFailureKind, CommandKind,
    CommandSuccess, NativeProcessLimits, NodeRuntimeCapability, RunInvocation, analyze,
    commit_prepared, discover_linux_native_toolchain, ensure_absent, failure,
    native_preparation_failure, preparation_failure, prepare_native_invocation_from_verified,
    request_error, validate_request,
};
use crate::distribution::{
    InstalledAdmission, InstalledBuildRequest, InstalledCompiler, InstalledProfile,
};
use std::path::Path;
use zryna_diagnostics::Diagnostic;
use zryna_source::{SourceFileInput, SourceMap};

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) fn finish(
    request: &BuildRequest,
    run: Option<&RunInvocation>,
    node: Option<&NodeRuntimeCapability>,
    output_root: &ArtifactOutputRoot,
    final_bundle: &Path,
    source_text: &str,
    compiled: &crate::SourceToIrSuccess,
    checkpoint: &dyn Fn() -> Result<(), CommandFailure>,
) -> Result<CommandSuccess, CommandFailure> {
    let command = if run.is_some() { CommandKind::Run } else { CommandKind::Build };
    let mut prepared =
        super::preparation::prepare_selected_guarded(compiled, request.targets, checkpoint)?;
    if let Some(node) = node {
        node.revalidate().map_err(preparation_failure)?;
    } else if run.is_some() {
        return Err(request_error(
            "ZRYNA-C1010",
            "run requires a retained Node capability",
            "use the existing run route",
        ));
    }
    let program = compiled.program();
    let verified_invocation = run
        .map(|invocation| {
            program.prepare_invocation(zryna_abi::Invocation::new(
                invocation.logical_export.clone(),
                invocation.arguments.clone(),
            ))
        })
        .transpose()
        .map_err(|error| {
            failure(
                CommandFailureKind::Source,
                Diagnostic::error(
                    error.code(),
                    None,
                    "run invocation does not match the verified scalar ABI export",
                    "use the exact export, arity, and scalar argument types",
                ),
            )
        })?;

    let native_executable = if run.is_some() && request.targets.native() {
        checkpoint()?;
        let invocation = verified_invocation.as_ref().ok_or_else(|| {
            request_error(
                "ZRYNA-C1010",
                "verified invocation preparation was not completed",
                "report this compiler invariant failure",
            )
        })?;
        let object = prepared.native_object.as_ref().ok_or_else(|| {
            request_error(
                "ZRYNA-C1010",
                "native object preparation was not completed",
                "report this compiler invariant failure",
            )
        })?;
        let toolchain = discover_linux_native_toolchain(NativeProcessLimits::default())
            .map_err(preparation_failure)?;
        Some(
            prepare_native_invocation_from_verified(
                program,
                object,
                invocation,
                output_root,
                &toolchain,
                NativeProcessLimits::default(),
            )
            .map_err(native_preparation_failure)?,
        )
    } else {
        None
    };

    let mut diagnostics = compiled.diagnostics().to_vec();
    if let Some(executable) = &native_executable {
        diagnostics.extend_from_slice(executable.diagnostics());
    }
    prepared.native_executable = native_executable;
    commit_prepared(
        request,
        command,
        run,
        verified_invocation.as_ref(),
        node,
        output_root,
        final_bundle,
        source_text,
        &prepared,
        diagnostics,
        checkpoint,
    )
}

pub(crate) fn execute_installed(
    installation: &InstalledCompiler,
    request: &InstalledBuildRequest,
    run: Option<(String, Vec<zryna_abi::ScalarValue>)>,
) -> Result<CommandSuccess, CommandFailure> {
    if request.profile != InstalledProfile::I32 {
        return Err(request_error(
            "ZRYNA-C1001",
            "installed scalar profile mismatch",
            "select i32-v1 for the installed scalar route",
        ));
    }
    let admission = InstalledAdmission::prepare(installation, request)?;
    let request = admission.request().as_workspace_request();
    let invocation =
        run.map(|(logical_export, arguments)| RunInvocation { logical_export, arguments });
    let command = if invocation.is_some() { CommandKind::Run } else { CommandKind::Build };
    validate_request(&request, command)?;
    let output_root = ArtifactOutputRoot::prepare_for_workspace(&request.workspace_root)
        .map_err(|diagnostic| failure(CommandFailureKind::Preparation, diagnostic))?;
    let final_bundle =
        output_root.path().join(format!("{}.{}", request.artifact_stem, command.suffix()));
    ensure_absent(&final_bundle)?;
    let source_text = admission.project().entrypoint_text()?;
    let sources = SourceMap::build(vec![SourceFileInput {
        path: request.entrypoint.clone(),
        text: source_text.clone(),
    }])
    .map_err(|error| failure(CommandFailureKind::Source, Diagnostic::from_source_error(&error)))?;
    let frontend = admission.execution().frontend_v2()?;
    admission.revalidate()?;
    let compiled = analyze(&frontend, &sources)?;
    admission.revalidate()?;
    finish(
        &request,
        invocation.as_ref(),
        Some(admission.execution().node()?),
        &output_root,
        &final_bundle,
        &source_text,
        &compiled,
        &|| admission.revalidate(),
    )
}
