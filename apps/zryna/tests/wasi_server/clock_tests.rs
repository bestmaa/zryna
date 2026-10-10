use super::support::{Case, common, exchange, frame, guard};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, net::TcpListener};

#[path = "clock_support.rs"]
mod fixture;
use fixture::{Granted, document};

#[test]
fn granted_clock_profile_executes_one_read_per_store_and_records_actual_teardown() {
    let _guard = guard();
    for status in [200, 599] {
        let granted = Granted::new(3);
        let mut running = granted.spawn(&format!("examples/wasi-server/status-{status}.zry"));
        assert!(exchange(running.address, b"invalid\r\n\r\n").is_empty());
        for _ in 0..2 {
            assert!(
                exchange(running.address, &frame(running.address, b""))
                    .starts_with(format!("HTTP/1.1 {status} ").as_bytes())
            );
        }
        let (exit, result) = running.finish();
        assert!(exit.success(), "{result}");
        let record = granted.manifest();
        assert_eq!(record["schema"], "zryna.wasi-server-manifest.v2");
        assert_eq!(record["profile"], "server-clock-status-v1");
        assert_eq!(record["operation"], "clock_read_then_status");
        assert_eq!(record["requested_guest_grants"], json!(["clock"]));
        assert_eq!(record["effective_guest_grants"], json!(["clock"]));
        assert_eq!(
            record["clock_limits"],
            json!({"monotonic_reads":1,"subscriptions":0,"timers":0})
        );
        assert_eq!(record["execution"]["served"], 2);
        assert_eq!(record["execution"]["rejected"], 1);
        assert_eq!(record["execution"]["clock_reads"], 2);
        assert_eq!(record["execution"]["denied_callbacks"], 0);
        assert_eq!(record["execution"]["teardown"]["stores_created"], 2);
        assert_eq!(record["execution"]["teardown"]["stores_destroyed"], 2);
        assert_eq!(record["execution"]["teardown"]["confirmed"], true);
        assert!(!granted.case.bundle.join("zryna-wasi-server-manifest-v1.json").exists());
        assert!(
            result["manifest"].as_str().expect("new record path").ends_with("manifest-v2.json")
        );
        assert!(!contains_path(&record, &granted.request.path.display().to_string()));
    }
}

#[test]
fn malformed_or_unapproved_guest_inputs_and_cross_profile_flags_reject_before_ready() {
    let _guard = guard();
    let valid: Value = serde_json::from_slice(&document()).expect("valid fixture");
    let mut rejected = vec![vec![b' '; 2049],
        br#"{"world":"zryna:capability-profiles/server@0.1.0","world":"zryna:capability-profiles/server@0.1.0","requests":["clock"],"clock":{"monotonic_reads":1,"subscriptions":0,"timers":0}}"#.to_vec()];
    for (field, value) in [
        ("world", json!("wrong")),
        ("requests", json!([])),
        ("requests", json!(["clock", "clock"])),
        ("requests", json!(["filesystem"])),
        ("extra", json!(true)),
    ] {
        let mut bad = valid.clone();
        bad[field] = value;
        rejected.push(serde_json::to_vec(&bad).expect("malformed request"));
    }
    for (field, value) in [("monotonic_reads", 2), ("subscriptions", 1), ("timers", 1)] {
        let mut bad = valid.clone();
        bad["clock"][field] = json!(value);
        rejected.push(serde_json::to_vec(&bad).expect("excessive request"));
    }
    for bytes in rejected {
        let granted = Granted::for_case(Case::new(1), bytes);
        assert_rejected(&granted, granted.command("examples/wasi-server/status-200.zry"));
    }
    for approval in [
        json!({"schema":"zryna.wasi-server-guest-approval.v1", "request_sha256":"wrong", "monotonic_reads":1}),
        json!({"schema":"zryna.wasi-server-guest-approval.v1", "request_sha256":"wrong", "monotonic_reads":0}),
    ] {
        let mut granted = Granted::new(1);
        granted.approval =
            common::Input::from_bytes(&serde_json::to_vec(&approval).expect("denied approval"));
        assert_rejected(&granted, granted.command("examples/wasi-server/status-200.zry"));
    }
    let granted = Granted::new(1);
    let mut pure = granted.case.command("examples/wasi-server/status-200.zry");
    pure.arg("--guest-request")
        .arg(&granted.request.path)
        .arg("--guest-approval")
        .arg(&granted.approval.path);
    assert_rejected(&granted, pure);
    let missing = granted
        .case
        .profile_command("examples/wasi-server/status-200.zry", "server-clock-status-v1");
    assert_rejected(&granted, missing);
}

fn contains_path(value: &Value, path: &str) -> bool {
    match value {
        Value::String(text) => text.contains(path),
        Value::Array(items) => items.iter().any(|item| contains_path(item, path)),
        Value::Object(fields) => fields.values().any(|item| contains_path(item, path)),
        _ => false,
    }
}

#[test]
fn clock_approval_does_not_admit_a_source_language_clock_intrinsic() {
    let _guard = guard();
    let granted = Granted::new(1);
    let (_owned, logical) = super::source_tests::source(
        &granted.case,
        "export function status(): i32 { return clock(); }",
    );
    let output = granted.command(&logical).output().expect("actual source intrinsic rejection");
    assert_eq!(output.status.code(), Some(4));
    let result: Value =
        serde_json::from_slice(&output.stdout).expect("only final preparation rejection");
    assert_eq!(result["diagnostics"][0]["code"], "ZRYNA-C4203");
    assert!(!granted.case.bundle.exists());
}

fn assert_rejected(granted: &Granted, mut command: std::process::Command) {
    let output = command.output().expect("actual guest rejection");
    assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stdout));
    let result: Value = serde_json::from_slice(&output.stdout).expect("only final rejection");
    assert_eq!(result["kind"], "result");
    assert_eq!(result["diagnostics"][0]["code"], "ZRYNA-C4201");
    assert!(!granted.case.bundle.exists());
}

