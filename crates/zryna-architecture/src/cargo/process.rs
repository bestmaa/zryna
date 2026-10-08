use crate::cargo::capture::receive_process_reader;
use crate::cargo::capture::spawn_process_reader;
use crate::cargo::capture::validate_cargo_process_output;
use crate::cargo::metadata::CargoMetadataDocument;
use crate::diagnostics::ValidationDiagnostics;
use crate::diagnostics::architecture_error;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

const MAX_CARGO_METADATA_DURATION: Duration = Duration::from_secs(30);

const MAX_CARGO_STDERR_BYTES: usize = 64 * 1024;
const MAX_CARGO_METADATA_BYTES: usize = 16 * 1024 * 1024;
pub(crate) fn load_cargo_metadata(
    root: &Path,
    frozen: bool,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<CargoMetadataDocument> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    load_cargo_metadata_with_executable(
        root,
        frozen,
        cargo,
        MAX_CARGO_METADATA_DURATION,
        diagnostics,
    )
}

pub(crate) fn load_cargo_metadata_with_executable(
    root: &Path,
    frozen: bool,
    cargo: OsString,
    duration: Duration,
    diagnostics: &mut ValidationDiagnostics,
) -> Option<CargoMetadataDocument> {
    let mut command = Command::new(cargo);
    command
        .current_dir(root)
        .arg("metadata")
        .arg("--format-version=1")
        .arg("--all-features")
        .arg("--color=never")
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .env("CARGO_TERM_COLOR", "never")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if frozen {
        command.arg("--frozen");
    }
    let mut child = match zryna_process::spawn(|| command.spawn()) {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1101",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata could not be started: {error}"),
                "install the pinned Cargo toolchain and restore the locked dependency graph",
            ));
            return None;
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
        let _ = child.kill();
        let _ = child.wait();
        diagnostics.push(architecture_error(
            "ZRYNA-A1205",
            Some(Path::new("Cargo.toml")),
            "Cargo metadata process streams could not be inspected",
            "restore the pinned Cargo toolchain; incomplete graph inspection never passes",
        ));
        return None;
    };
    let stdout_reader = spawn_process_reader(stdout, MAX_CARGO_METADATA_BYTES);
    let stderr_reader = spawn_process_reader(stderr, MAX_CARGO_STDERR_BYTES);
    let deadline = Instant::now() + duration;
    let wait_result = wait_for_cargo_metadata(&mut child, deadline);
    let (status, timed_out) = match wait_result {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(architecture_error(
                "ZRYNA-A1205",
                Some(Path::new("Cargo.toml")),
                format!("Cargo metadata did not complete: {error}"),
                "restore the pinned Cargo toolchain; incomplete graph inspection never passes",
            ));
            return None;
        }
    };
    if timed_out {
        diagnostics.halt(architecture_error(
            "ZRYNA-A1204",
            Some(Path::new("Cargo.toml")),
            "Cargo metadata exceeded its deterministic execution budget",
            "reduce the locked dependency graph or restore the local Cargo cache",
        ));
        return None;
    }
    let stdout = receive_process_reader(&stdout_reader, "stdout", deadline, diagnostics);
    let stderr = receive_process_reader(&stderr_reader, "stderr", deadline, diagnostics);
    let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
        return None;
    };
    validate_cargo_process_output(status, &stdout, &stderr, diagnostics)
}

fn wait_for_cargo_metadata(
    child: &mut std::process::Child,
    deadline: Instant,
) -> std::io::Result<(std::process::ExitStatus, bool)> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok((status, false)),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                return child.wait().map(|status| (status, true));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    }
}
