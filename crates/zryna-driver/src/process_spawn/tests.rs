//! Real formerly-bypassed callers must wait while a private executable writer is live.

use std::{
    fs,
    io::Write as _,
    os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

use zryna_source::{SourceFileInput, SourceMap};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zryna-real-spawn-exclusion-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).expect("private spawn fixture");
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove this spawn fixture");
    }
}

fn while_executable_writer_is_live<T: Send + 'static>(
    operation: impl FnOnce() -> T + Send + 'static,
) -> T {
    let fixture = Fixture::new();
    let executable = fixture.0.join("snapshot.elf");
    let bytes = fs::read("/bin/true").expect("real executable fixture");
    let guard = super::snapshot_writer().expect("shared snapshot exclusion");
    let mut writer = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&executable)
        .expect("live executable writer");
    writer.write_all(&bytes).expect("write executable snapshot");
    writer.sync_all().expect("flush executable snapshot");
    let caller = thread::spawn(operation);
    let deadline = Instant::now() + Duration::from_secs(5);
    let observed = loop {
        if zryna_process::spawn_pending(caller.thread().id()) {
            break true;
        }
        if caller.is_finished() || Instant::now() >= deadline {
            break false;
        }
        thread::yield_now();
    };
    // Observe this exact caller in the actual shared spawn boundary, not a cooperative mock.
    // No child creation can occur while the writer remains open. Release in production order.
    drop(writer);
    drop(guard);
    let result = caller.join().expect("real spawn caller must finish");
    assert!(observed, "formerly-bypassed real caller did not enter shared spawn exclusion");
    assert!(
        super::status(&mut Command::new(&executable)).expect("execute closed snapshot").success()
    );
    result
}

#[test]
fn frontend_worker_real_process_creation_waits_for_live_snapshot_writer() {
    let frontend = crate::tests::typescript_frontend();
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "src/main.zry".into(),
        text: "function main(): i32 { return 7; }".into(),
    }])
    .expect("authoritative source");
    let project = while_executable_writer_is_live(move || frontend.analyze_verified(&sources))
        .expect("real worker handshake and verified source analysis");
    assert_eq!(project.files().len(), 1);
}

#[test]
fn node_integration_helper_real_process_creation_waits_for_live_snapshot_writer() {
    let output = while_executable_writer_is_live(|| {
        crate::tests::run_node_module(Path::new("unused"), "console.log('spawn-probe')", &[])
    });
    assert!(output.status.success());
    assert_eq!(output.stdout, b"spawn-probe\n");
}

#[test]
fn architecture_cargo_probe_real_process_creation_waits_for_live_snapshot_writer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let report =
        while_executable_writer_is_live(move || zryna_architecture::validate_workspace(&root));
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
}
