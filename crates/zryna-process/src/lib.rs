//! Shared exclusion between private executable snapshot writes and Zryna-owned spawning.

#![forbid(unsafe_code)]

use std::{
    io,
    process::{Command, ExitStatus, Output, Stdio},
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::sync::{Mutex, MutexGuard};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
static SNAPSHOT_AND_SPAWN: Mutex<()> = Mutex::new(());

/// Excludes Zryna-owned child creation until the private snapshot writer is closed.
///
/// # Errors
/// Returns an I/O error if a previous holder poisoned the gate.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn snapshot_writer() -> io::Result<MutexGuard<'static, ()>> {
    lock(&SNAPSHOT_AND_SPAWN)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn lock(gate: &Mutex<()>) -> io::Result<MutexGuard<'_, ()>> {
    gate.lock().map_err(|_| io::Error::other("private snapshot/spawn gate poisoned"))
}

/// Performs child creation under the shared gate, releasing it before child waiting.
///
/// The closure must perform only process creation, not wait for the resulting child.
///
/// # Errors
/// Returns the operation's error or fails closed if the shared gate is poisoned.
pub fn spawn<T>(operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64", feature = "test-observation"))]
    let pending = observation::Pending::new();
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    let _guard = snapshot_writer()?;
    #[cfg(all(target_os = "linux", target_arch = "x86_64", feature = "test-observation"))]
    drop(pending);
    operation()
}

/// Captures a command's output, with child waiting outside the shared spawn gate.
///
/// # Errors
/// Returns the spawn, gate, output-read or child-wait error without retrying.
pub fn output(command: &mut Command) -> io::Result<Output> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    spawn(|| command.spawn())?.wait_with_output()
}

/// Waits for a command's status outside the shared spawn gate.
///
/// # Errors
/// Returns the spawn, gate or child-wait error without retrying.
pub fn status(command: &mut Command) -> io::Result<ExitStatus> {
    spawn(|| command.spawn())?.wait()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64", feature = "test-observation"))]
mod observation;

/// Reports whether one exact thread has entered spawn and is waiting for the shared gate.
/// This read-only test observation does not admit spawning or release a writer.
#[cfg(all(target_os = "linux", target_arch = "x86_64", feature = "test-observation"))]
#[must_use]
pub fn spawn_pending(thread: std::thread::ThreadId) -> bool {
    observation::contains(thread)
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests;
