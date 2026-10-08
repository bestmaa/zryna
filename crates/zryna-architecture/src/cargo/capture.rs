use crate::cargo::metadata::CargoMetadataDocument;
use crate::cargo::metadata::validate_cargo_metadata_limits;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::time::Instant;

pub(crate) fn spawn_process_reader(
    stream: impl Read + Send + 'static,
    limit: usize,
) -> Receiver<std::io::Result<BoundedProcessStream>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = sender.send(read_process_stream(stream, limit));
    });
    receiver
}

pub(crate) fn receive_process_reader(
    receiver: &Receiver<std::io::Result<BoundedProcessStream>>,
    stream_name: &str,
    deadline: Instant,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<BoundedProcessStream> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    match receiver.recv_timeout(remaining) {
        Ok(Ok(value)) => Some(value),
        Ok(Err(error)) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata {stream_name} could not be read: {error}"),
                "restore stable process I/O and retry architecture validation",
            ));
            None
        }
        Err(RecvTimeoutError::Disconnected) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata {stream_name} reader failed"),
                "restore stable process I/O and retry architecture validation",
            ));
            None
        }
        Err(RecvTimeoutError::Timeout) => {
            diagnostics.halt(architecture_error(
                "ZRYNA-A1204",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata {stream_name} exceeded its execution budget"),
                "stop descendant processes retaining Cargo output streams and retry",
            ));
            None
        }
    }
}

pub(crate) fn validate_cargo_process_output(
    status: std::process::ExitStatus,
    stdout: &BoundedProcessStream,
    stderr: &BoundedProcessStream,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<CargoMetadataDocument> {
    if stdout.exceeded || stderr.exceeded {
        diagnostics.halt(architecture_error(
            "ZRYNA-A1204",
            Some(Path::new("Cargo.toml")),
            "Cargo metadata exceeded its deterministic process-output budget",
            "reduce the locked dependency graph; incomplete metadata never passes",
        ));
        return None;
    }
    if !status.success() {
        diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            "Cargo metadata rejected the locked workspace",
            "repair Cargo manifests and Cargo.lock with the pinned toolchain",
        ));
        return None;
    }
    let metadata: CargoMetadataDocument = match serde_json::from_slice(&stdout.bytes) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata output is invalid: {error}"),
                "use the pinned Cargo toolchain and metadata format version 1",
            ));
            return None;
        }
    };
    if metadata.version != 1 {
        diagnostics.push(architecture_error(
            "ZRYNA-A1101",
            Some(Path::new("Cargo.toml")),
            format!("Cargo metadata format version {} is unsupported", metadata.version),
            "use Cargo metadata format version 1",
        ));
        return None;
    }
    if !validate_cargo_metadata_limits(&metadata, diagnostics) {
        return None;
    }
    Some(metadata)
}

pub(crate) struct BoundedProcessStream {
    pub(crate) bytes: Vec<u8>,
    pub(crate) exceeded: bool,
}

pub(crate) fn read_process_stream(
    mut stream: impl Read,
    limit: usize,
) -> std::io::Result<BoundedProcessStream> {
    let mut bytes = Vec::with_capacity(limit.min(8192));
    let mut exceeded = false;
    let mut chunk = [0_u8; 8192];
    loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let stored = remaining.min(count);
        bytes.extend_from_slice(&chunk[..stored]);
        if stored != count {
            exceeded = true;
        }
    }
    Ok(BoundedProcessStream { bytes, exceeded })
}
