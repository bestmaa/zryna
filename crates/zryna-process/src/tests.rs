use super::*;
use std::{process::Command, sync::TryLockError};

#[test]
fn snapshot_writer_excludes_child_creation_and_spawn_releases_before_waiting() {
    let writer = snapshot_writer().expect("snapshot gate");
    assert!(matches!(SNAPSHOT_AND_SPAWN.try_lock(), Err(TryLockError::WouldBlock)));
    drop(writer);
    let mut child = spawn(|| {
        assert!(matches!(SNAPSHOT_AND_SPAWN.try_lock(), Err(TryLockError::WouldBlock)));
        Command::new("/bin/sh").args(["-c", "read ignored"]).stdin(Stdio::piped()).spawn()
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
    assert_eq!(lock(&gate).expect_err("poison must not be recovered").kind(), io::ErrorKind::Other);
}
