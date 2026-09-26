//! Exact target artifact inventory staged by the atomic command transaction.

use super::{
    BuildRequest, CommandFailure, CommandKind, ManifestTarget, PreparedArtifacts,
    PublishedTargetArtifact, TargetSelection, Transaction, request_error,
};

pub(super) fn write_prepared_artifacts(
    transaction: &Transaction,
    request: &BuildRequest,
    command: CommandKind,
    prepared: &PreparedArtifacts,
) -> Result<(Vec<PublishedTargetArtifact>, Option<usize>), CommandFailure> {
    let mut artifacts = Vec::with_capacity(request.targets.ordered().len());
    let mut core_offset = None;
    if let Some(artifact) = &prepared.javascript {
        artifacts.push(transaction.write_artifact(
            ManifestTarget::JavaScript,
            "ecmascript-module",
            &request.artifact_stem,
            "mjs",
            artifact.source.as_bytes(),
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
    if let Some(artifact) = &prepared.component {
        artifacts.push(transaction.write_artifact(
            ManifestTarget::Component,
            "webassembly-component",
            &request.artifact_stem,
            "wasm",
            artifact.bytes(),
        )?);
        if request.targets == TargetSelection::BrowserComponent {
            let bindings =
                crate::browser_component::generate(artifact).map_err(super::preparation_failure)?;
            core_offset = Some(bindings.core_offset);
            artifacts.push(transaction.write_artifact(
                ManifestTarget::Component,
                "ecmascript-module",
                &request.artifact_stem,
                "mjs",
                &bindings.loader,
            )?);
            artifacts.push(transaction.write_artifact(
                ManifestTarget::Component,
                "typescript-declarations",
                &request.artifact_stem,
                "d.mts",
                &bindings.declarations,
            )?);
        }
    }
    if request.targets.native() {
        let (kind, extension, bytes): (&str, &str, &[u8]) = if command == CommandKind::Run {
            let executable = prepared.native_executable.as_ref().ok_or_else(|| {
                request_error(
                    "ZRYNA-C1010",
                    "native executable preparation was not completed",
                    "report this compiler invariant failure",
                )
            })?;
            ("linux-x86-64-invocation-executable", "elf", executable.bytes())
        } else {
            let object = prepared.native_object.as_ref().ok_or_else(|| {
                request_error(
                    "ZRYNA-C1010",
                    "native object preparation was not completed",
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
    Ok((artifacts, core_offset))
}
