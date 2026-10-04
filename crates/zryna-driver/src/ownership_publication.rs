//! Create-only atomic publication of complete candidate ownership bundles.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use zryna_diagnostics::Diagnostic;

use crate::{
    ArtifactOutputRoot, CommandFailure, CommandFailureKind, CommandKind, OWNERSHIP_MANIFEST_NAME,
    OwnershipManifestResult, OwnershipTarget, decode_ownership_manifest_v3,
    ownership_manifest::render_ownership_manifest_v3,
    ownership_pipeline::DataOwnershipCandidateSuccess, pipeline::Transaction,
};

/// One published artifact in canonical target order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedOwnershipArtifact {
    target: OwnershipTarget,
    path: PathBuf,
    bytes: u64,
    sha256: [u8; 32],
}

impl PublishedOwnershipArtifact {
    /// Returns the artifact target.
    #[must_use]
    pub const fn target(&self) -> OwnershipTarget {
        self.target
    }
    /// Returns the absolute published path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Returns the exact byte length.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
    /// Returns the authenticated SHA-256 digest.
    #[must_use]
    pub const fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
}

/// One complete create-only candidate bundle.
#[derive(Clone, Debug)]
pub struct PublishedOwnershipBundle {
    command: CommandKind,
    path: PathBuf,
    manifest_path: PathBuf,
    artifacts: Vec<PublishedOwnershipArtifact>,
    results: Vec<OwnershipManifestResult>,
    diagnostics: Vec<Diagnostic>,
}

