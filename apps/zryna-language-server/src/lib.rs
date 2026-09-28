//! Bounded stdio language-server transport for compiler-owned diagnostics and definition queries.

#![forbid(unsafe_code)]

mod coordinates;
mod diagnostics;
mod documents;
mod framing;
mod params;
mod protocol;
mod server;

use std::{
    io::{self, BufReader, BufWriter},
    path::Path,
    sync::mpsc,
    thread,
    time::Duration,
};

pub use framing::{FrameError, MAX_LSP_MESSAGE_BYTES, read_frame, write_frame};
pub use server::{MAX_OUTSTANDING_REQUESTS, Outgoing, RevisionCompiler, Server};
use zryna_driver::diagnostic_sessions::ToolingCompiler;

/// Runs the bounded language server over standard input and output.
///
/// # Errors
///
/// Returns an error for invalid compiler configuration, malformed framing, I/O failure, or an
/// unexpected end of the protocol before `exit`.
pub fn run_stdio(compiler_root: &Path, node: &Path) -> Result<(), String> {
    let compiler =
        ToolingCompiler::discover(compiler_root, node).map_err(|error| error.to_string())?;
    serve(compiler, None)
}

/// Runs a source-built server with one trusted, retained workspace source root.
///
/// # Errors
/// Rejects unsafe root or compiler configuration and protocol or I/O failures.
pub fn run_stdio_with_workspace_root(
    compiler_root: &Path,
    node: &Path,
    workspace_root: &Path,
) -> Result<(), String> {
    let compiler =
        ToolingCompiler::discover(compiler_root, node).map_err(|error| error.to_string())?;
    serve(compiler, Some(workspace_root))
}

/// Serves scalar tooling with the fixed runtime from a verified installed distribution.
///
/// # Errors
/// Rejects incompatible runtime bytes, malformed framing and protocol or I/O failures.
pub fn run_installed_stdio(root: &Path) -> Result<(), String> {
    let compiler = ToolingCompiler::discover_installed(root).map_err(|error| error.to_string())?;
    serve(compiler, None)
}

/// Runs installed tooling with one trusted, retained workspace source root.
///
/// # Errors
/// Rejects unsafe root or installed configuration and protocol or I/O failures.
pub fn run_installed_stdio_with_workspace_root(
    installed_root: &Path,
    workspace_root: &Path,
) -> Result<(), String> {
    let compiler =
        ToolingCompiler::discover_installed(installed_root).map_err(|error| error.to_string())?;
    serve(compiler, Some(workspace_root))
}

fn serve(compiler: ToolingCompiler, workspace_root: Option<&Path>) -> Result<(), String> {
    let mut server = Server::new(compiler).map_err(|error| error.to_string())?;
    if let Some(root) = workspace_root {
        server.configure_workspace_root(root)?;
    }
    let (sender, receiver) = mpsc::sync_channel(32);
    let reader = thread::spawn(move || {
        let mut input = BufReader::new(io::stdin());
        loop {
            match read_frame(&mut input) {
                Ok(Some(frame)) => {
                    if sender.send(Ok(frame)).is_err() {
                        return;
                    }
                }
                Ok(None) => {
                    let _ = sender.send(Err("standard input ended before exit".to_owned()));
                    return;
                }
                Err(error) => {
                    let _ = sender.send(Err(error.to_string()));
                    return;
                }
            }
        }
    });
    let mut output = BufWriter::new(io::stdout());
    let mut messages_with_pending = 0_u8;
    loop {
        match receiver.recv_timeout(Duration::from_millis(2)) {
            Ok(Ok(frame)) => {
                for message in server.handle_bytes(&frame) {
                    server.write_outgoing(&mut output, message)?;
                }
                if server.should_exit() {
                    break;
                }
                if server.has_pending() {
                    messages_with_pending = messages_with_pending.saturating_add(1);
                    if messages_with_pending >= 32 {
                        for message in server.finish_pending() {
                            server.write_outgoing(&mut output, message)?;
                        }
                        messages_with_pending = 0;
                    }
                } else {
                    messages_with_pending = 0;
                }
            }
            Ok(Err(error)) => return Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                for message in server.finish_pending() {
                    server.write_outgoing(&mut output, message)?;
                }
                messages_with_pending = 0;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("standard input reader stopped before exit".to_owned());
            }
        }
    }
    drop(receiver);
    drop(reader);
    Ok(())
}
