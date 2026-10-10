//! Authenticated pure-source loopback service with separate create-only public records.

mod approval;
mod record;

use crate::{
    ArtifactOutputRoot, CommandFailure, CommandFailureKind, WorkspaceSourceRoot,
    pipeline::Transaction, runtime::NodeRuntimeCapability, server_runtime, server_transport,
};
use approval::Inputs;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use zryna_diagnostics::Diagnostic;
use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

/// Explicit source-checkout selection; no installed-package or ambient guest authority.
#[derive(Clone)]
pub struct ServerRunRequest {
    /// Absolute compiler workspace root.
    pub workspace_root: PathBuf,
    /// One portable workspace-relative source.
    pub entrypoint: String,
    /// Sole no-argument i32 source export.
    pub export: String,
    /// Fresh portable output stem.
    pub artifact_stem: String,
    /// Absolute pinned Node syntax-provider executable.
    pub node_runtime: PathBuf,
    /// Absolute caller-private configuration file.
    pub configuration: PathBuf,
    /// Separate absolute caller-private root listener approval.
    pub listener_approval: PathBuf,
}

/// Issued only after authenticated preparation and successful loopback binding.
#[derive(Clone)]
pub struct ServerReadiness {
    address: SocketAddr,
    control: Arc<server_transport::Control>,
}
impl ServerReadiness {
    /// Actual bounded loopback endpoint, including an OS-selected ephemeral port.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }
    /// Requests cancellation and interrupts the current connection. Serving joins teardown before publication.
    pub fn cancel(&self) {
        self.control.stop();
    }
}

/// Complete server record and component published only after observed teardown.
pub struct PublishedServerBundle {
    path: PathBuf,
    manifest: serde_json::Value,
}
impl PublishedServerBundle {
    /// Create-only bundle directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Bounded exact-identity manifest and actual execution observations.
    #[must_use]
    pub const fn manifest(&self) -> &serde_json::Value {
        &self.manifest
    }
}

