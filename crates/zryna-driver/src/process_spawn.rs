//! Exclude driver-owned fork/exec from private executable snapshot writes.
//!
//! CLOEXEC closes an inherited writer at exec, not at fork. A concurrent child can therefore
//! keep a snapshot writable after its producer closes it and make another exec fail ETXTBSY.
//! The gate covers the open-to-close writer interval and spawning through exec. Child waiting
//! happens outside it. Commands started independently by an embedding application are outside
//! this driver-owned protocol.

use std::io;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::sync::{Mutex, MutexGuard};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
static SNAPSHOT_AND_SPAWN: Mutex<()> = Mutex::new(());

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) fn snapshot_writer() -> io::Result<MutexGuard<'static, ()>> {
    lock(&SNAPSHOT_AND_SPAWN)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn lock(gate: &Mutex<()>) -> io::Result<MutexGuard<'_, ()>> {
    gate.lock().map_err(|_| io::Error::other("private snapshot/spawn gate poisoned"))
}

pub(crate) fn spawn<T>(operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    let _guard = snapshot_writer()?;
    operation()
}

#[cfg(test)]
pub(crate) fn output(command: &mut std::process::Command) -> io::Result<std::process::Output> {
    use std::process::Stdio;

    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    spawn(|| command.spawn())?.wait_with_output()
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
pub(crate) fn status(command: &mut std::process::Command) -> io::Result<std::process::ExitStatus> {
    spawn(|| command.spawn())?.wait()
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests {
    use super::*;
    use std::{process::Command, sync::TryLockError};

    #[test]
    fn snapshot_writer_excludes_child_creation_and_spawn_releases_before_waiting() {
        let writer = snapshot_writer().expect("snapshot gate");
        assert!(matches!(SNAPSHOT_AND_SPAWN.try_lock(), Err(TryLockError::WouldBlock)));
        drop(writer);
        let mut child = spawn(|| {
            assert!(matches!(SNAPSHOT_AND_SPAWN.try_lock(), Err(TryLockError::WouldBlock)));
            Command::new("/bin/sh")
                .args(["-c", "read ignored"])
                .stdin(std::process::Stdio::piped())
                .spawn()
        })
        .expect("child creation under gate");
        // The child is alive and blocked on its pipe; a new writer must already be admitted.
        let writer = snapshot_writer().expect("child waiting must not retain the gate");
        assert!(child.try_wait().expect("observe child").is_none());
        drop(writer);
        drop(child.stdin.take());
        assert!(!child.wait().expect("reap child").success());
    }

    #[test]
    fn poisoned_gate_fails_closed() {
        let gate = Mutex::new(());
        std::thread::scope(|scope| {
            assert!(
                scope
                    .spawn(|| {
                        let _guard = lock(&gate).expect("unpoisoned local gate");
                        panic!("controlled gate poison");
                    })
                    .join()
                    .is_err()
            );
        });
        assert_eq!(
            lock(&gate).expect_err("poison must not be recovered").kind(),
            io::ErrorKind::Other
        );
    }
}
