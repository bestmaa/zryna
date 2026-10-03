//! Dedicated executable checks; resource acceptance still belongs to the external supervisor.

use std::{
    env,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(target_os = "linux")]
use std::fs;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn request(command: &mut Command, input: &[u8]) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("owned dedicated compiler process");
    child.stdin.take().expect("request pipe").write_all(input).expect("bounded request");
    child.wait_with_output().expect("owned process exit and pipe EOF")
}

fn compiler(materials: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zryna-playground-compiler"));
    command.args(["--materials-root"]).arg(materials);
    command
}

#[test]
fn malformed_request_stops_before_material_capture_on_each_host() {
    let missing = env::temp_dir().join(format!(
        ".zryna-missing-materials-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    assert!(!missing.exists(), "test root must remain absent");
    let output = request(&mut compiler(&missing), br#"{"version":2,"revision":1,"source":""}"#);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"PLAYGROUND-REQUEST-VERSION\n");
    assert!(!missing.exists());
}

#[cfg(not(target_os = "linux"))]
#[test]
fn admitted_request_keeps_unsupported_host_runtime_rejection() {
    let missing = env::temp_dir().join(".zryna-missing-materials");
    let output = request(&mut compiler(&missing), br#"{"version":1,"revision":1,"source":""}"#);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"PLAYGROUND-RUNTIME-LIMIT\n");
}

#[cfg(target_os = "linux")]
fn isolated_compiler(materials: &Path, scratch: &Path, fsize: u64) -> Command {
    // Tests must themselves run in the separately owned whole-tree envelope.
    // The fixed harness only lowers inherited limits and execs the genuine application.
    let mut command = Command::new("/usr/bin/python3");
    command.args([
        "-I",
        "-S",
        "-B",
        "-c",
        r"
import os, resource, sys
limits = [(resource.RLIMIT_CORE, 0), (resource.RLIMIT_FSIZE, int(sys.argv[3])),
          (resource.RLIMIT_NOFILE, 512)]
for kind, limit in limits:
    hard = resource.getrlimit(kind)[1]
    if hard != resource.RLIM_INFINITY and hard < limit:
        raise RuntimeError('test parent cannot supply required hard limit')
    resource.setrlimit(kind, (limit, limit))
os.execve(sys.argv[1], [sys.argv[1], '--materials-root', sys.argv[2]],
          {'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8', 'TMPDIR': sys.argv[4]})
",
    ]);
    command
        .arg(env!("CARGO_BIN_EXE_zryna-playground-compiler"))
        .arg(materials)
        .arg(fsize.to_string())
        .arg(scratch);
    command
}

#[cfg(target_os = "linux")]
#[test]
fn inherited_one_mib_hard_limit_rejects_before_material_creation() {
    let missing = env::temp_dir().join(format!(
        ".zryna-rejected-materials-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    assert!(!missing.exists());
    let output = request(
        &mut isolated_compiler(&missing, &missing, 1_048_576),
        br#"{"version":1,"revision":1,"source":""}"#,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"PLAYGROUND-RUNTIME-LIMIT\n");
    assert!(!missing.exists(), "neither material nor stage root may be created");
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires independently authenticated installed materials and owned native parent envelope"]
fn authenticated_large_provider_stages_then_compiles_and_removes_owned_stage() {
    let materials = env::var_os("ZRYNA_PLAYGROUND_TEST_MATERIALS")
        .map(std::path::PathBuf::from)
        .expect("reviewed installed material root");
    assert!(materials.is_absolute());
    let scratch = env::temp_dir().join(format!(
        ".zryna-isolated-compiler-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&scratch).expect("unique test-owned private scratch");
    let output = request(
        &mut isolated_compiler(&materials, &scratch, 268_435_456),
        br#"{"version":1,"revision":1,"source":"export function add(a: i32, b: i32): i32 { return a + b; }\n"}"#,
    );
    assert!(
        output.status.success(),
        "actual compiler failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(output.stdout.len() >= 4 && output.stdout.len() <= 1_277_956);
    let header = u32::from_le_bytes(output.stdout[..4].try_into().expect("frame length")) as usize;
    assert!(header <= 98_304 && header + 4 <= output.stdout.len());
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout[4..4 + header]).expect("actual compilation metadata");
    assert_eq!(metadata["status"], "compiled");
    assert_eq!(metadata["revision"], 1);
    assert_eq!(metadata["sourcePath"], "src/main.zry");
    assert_eq!(metadata["exports"][0]["logical"], "add");
    assert_eq!(metadata["exports"][0]["arity"], 2);
    let artifacts = metadata["artifacts"].as_array().expect("audited artifact declarations");
    assert_eq!(artifacts.len(), 3);
    let payload: u64 =
        artifacts.iter().map(|item| item["bytes"].as_u64().expect("artifact bytes")).sum();
    assert_eq!(
        usize::try_from(payload).expect("bounded frame payload fits host size") + header + 4,
        output.stdout.len()
    );
    assert_eq!(fs::read_dir(&scratch).expect("owned scratch inventory").count(), 0);
    fs::remove_dir(scratch).expect("remove exact empty test-owned scratch");
}
