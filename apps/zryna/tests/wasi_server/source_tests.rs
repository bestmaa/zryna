use super::{Case, guard};
use std::{fs, io::Write as _};

pub(super) struct OwnedSource {
    path: std::path::PathBuf,
}
impl Drop for OwnedSource {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn source(case: &Case, text: &str) -> (OwnedSource, String) {
    let logical = format!("examples/wasi-server/{}.zry", case.stem);
    let path = case.root.join(&logical);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .expect("unique test-owned source");
    file.write_all(text.as_bytes()).expect("source bytes");
    (OwnedSource { path }, logical)
}

#[test]
fn actual_source_effects_wrong_signatures_extra_exports_and_dependencies_reject_before_ready() {
    let _guard = guard();
    for text in [
        "export function status(): i32 { return clock(); }",
        "export function status(value: i32): i32 { return value; }",
        "export function status(): i32 { return 200; } export function other(): i32 { return 201; }",
        "import { other } from './other.zry'; export function status(): i32 { return other(); }",
    ] {
        let case = Case::new(1);
        let (_source, logical) = source(&case, text);
        let output = case.command(&logical).output().expect("real unsupported source admission");
        assert_eq!(output.status.code(), Some(4), "{}", String::from_utf8_lossy(&output.stdout));
        let response: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("only final source rejection");
        assert_eq!(response["diagnostics"][0]["code"], "ZRYNA-C4203");
        assert!(!case.bundle.exists());
    }
}

#[cfg(unix)]
#[test]
fn source_replacement_after_readiness_suppresses_response_and_final_record() {
    let _guard = guard();
    let case = Case::new(1);
    let (owned, logical) = source(&case, "export function status(): i32 { return 200; }");
    let mut running = case.spawn(&logical);
    fs::write(&owned.path, "export function status(): i32 { return 599; }")
        .expect("change original source");
    let (exit, result) = running.finish();
    assert_eq!(exit.code(), Some(4), "{result}");
    assert_eq!(result["diagnostics"][0]["code"], "ZRYNA-C4202");
    assert!(!case.bundle.exists());
}

#[cfg(windows)]
#[test]
fn retained_source_blocks_write_and_replacement_then_serves_original_status() {
    let _guard = guard();
    let case = Case::new(1);
    let (owned, logical) = source(&case, "export function status(): i32 { return 200; }");
    let mut running = case.spawn(&logical);
    assert!(fs::write(&owned.path, "export function status(): i32 { return 599; }").is_err());
    assert!(fs::rename(&owned.path, owned.path.with_extension("replaced")).is_err());
    let response = super::exchange(running.address, &super::frame(running.address, b""));
    assert!(response.starts_with(b"HTTP/1.1 200 "));
    let (exit, result) = running.finish();
    assert!(exit.success(), "{result}");
    assert_eq!(case.manifest()["execution"]["served"], 1);
    assert_eq!(case.manifest()["execution"]["teardown"]["confirmed"], true);
}
