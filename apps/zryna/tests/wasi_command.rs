//! Actual source-checkout CLI acceptance on Linux and Windows with pinned syntax provider.
#![forbid(unsafe_code)]

#[path = "wasi_command/support.rs"]
mod support;

use std::fs;
use support::{Case, Input, guard, input_path, read_json};

#[test]
fn actual_cli_unused_helper_match_cannot_consume_the_lookup_result() {
    let _guard = guard();
    let case = Case::new();
    let input = Input::new("MODE", Some("on"));
    let output = case.run(
        "tests/wasi-command-source-fixtures/unused-environment-match-helper.zry",
        Some(&input),
        &[],
    );
    assert_eq!(
        output.status.code(),
        Some(3),
        "lookup result must actually reach an exhaustive match: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!case.bundle.exists(), "rejected source has no final execution bundle");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("ZRYNA-I4100"),
        "command consumption rejection: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    for value in [Some("on"), None] {
        let forwarded = Case::new();
        let input = Input::new("MODE", value);
        let output = forwarded.run(
            "tests/wasi-command-source-fixtures/forwarded-environment-match-helper.zry",
            Some(&input),
            &[],
        );
        assert_eq!(output.status.code(), Some(if value.is_some() { 0 } else { 5 }));
        let manifest = read_json(&forwarded.manifest());
        assert_eq!(manifest["execution"]["runReturn"], if value.is_some() { "ok" } else { "err" });
        assert_eq!(manifest["teardown"], "confirmed");
    }
}

#[test]
fn actual_cli_pure_and_declared_error_commit_distinct_complete_command_bundles() {
    let _guard = guard();
    for (source, expected, exit) in
        [("examples/wasi-command/pure.zry", "ok", 0), ("examples/wasi-command/error.zry", "err", 5)]
    {
        let case = Case::new();
        let output = case.run(source, None, &[]);
        assert_eq!(output.status.code(), Some(exit), "{}", String::from_utf8_lossy(&output.stderr));
        let response = read_json(&output.stdout);
        assert_eq!(response["ok"], exit == 0);
        assert_eq!(
            response["manifest"],
            format!(
                ".zryna/out/{}.wasi-command-run/{}",
                case.stem,
                zryna_driver::COMMAND_H1_MANIFEST_NAME
            )
        );
        let bytes = case.manifest();
        zryna_driver::decode_command_h1_manifest(&bytes).expect("closed actual execution record");
        let manifest = read_json(&bytes);
        assert_eq!(manifest["schema"], "zryna.wasi-command-manifest.v1");
        assert_eq!(manifest["execution"]["runReturn"], expected);
        assert_eq!(manifest["input"]["kind"], "none");
        assert_eq!(manifest["teardown"], "confirmed");
        assert!(case.bundle.join("wasi-command").join(format!("{}.wasm", case.stem)).is_file());
        assert!(!case.bundle.join("zryna-manifest-v3.json").exists());
        assert!(!case.bundle.join("zryna-browser-manifest-v1.json").exists());
    }
}

#[test]
fn actual_cli_found_missing_empty_and_utf8_values_are_private_and_released() {
    let _guard = guard();
    for value in [None, Some(""), Some("private-command-fixture-हिन्दी🙂"), Some("on")]
    {
        let case = Case::new();
        let input = Input::new("MODE", value);
        let output = case.run("examples/wasi-command/lookup.zry", Some(&input), &[]);
        assert_eq!(
            output.status.code(),
            Some(if value.is_some() { 0 } else { 5 }),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let bytes = case.manifest();
        zryna_driver::decode_command_h1_manifest(&bytes).expect("actual granted record");
        let manifest = read_json(&bytes);
        assert_eq!(manifest["execution"]["runReturn"], if value.is_some() { "ok" } else { "err" });
        assert_eq!(manifest["input"]["kind"], if value.is_some() { "present" } else { "missing" });
        assert_eq!(manifest["input"]["utf8ByteCount"], value.map_or(0, str::len));
        let text = String::from_utf8(bytes).expect("UTF8 manifest");
        assert!(!text.contains("private-command-fixture-"));
        assert!(!text.contains("request.json"));
        assert!(!text.contains("valueHash"));
        let original = fs::read(input_path(&input)).expect("input remains caller-owned");
        let moved = input_path(&input).with_file_name("released.json");
        fs::rename(input_path(&input), &moved).expect("CLI released original file authority");
        fs::rename(moved, input_path(&input)).expect("restore caller input");
        assert_eq!(fs::read(input_path(&input)).expect("unchanged input"), original);
    }
}

#[test]
fn actual_cli_wrong_omitted_extra_grants_and_wrong_export_reject_without_bundle() {
    let _guard = guard();
    let wrong = Input::new("OTHER", Some("on"));
    let proper = Input::new("MODE", Some("on"));
    for (source, input, extra, expected_status) in [
        ("examples/wasi-command/lookup.zry", None, Vec::new(), 3),
        ("examples/wasi-command/lookup.zry", Some(&wrong), Vec::new(), 3),
        ("examples/wasi-command/pure.zry", Some(&proper), Vec::new(), 3),
        ("examples/wasi-command/pure.zry", None, vec!["--arg=i32:1"], 2),
    ] {
        let case = Case::new();
        let output = case.run(source, input, &extra);
        assert_eq!(
            output.status.code(),
            Some(expected_status),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!case.bundle.exists(), "rejected command has no final bundle");
    }
    let case = Case::new();
    assert_eq!(
        case.run("examples/wasi-command/pure.zry", None, &["--export", "other"]).status.code(),
        Some(2)
    );
    assert!(!case.bundle.exists());
}

#[test]
fn actual_cli_create_only_retry_preserves_component_and_manifest_inventory() {
    let _guard = guard();
    let case = Case::new();
    let first = case.run("examples/wasi-command/pure.zry", None, &[]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let original = case.manifest();
    let component = case.bundle.join("wasi-command").join(format!("{}.wasm", case.stem));
    let component_before = fs::read(&component).expect("component");
    let repeated = case.run("examples/wasi-command/pure.zry", None, &[]);
    assert_eq!(repeated.status.code(), Some(4));
    assert_eq!(case.manifest(), original);
    assert_eq!(fs::read(component).expect("retained component"), component_before);
    let inventory = fs::read_dir(&case.bundle)
        .expect("bundle inventory")
        .map(|entry| entry.expect("entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(inventory.len(), 2, "only component directory and distinct manifest");
}