#[test]
fn matching_digest_cannot_authorize_zero_reads_or_unknown_duplicate_approval_fields() {
    let _guard = guard();
    let mut granted = Granted::new(1);
    let digest = format!("{:x}", Sha256::digest(&granted.bytes));
    for approval in [
        json!({"schema":"zryna.wasi-server-guest-approval.v1", "request_sha256":digest, "monotonic_reads":0}),
        json!({"schema":"zryna.wasi-server-guest-approval.v1", "request_sha256":digest, "monotonic_reads":2}),
        json!({"schema":"wrong", "request_sha256":digest, "monotonic_reads":1}),
        json!({"schema":"zryna.wasi-server-guest-approval.v1", "request_sha256":digest, "monotonic_reads":1, "extra":true}),
    ] {
        granted.approval =
            common::Input::from_bytes(&serde_json::to_vec(&approval).expect("invalid approval"));
        assert_rejected(&granted, granted.command("examples/wasi-server/status-200.zry"));
    }
    let duplicate = format!(
        "{{\"schema\":\"zryna.wasi-server-guest-approval.v1\",\"request_sha256\":\"{digest}\",\"monotonic_reads\":1,\"monotonic_reads\":1}}"
    );
    granted.approval = common::Input::from_bytes(duplicate.as_bytes());
    assert_rejected(&granted, granted.command("examples/wasi-server/status-200.zry"));
}

#[test]
fn guest_request_and_approval_byte_ceilings_accept_exact_and_reject_first_extra() {
    let _guard = guard();
    let mut bytes = document();
    bytes.resize(2048, b' ');
    let mut granted = Granted::for_case(Case::new(1), bytes.clone());
    let mut approval = fs::read(&granted.approval.path).expect("root approval bytes");
    approval.resize(1024, b' ');
    granted.approval = common::Input::from_bytes(&approval);
    let mut running = granted.spawn("examples/wasi-server/status-200.zry");
    assert!(exchange(running.address, &frame(running.address, b"")).starts_with(b"HTTP/1.1 200 "));
    let (exit, result) = running.finish();
    assert!(exit.success(), "{result}");
    assert_eq!(granted.manifest()["execution"]["clock_reads"], 1);
    bytes.push(b' ');
    let granted = Granted::for_case(Case::new(1), bytes);
    assert_rejected(&granted, granted.command("examples/wasi-server/status-200.zry"));
    let mut granted = Granted::new(1);
    let mut approval = fs::read(&granted.approval.path).expect("original approval");
    approval.resize(1025, b' ');
    granted.approval = common::Input::from_bytes(&approval);
    assert_rejected(&granted, granted.command("examples/wasi-server/status-200.zry"));
}

