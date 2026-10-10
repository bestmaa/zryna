//! Target preparation from one default-profile verified program.

use std::{fs, path::Path};

use zryna_frontend::{
    FrontendCapabilities, ProviderExpectation, VerifiedFrontendProvider, WorkerFrontend,
    WorkerLimits, WorkerSpec, syntax_v2,
};
use zryna_source::SourceMap;

use super::{
    CommandFailure, CommandFailureKind, FailureKindExt, NATIVE_TARGET, TargetSelection,
    entrypoint_error, failure, metadata_is_link_or_reparse, preparation_failure, source_failure,
    validate_real_directory,
};
use crate::runtime::{NodeRuntimeCapability, node_compatible_path};

pub(super) struct PreparedArtifacts {
    pub(super) javascript: Option<zryna_backend_javascript::JavaScriptArtifact>,
    pub(super) webassembly: Option<zryna_backend_webassembly::ValidatedWebAssemblyArtifact>,
    pub(super) native_object: Option<zryna_backend_native::ValidatedNativeObjectArtifact>,
    pub(super) native_executable: Option<crate::native::PreparedNativeExecutable>,
    pub(super) component: Option<zryna_backend_webassembly::ValidatedScalarComponent>,
}

#[cfg(test)]
pub(super) fn compile_selected<Provider: VerifiedFrontendProvider + ?Sized>(
    frontend: &Provider,
    sources: &SourceMap,
    targets: TargetSelection,
) -> Result<(crate::SourceToIrSuccess, PreparedArtifacts), CommandFailure> {
    let compiled = analyze(frontend, sources)?;
    let prepared = prepare_selected(&compiled, targets)?;
    Ok((compiled, prepared))
}

pub(super) fn analyze<Provider: VerifiedFrontendProvider + ?Sized>(
    frontend: &Provider,
    sources: &SourceMap,
) -> Result<crate::SourceToIrSuccess, CommandFailure> {
    crate::compile_to_verified_ir(frontend, sources).map_err(|error| source_failure(&error))
}

pub(crate) fn configured_frontend(
    compiler_root: &Path,
    node: &NodeRuntimeCapability,
) -> Result<WorkerFrontend, CommandFailure> {
    let adapter_root = compiler_root.join("adapters/typescript-6");
    validate_real_directory(&adapter_root)
        .map_err(|diagnostic| failure(CommandFailureKind::Preparation, diagnostic))?;
    let worker_entrypoint = adapter_root.join("src/worker.mjs");
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
    let expected = ProviderExpectation::new(
        "typescript-6",
        "6.0.3",
        syntax_v2::PROTOCOL_VERSION,
        FrontendCapabilities { module_resolution: false, semantic_diagnostics: false },
    )
    .map_err(|error| CommandFailure {
        kind: CommandFailureKind::Preparation,
        diagnostics: error.diagnostics().to_vec(),
    })?;
    let spec = WorkerSpec::new(
        node.executable().map_err(preparation_failure)?,
        vec![node_worker_entrypoint.into_os_string()],
        node_adapter_root,
        expected,
        WorkerLimits::default(),
    )
    .map_err(|error| CommandFailure {
        kind: CommandFailureKind::Preparation,
        diagnostics: error.diagnostics().to_vec(),
    })?;
    Ok(WorkerFrontend::new(spec))
}

#[cfg(test)]
pub(super) fn prepare_selected(
    compiled: &crate::SourceToIrSuccess,
    targets: TargetSelection,
) -> Result<PreparedArtifacts, CommandFailure> {
    prepare_selected_guarded(compiled, targets, &|| Ok(()))
}

pub(super) fn prepare_selected_guarded(
    compiled: &crate::SourceToIrSuccess,
    targets: TargetSelection,
    checkpoint: &dyn Fn() -> Result<(), CommandFailure>,
) -> Result<PreparedArtifacts, CommandFailure> {
    let program = compiled.program();
    let javascript = if targets.javascript() {
        checkpoint()?;
        Some(zryna_backend_javascript::emit(program).map_err(preparation_failure)?)
    } else {
        None
    };
    let webassembly = if targets.webassembly() {
        checkpoint()?;
        Some(zryna_backend_webassembly::emit(program).map_err(preparation_failure)?)
    } else {
        None
    };
    let native_object = if targets.native() {
        checkpoint()?;
        let target =
            crate::select_native_object_target(NATIVE_TARGET).map_err(preparation_failure)?;
        let mir = zryna_native_mir::lower(program).map_err(|diagnostics| CommandFailure {
            kind: CommandFailureKind::Preparation,
            diagnostics,
        })?;
        Some(zryna_backend_native::emit_object(&mir, target).map_err(preparation_failure)?)
    } else {
        None
    };
    let component = if targets.component() {
        checkpoint()?;
        Some(
            zryna_backend_webassembly::emit_scalar_component(
                program,
                &zryna_backend_webassembly::pinned_wit_sources(),
            )
            .map_err(preparation_failure)?,
        )
    } else {
        None
    };
    Ok(PreparedArtifacts {
        javascript,
        webassembly,
        native_object,
        native_executable: None,
        component,
    })
}
