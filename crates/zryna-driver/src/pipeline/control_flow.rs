//! Shared M2 preparation and publication from an already sealed module closure.

use std::path::Path;

use zryna_diagnostics::Diagnostic;
use zryna_source::NormalizedSourcePath;

use super::{
    ArtifactOutputRoot, BuildRequest, CommandFailure, CommandFailureKind, CommandKind,
    CommandSuccess, ControlFlowBuildRequest, ControlFlowCheckpoint, ControlFlowPhase,
    NativeProcessLimits, NodeRuntimeCapability, RunInvocation, commit_control_flow_prepared,
    compile_control_flow_selected, discover_linux_native_toolchain, ensure_absent, failure,
    module_closure_failure, native_preparation_failure, preparation_failure,
    prepare_control_flow_native_invocation_from_verified, request_error,
    unsupported_component_request, validate_request,
};
use crate::{
    VerifiedModuleClosure,
    distribution::{
        InstalledAdmission, InstalledBuildRequest, InstalledCompiler, InstalledProfile,
    },
};

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) fn finish(
    request: &ControlFlowBuildRequest,
    run: Option<&RunInvocation>,
    node: Option<&NodeRuntimeCapability>,
    output_root: &ArtifactOutputRoot,
    final_bundle: &Path,
    closure: &VerifiedModuleClosure,
    checkpoint: ControlFlowCheckpoint<'_>,
) -> Result<CommandSuccess, CommandFailure> {
    let command = if run.is_some() { CommandKind::Run } else { CommandKind::Build };
    checkpoint(ControlFlowPhase::Semantics)?;
    let scalar_source = crate::scalar_adapter_interface::lower_verified_scalar_source(closure)
        .map_err(|diagnostics| CommandFailure { kind: CommandFailureKind::Source, diagnostics })?;
    let verified_invocation = run
        .map(|invocation| {
            scalar_source.program().prepare_invocation(zryna_abi::Invocation::new(
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
                    "run invocation does not match the verified ControlFlowV1 scalar ABI export",
                    "use the exact entry-module export, arity, and scalar argument types",
                ),
            )
        })?;
    let mut prepared = compile_control_flow_selected(&scalar_source, request.targets, checkpoint)?;
    if let Some(node) = node {
        node.revalidate().map_err(preparation_failure)?;
    } else if run.is_some() {
        return Err(request_error(
            "ZRYNA-C1010",
            "run requires a retained Node capability",
            "use the existing run route",
        ));
    }

    if run.is_some() && request.targets.native() {
        checkpoint(ControlFlowPhase::NativeLink)?;
        let invocation = verified_invocation.as_ref().ok_or_else(|| {
            request_error(
                "ZRYNA-C1010",
                "verified M2 invocation preparation was not completed",
                "report this compiler invariant failure",
            )
        })?;
        let object = prepared.native_object.as_ref().ok_or_else(|| {
            request_error(
                "ZRYNA-C1010",
                "M2 native object preparation was not completed",
                "report this compiler invariant failure",
            )
        })?;
        let toolchain = discover_linux_native_toolchain(NativeProcessLimits::default())
            .map_err(preparation_failure)?;
        prepared.native_executable = Some(
            prepare_control_flow_native_invocation_from_verified(
                scalar_source.program(),
                object,
                invocation,
                output_root,
                &toolchain,
                NativeProcessLimits::default(),
            )
            .map_err(native_preparation_failure)?,
        );
    }

    let diagnostics = closure
        .syntax()
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.severity() == zryna_diagnostics::Severity::Warning)
        .cloned()
        .collect::<Vec<_>>();
    commit_control_flow_prepared(
        request,
        command,
        run,
        verified_invocation.as_ref(),
        node,
        output_root,
        final_bundle,
        closure,
        &prepared,
        diagnostics,
        checkpoint,
    )
}

/// The retained proof stays on this stack until all shared publication phases finish.
pub(crate) fn execute_installed(
    installation: &InstalledCompiler,
    request: &InstalledBuildRequest,
    run: Option<(String, Vec<zryna_abi::ScalarValue>)>,
) -> Result<CommandSuccess, CommandFailure> {
    if request.profile != InstalledProfile::ControlFlow {
        return Err(request_error(
            "ZRYNA-C1001",
            "installed M2 profile mismatch",
            "select control-flow-v1 for the installed M2 route",
        ));
    }
    let admission = InstalledAdmission::prepare(installation, request)?;
    let project = admission.request();
    let request = ControlFlowBuildRequest {
        workspace_root: project.project_root.clone(),
        entrypoint: project.entrypoint.clone(),
        artifact_stem: project.artifact_stem.clone(),
        targets: project.targets,
        node_runtime: project.node_runtime.clone(),
    };
    let invocation =
        run.map(|(logical_export, arguments)| RunInvocation { logical_export, arguments });
    let command = if invocation.is_some() { CommandKind::Run } else { CommandKind::Build };
    if request.targets.component() {
        return Err(unsupported_component_request(
            "component emission currently accepts only the default scalar profile",
        ));
    }
    validate_request(
        &BuildRequest {
            workspace_root: request.workspace_root.clone(),
            entrypoint: request.entrypoint.clone(),
            artifact_stem: request.artifact_stem.clone(),
            targets: request.targets,
            node_runtime: request.node_runtime.clone(),
        },
        command,
    )?;
    let output_root = ArtifactOutputRoot::prepare_for_workspace(&request.workspace_root)
        .map_err(|diagnostic| failure(CommandFailureKind::Preparation, diagnostic))?;
    let final_bundle =
        output_root.path().join(format!("{}.{}", request.artifact_stem, command.suffix()));
    ensure_absent(&final_bundle)?;
    let entrypoint = NormalizedSourcePath::new(request.entrypoint.clone()).map_err(|error| {
        failure(CommandFailureKind::Request, Diagnostic::from_source_error(&error))
    })?;
    let frontend = admission.execution().frontend_v3()?;
    admission.revalidate()?;
    let closure = crate::module_closure::discover_module_closure_with_clock(
        &admission.sources(),
        entrypoint,
        &frontend,
        std::time::Instant::now,
    )
    .map_err(|error| module_closure_failure(&error))?;
    let result = finish(
        &request,
        invocation.as_ref(),
        Some(admission.execution().node()?),
        &output_root,
        &final_bundle,
        &closure,
        &|_| admission.revalidate(),
    )?;
    admission.revalidate()?;
    Ok(result)
}