impl PublishedOwnershipBundle {
    /// Returns the authenticated provider warnings retained in manifest v3.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    /// Returns build or run.
    #[must_use]
    pub const fn command(&self) -> CommandKind {
        self.command
    }
    /// Returns the absolute committed bundle directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Returns the strict manifest-v3 path.
    #[must_use]
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }
    /// Returns artifacts in canonical target order.
    #[must_use]
    pub fn artifacts(&self) -> &[PublishedOwnershipArtifact] {
        &self.artifacts
    }
    /// Returns normalized run outcomes.
    #[must_use]
    pub fn results(&self) -> &[OwnershipManifestResult] {
        &self.results
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublicationPhase {
    JavaScript,
    WebAssembly,
    Native,
    Manifest,
    Commit,
}

type Checkpoint<'a> = &'a dyn Fn(PublicationPhase) -> Result<(), CommandFailure>;

pub(crate) fn publish_installed_build(
    prepared: &crate::ownership_pipeline::installed::PreparedInstalledOwnership<'_, '_>,
) -> Result<PublishedOwnershipBundle, CommandFailure> {
    if prepared.success().logical_export().is_some() {
        return Err(transaction_failure("installed run requires sealed execution evidence"));
    }
    prepared.revalidate()?;
    publish_with_checkpoint(prepared.success(), &|_| prepared.revalidate())
}

pub(crate) fn publish_installed_after_staging<Runner>(
    prepared: &crate::ownership_pipeline::installed::PreparedInstalledOwnership<'_, '_>,
    runner: Runner,
) -> Result<PublishedOwnershipBundle, CommandFailure>
where
    Runner: FnOnce(
        &Transaction,
        &ArtifactOutputRoot,
    ) -> Result<Vec<OwnershipManifestResult>, CommandFailure>,
{
    prepared.revalidate()?;
    publish_with_runner(prepared.success(), &|_| prepared.revalidate(), runner)
}

/// Publishes all selected artifacts and manifest v3 with one create-only directory commit.
///
/// The function either returns one complete bundle or removes its private transaction. Existing
/// destinations are never replaced.
///
/// # Errors
/// Returns stable preparation, publication, or cleanup diagnostics without a partial final path.
pub(crate) fn publish_data_ownership_build(
    success: &DataOwnershipCandidateSuccess,
) -> Result<PublishedOwnershipBundle, CommandFailure> {
    if success.logical_export().is_some() {
        return Err(transaction_failure(
            "candidate run publication requires sealed execution evidence",
        ));
    }
    publish_with_checkpoint(success, &|_| Ok(()))
}

pub(crate) fn publish_with_checkpoint(
    success: &DataOwnershipCandidateSuccess,
    checkpoint: Checkpoint<'_>,
) -> Result<PublishedOwnershipBundle, CommandFailure> {
    publish_with_runner(success, checkpoint, |_, _| Ok(Vec::new()))
}

pub(crate) fn publish_after_staging<Runner>(
    success: &DataOwnershipCandidateSuccess,
    runner: Runner,
) -> Result<PublishedOwnershipBundle, CommandFailure>
where
    Runner: FnOnce(
        &Transaction,
        &ArtifactOutputRoot,
    ) -> Result<Vec<OwnershipManifestResult>, CommandFailure>,
{
    publish_with_runner(success, &|_| Ok(()), runner)
}

fn publish_with_runner<Runner>(
    success: &DataOwnershipCandidateSuccess,
    checkpoint: Checkpoint<'_>,
    runner: Runner,
) -> Result<PublishedOwnershipBundle, CommandFailure>
where
    Runner: FnOnce(
        &Transaction,
        &ArtifactOutputRoot,
    ) -> Result<Vec<OwnershipManifestResult>, CommandFailure>,
{
    let command =
        if success.logical_export().is_some() { CommandKind::Run } else { CommandKind::Build };
    let suffix = match command {
        CommandKind::Build => "build",
        CommandKind::Run => "run",
    };
    let output = ArtifactOutputRoot::prepare_for_workspace(success.workspace_root())
        .map_err(preparation_failure)?;
    let bundle = output.path().join(format!("{}.{suffix}", success.artifact_stem()));
    let mut transaction = Transaction::create(&output)?;
    let operation: Result<PublishedOwnershipBundle, CommandFailure> = (|| {
        let artifacts = stage_artifacts(&transaction, success, &bundle, checkpoint)?;
        let results = runner(&transaction, &output)?;
        let manifest = render_ownership_manifest_v3(success, success.artifact_stem(), &results)
            .map_err(preparation_failure)?;
        decode_ownership_manifest_v3(&manifest).map_err(preparation_failure)?;
        transaction.write_manifest(OWNERSHIP_MANIFEST_NAME, &manifest)?;
        checkpoint(PublicationPhase::Manifest)?;
        checkpoint(PublicationPhase::Commit)?;
        transaction.commit(&output, &bundle)?;
        Ok(PublishedOwnershipBundle {
            command,
            manifest_path: bundle.join(OWNERSHIP_MANIFEST_NAME),
            path: bundle,
            artifacts,
            results,
            diagnostics: success.diagnostics().to_vec(),
        })
    })();
    match operation {
        Ok(bundle) => Ok(bundle),
        Err(mut failure) => {
            if let Err(cleanup) = transaction.cleanup(&output) {
                failure.kind = CommandFailureKind::Cleanup;
                failure.diagnostics.extend(cleanup.diagnostics);
            }
            Err(failure)
        }
    }
}

fn stage_artifacts(
    transaction: &Transaction,
    success: &DataOwnershipCandidateSuccess,
    bundle: &Path,
    checkpoint: Checkpoint<'_>,
) -> Result<Vec<PublishedOwnershipArtifact>, CommandFailure> {
    let mut published = Vec::with_capacity(3);
    let stem = success.artifact_stem();
    let artifacts = success.artifacts();
    if let Some(artifact) = artifacts.javascript() {
        published.push(stage_artifact(
            transaction,
            bundle,
            OwnershipTarget::JavaScript,
            "ecmascript-module",
            stem,
            "mjs",
            artifact.source.as_bytes(),
        )?);
        checkpoint(PublicationPhase::JavaScript)?;
    }
    if let Some(artifact) = artifacts.webassembly() {
        published.push(stage_artifact(
            transaction,
            bundle,
            OwnershipTarget::WebAssembly,
            "core-webassembly-module",
            stem,
            "wasm",
            artifact.bytes(),
        )?);
        checkpoint(PublicationPhase::WebAssembly)?;
    }
    if let Some(artifact) = artifacts.native_object() {
        published.push(stage_artifact(
            transaction,
            bundle,
            OwnershipTarget::Native,
            "linux-x86-64-relocatable-object",
            stem,
            "o",
            artifact.bytes(),
        )?);
        checkpoint(PublicationPhase::Native)?;
    }
    if let Some(artifact) = artifacts.native_executable() {
        published.push(stage_artifact(
            transaction,
            bundle,
            OwnershipTarget::Native,
            "linux-x86-64-invocation-executable",
            stem,
            "elf",
            artifact.executable_bytes(),
        )?);
        checkpoint(PublicationPhase::Native)?;
    }
    if published.is_empty() || published.windows(2).any(|pair| pair[0].target >= pair[1].target) {
        return Err(transaction_failure("ownership artifact order is empty or invalid"));
    }
    Ok(published)
}

fn stage_artifact(
    transaction: &Transaction,
    bundle: &Path,
    target: OwnershipTarget,
    kind: &'static str,
    stem: &str,
    extension: &str,
    bytes: &[u8],
) -> Result<PublishedOwnershipArtifact, CommandFailure> {
    transaction.write_ownership_artifact(target, kind, stem, extension, bytes)?;
    let target_name = match target {
        OwnershipTarget::JavaScript => "javascript",
        OwnershipTarget::WebAssembly => "webassembly",
        OwnershipTarget::Native => "native",
    };
    let path = bundle.join(target_name).join(format!("{stem}.{extension}"));
    Ok(PublishedOwnershipArtifact {
        target,
        path,
        bytes: u64::try_from(bytes.len())
            .map_err(|_| transaction_failure("ownership artifact length exceeds u64"))?,
        sha256: Sha256::digest(bytes).into(),
    })
}

fn preparation_failure(diagnostic: Diagnostic) -> CommandFailure {
    CommandFailure { kind: CommandFailureKind::Preparation, diagnostics: vec![diagnostic] }
}

fn transaction_failure(message: impl Into<String>) -> CommandFailure {
    CommandFailure {
        kind: CommandFailureKind::Preparation,
        diagnostics: vec![Diagnostic::error(
            "ZRYNA-C3201",
            None,
            message,
            "retry with one absent contained ownership bundle destination",
        )],
    }
}

#[cfg(test)]
mod tests;