/// Runs a finite pure-source server, calls readiness once, and commits a separate result record.
///
/// # Errors
/// Rejects malformed authority/source/configuration and suppresses publication after revocation.
/// Existing output is never replaced. Forced host termination cannot certify graceful cleanup.
pub fn serve_workspace(
    request: &ServerRunRequest,
    ready: impl FnOnce(ServerReadiness) -> Result<(), Diagnostic>,
) -> Result<PublishedServerBundle, CommandFailure> {
    let entry = validate_selection(request)?;
    let inputs = Inputs::capture(&request.configuration, &request.listener_approval)?;
    let (output, bundle) = prepare_output(request)?;
    let source_root = WorkspaceSourceRoot::capture(&request.workspace_root)
        .map_err(|_| failure("ZRYNA-C4202", "Server source root could not be retained."))?;
    let mut session = source_root
        .begin_discovery()
        .map_err(|_| failure("ZRYNA-C4202", "Server source discovery could not start."))?;
    let source = session
        .read_source(&entry)
        .map_err(|_| failure("ZRYNA-C4202", "Server source could not be retained."))?;
    let source_bytes = source.text.as_bytes().to_vec();
    let sources = SourceMap::build(vec![SourceFileInput {
        path: request.entrypoint.clone(),
        text: source.text,
    }])
    .map_err(|_| failure("ZRYNA-C4201", "Server source exceeds the single-source limits."))?;
    let node = NodeRuntimeCapability::discover(&request.node_runtime, &request.workspace_root)
        .map_err(|_| {
            failure("ZRYNA-C4203", "Pinned server syntax-provider runtime was rejected.")
        })?;
    let frontend =
        crate::pipeline::preparation::configured_frontend(&request.workspace_root, &node)?;
    session
        .validate_provider_batch(&sources)
        .map_err(|_| failure("ZRYNA-C4202", "Server source/provider binding was rejected."))?;
    let transport = Arc::new(server_transport::Observation::default());
    let runtime = Arc::new(server_runtime::Observation::default());
    let prepared = server_runtime::Prepared::new_public(
        &frontend,
        sources,
        server_runtime::Preparation {
            export: &request.export,
            operation: zryna_backend_webassembly::ServerOperation::Reply,
            document: br#"{"world":"zryna:capability-profiles/server@0.1.0","requests":[]}"#,
            approval: server_runtime::Approval::deny_all(),
            envelope: inputs.config.envelope(),
        },
        Arc::clone(&runtime),
    )
    .map_err(|_| failure("ZRYNA-C4203", "Authenticated pure server source preparation failed."))?;
    let material = record::material(
        &prepared,
        &request.entrypoint,
        &source_bytes,
        &request.export,
        &request.artifact_stem,
        &inputs,
    )?;
    let mut revalidate = || {
        session
            .revalidate_all()
            .map_err(|_| failure("ZRYNA-C4202", "Original server source changed."))?;
        node.revalidate()
            .map_err(|_| failure("ZRYNA-C4202", "Original syntax-provider runtime changed."))?;
        inputs.revalidate()
    };
    revalidate()?;
    let mut transaction = Transaction::create(&output)?;
    let operation = (|| {
        transaction.write_server_artifact(&request.artifact_stem, prepared.artifact().bytes())?;
        revalidate()?;
        let bound = server_transport::Bound::start(inputs.config, prepared, Arc::clone(&transport))
            .map_err(|_| failure("ZRYNA-C4204", "Bounded loopback listener could not start."))?;
        let address = bound.address();
        ready(ServerReadiness { address, control: bound.control() })
            .map_err(|_| failure("ZRYNA-C4204", "Server readiness publication failed."))?;
        let result =
            bound.run_guarded(&mut || revalidate().map_err(|_| server_transport::Error::Authority));
        revalidate()?;
        let manifest = record::complete(material, address, result, &transport, &runtime)?;
        let bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|_| failure("ZRYNA-C4205", "Server result encoding failed."))?;
        if bytes.len() > 65536 {
            return Err(failure("ZRYNA-C4205", "Server result exceeds its byte bound."));
        }
        transaction
            .write_manifest(record::NAME, &bytes)
            .map_err(|_| failure("ZRYNA-C4205", "Server manifest publication failed."))?;
        revalidate()?;
        transaction
            .commit(&output, &bundle)
            .map_err(|_| failure("ZRYNA-C4205", "Create-only server record commit failed."))?;
        Ok(PublishedServerBundle { path: bundle, manifest })
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

pub(crate) fn failure(code: &'static str, message: &'static str) -> CommandFailure {
    CommandFailure {
        kind: match code {
            "ZRYNA-C4202" | "ZRYNA-C4203" => CommandFailureKind::Preparation,
            "ZRYNA-C4204" => CommandFailureKind::Execution,
            "ZRYNA-C4205" => CommandFailureKind::Cleanup,
            _ => CommandFailureKind::Request,
        },
        diagnostics: vec![Diagnostic::error(
            code,
            None,
            message,
            "Use the exact server-status-v1 source, private configuration and explicit loopback listener approval.",
        )],
    }
}

fn validate_selection(request: &ServerRunRequest) -> Result<NormalizedSourcePath, CommandFailure> {
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

fn prepare_output(
    request: &ServerRunRequest,
) -> Result<(ArtifactOutputRoot, PathBuf), CommandFailure> {
    let output = ArtifactOutputRoot::prepare_for_workspace(&request.workspace_root)
        .map_err(|_| failure("ZRYNA-C4205", "Server output root could not be retained."))?;
    let bundle = output.path().join(format!("{}.wasi-server-run", request.artifact_stem));
    match std::fs::symlink_metadata(&bundle) {
        Ok(_) => return Err(failure("ZRYNA-C4205", "Server output already exists.")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(failure("ZRYNA-C4205", "Server output could not be inspected.")),
    }
    Ok((output, bundle))
}