#[test]
fn forced_clock_server_termination_releases_listener_and_private_input_handles() {
    let _guard = guard();
    let granted = Granted::new(64);
    let mut running = granted.spawn("examples/wasi-server/status-200.zry");
    let address = running.address;
    running.kill();
    drop(running);
    assert!(TcpListener::bind(address).is_ok());
    fs::write(&granted.request.path, b"{}").expect("captured request handles released");
    fs::write(&granted.approval.path, b"{}").expect("captured approval handles released");
    assert!(!granted.case.bundle.exists());
}

#[cfg(unix)]
#[test]
fn guest_request_replacement_and_approval_revocation_stop_service_without_record() {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let _guard = guard();
    for request in [true, false] {
        let granted = Granted::new(1);
        let mut running = granted.spawn("examples/wasi-server/status-200.zry");
        let path = if request { &granted.request.path } else { &granted.approval.path };
        fs::remove_file(path).expect("remove retained guest identity");
        if request {
            let mut replacement = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(path)
                .expect("different identity");
            replacement.write_all(&granted.bytes).expect("identical request bytes");
        }
        let (exit, result) = running.finish();
        assert_eq!(exit.code(), Some(4), "{result}");
        assert_eq!(result["diagnostics"][0]["code"], "ZRYNA-C4202");
        assert!(!granted.case.bundle.exists());
    }
}

#[cfg(windows)]
#[test]
fn retained_guest_inputs_block_writes_and_replacement_then_keep_exact_permission() {
    let _guard = guard();
    let granted = Granted::new(1);
    let mut running = granted.spawn("examples/wasi-server/status-200.zry");
    for path in [&granted.request.path, &granted.approval.path] {
        assert!(fs::write(path, b"{}").is_err());
        assert!(fs::rename(path, path.with_extension("replaced")).is_err());
    }
    assert!(exchange(running.address, &frame(running.address, b"")).starts_with(b"HTTP/1.1 200 "));
    let (exit, result) = running.finish();
    assert!(exit.success(), "{result}");
    assert_eq!(granted.manifest()["execution"]["clock_reads"], 1);
    assert_eq!(granted.manifest()["execution"]["teardown"]["confirmed"], true);
}

#[test]
fn approved_clock_without_guest_execution_records_zero_reads_on_cancel_and_deadline() {
    let _guard = guard();
    let granted = Granted::new(64);
    let request = zryna_driver::ClockServerRunRequest {
        server: granted.case.driver_request("examples/wasi-server/status-200.zry"),
        guest_request: granted.request.path.clone(),
        guest_approval: granted.approval.path.clone(),
    };
    let mut endpoint = None;
    let result = zryna_driver::serve_clock_workspace(&request, |ready| {
        endpoint = Some(ready.address());
        ready.cancel();
        Ok(())
    })
    .expect("actual clock cancellation");
    assert_eq!(result.manifest()["execution"]["outcome"], "cancelled");
    assert_eq!(result.manifest()["execution"]["clock_reads"], 0);
    assert_eq!(result.manifest()["execution"]["teardown"]["stores_created"], 0);
    assert!(TcpListener::bind(endpoint.expect("actual endpoint")).is_ok());
    let bytes = serde_json::to_vec(&json!({"listen":"127.0.0.1:0","attempts":1,
        "header_bytes":1024,"body_bytes":64,"request_ms":10,"service_ms":10}))
    .expect("short lifetime");
    let granted = Granted::for_case(Case::from_config(&bytes), document());
    let mut running = granted.spawn("examples/wasi-server/status-200.zry");
    let (exit, result) = running.finish();
    assert_eq!(exit.code(), Some(5), "{result}");
    assert_eq!(granted.manifest()["execution"]["outcome"], "service_deadline");
    assert_eq!(granted.manifest()["execution"]["clock_reads"], 0);
    assert_eq!(granted.manifest()["execution"]["teardown"]["confirmed"], true);
}
