//! Real source-checkout CLI, private root approval and actual loopback teardown evidence.
#![forbid(unsafe_code)]
#[path = "wasi_server/support.rs"]
mod support;
use serde_json::{Value, json};
use std::{fs, net::TcpListener};
use support::{Case, exchange, frame, guard};

#[path = "wasi_server/source_tests.rs"]
mod source_tests;

#[test]
fn actual_server_status_boundaries_malformed_attempts_and_repeated_start() {
    let _guard = guard();
    for status in [200, 599, 200] {
        let case = Case::new(3);
        let mut running = case.spawn(&format!("examples/wasi-server/status-{status}.zry"));
        assert!(running.address.ip().is_loopback());
        assert_ne!(running.address.port(), 0);
        assert!(exchange(running.address, b"invalid\r\n\r\n").is_empty());
        for body in [b"".as_slice(), b"input".as_slice()] {
            let response = exchange(running.address, &frame(running.address, body));
            assert_eq!(
                response,
                format!(
                    "HTTP/1.1 {status} Zryna\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .as_bytes()
            );
        }
        let (exit, result) = running.finish();
        assert!(exit.success(), "{result}");
        let record = case.manifest();
        assert_eq!(record["schema"], "zryna.wasi-server-manifest.v1");
        assert_eq!(record["requested_guest_grants"], json!([]));
        assert_eq!(record["effective_guest_grants"], json!([]));
        assert_eq!(record["execution"]["served"], 2);
        assert_eq!(record["execution"]["rejected"], 1);
        assert_eq!(record["execution"]["teardown"]["confirmed"], true);
        assert_eq!(
            record["execution"]["teardown"]["stores_created"],
            record["execution"]["teardown"]["stores_destroyed"]
        );
        assert_eq!(
            record["execution"]["teardown"]["resources_created"],
            record["execution"]["teardown"]["resources_destroyed"]
        );
        assert!(case.bundle.join("component").join(format!("{}.wasm", case.stem)).is_file());
        assert!(!case.bundle.join("zryna-wasi-command-manifest-v1.json").exists());
        let original =
            fs::read(case.bundle.join("zryna-wasi-server-manifest-v1.json")).expect("record");
        let output =
            case.command("examples/wasi-server/status-200.zry").output().expect("same stem");
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("\"kind\":\"ready\""));
        assert_eq!(
            fs::read(case.bundle.join("zryna-wasi-server-manifest-v1.json"))
                .expect("retained record"),
            original
        );
    }
}

#[test]
fn rejected_configuration_and_unapproved_listener_never_start_or_publish() {
    let _guard = guard();
    for config in [b"{}".to_vec(),vec![b' ';1025],
        br#"{"listen":"0.0.0.0:0","attempts":1,"header_bytes":1024,"body_bytes":64,"request_ms":1000,"service_ms":1000}"#.to_vec(),
        br#"{"listen":"127.0.0.1:0","attempts":65,"header_bytes":1024,"body_bytes":64,"request_ms":1000,"service_ms":1000}"#.to_vec()] {
        let case=Case::from_config(&config);let output=case.command("examples/wasi-server/status-200.zry").output().expect("reject config");
        assert_eq!(output.status.code(),Some(2));assert!(!case.bundle.exists());
        let response:Value=serde_json::from_slice(&output.stdout).expect("only final rejection");assert_eq!(response["diagnostics"][0]["code"],"ZRYNA-C4201");
    }
    let mut case = Case::new(1);
    case.approval=support::common::Input::from_bytes(br#"{"schema":"zryna.wasi-server-listener-approval.v1","configuration_sha256":"wrong","allow_listen":true}"#);
    let output =
        case.command("examples/wasi-server/status-200.zry").output().expect("reject approval");
    assert_eq!(output.status.code(), Some(2));
    assert!(!case.bundle.exists());
}

#[cfg(unix)]
#[test]
fn same_value_configuration_replacement_and_listener_revocation_suppress_record() {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let _guard = guard();
    for configuration in [true, false] {
        let case = Case::new(1);
        let mut running = case.spawn("examples/wasi-server/status-200.zry");
        let path = if configuration { &case.configuration.path } else { &case.approval.path };
        let original = fs::read(path).expect("original private input bytes");
        fs::remove_file(path).expect("replace retained identity");
        if configuration {
            let mut replacement = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(path)
                .expect("replacement");
            replacement.write_all(&original).expect("same bytes, different identity");
        }
        let (exit, result) = running.finish();
        assert_eq!(exit.code(), Some(4), "{result}");
        assert_eq!(result["diagnostics"][0]["code"], "ZRYNA-C4202");
        assert!(!case.bundle.exists());
    }
}

#[test]
fn forced_cli_termination_closes_listener_without_graceful_record() {
    let _guard = guard();
    let case = Case::new(64);
    let mut running = case.spawn("examples/wasi-server/status-200.zry");
    let address = running.address;
    running.kill();
    drop(running);
    assert!(TcpListener::bind(address).is_ok());
    assert!(!case.bundle.exists());
}

#[test]
fn exact_body_and_header_limits_accept_and_first_extra_requests_are_rejected() {
    let _guard = guard();
    let case = Case::new(2);
    let mut running = case.spawn("examples/wasi-server/status-200.zry");
    assert!(
        exchange(running.address, &frame(running.address, &[b'x'; 64]))
            .starts_with(b"HTTP/1.1 200 ")
    );
    assert!(exchange(running.address, &frame(running.address, &[b'x'; 65])).is_empty());
    let (exit, result) = running.finish();
    assert!(exit.success(), "{result}");
    assert_eq!(case.manifest()["execution"]["served"], 1);
    assert_eq!(case.manifest()["execution"]["rejected"], 1);
    let case = Case::new(2);
    let mut running = case.spawn("examples/wasi-server/status-200.zry");
    for length in [1024, 1025] {
        let mut bytes = format!(
            "POST /local HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\nConnection: close\r\nX-Pad: ",
            running.address
        )
        .into_bytes();
        bytes.resize(length - 4, b'x');
        bytes.extend(b"\r\n\r\n");
        let response = exchange(running.address, &bytes);
        if length == 1024 {
            assert!(response.starts_with(b"HTTP/1.1 200 "));
        } else {
            assert!(response.is_empty());
        }
    }
    let (exit, result) = running.finish();
    assert!(exit.success(), "{result}");
    assert_eq!(case.manifest()["execution"]["served"], 1);
    assert_eq!(case.manifest()["execution"]["rejected"], 1);
}

#[test]
fn readiness_cancellation_closes_actual_listener_and_commits_typed_teardown() {
    let _guard = guard();
    let case = Case::new(64);
    let request = zryna_driver::ServerRunRequest {
        workspace_root: case.root.clone(),
        entrypoint: "examples/wasi-server/status-200.zry".into(),
        export: "status".into(),
        artifact_stem: case.stem.clone(),
        node_runtime: support::common::node(),
        configuration: case.configuration.path.clone(),
        listener_approval: case.approval.path.clone(),
    };
    let mut endpoint = None;
    let result = zryna_driver::serve_workspace(&request, |ready| {
        endpoint = Some(ready.address());
        ready.cancel();
        Ok(())
    })
    .expect("actual public cancellation");
    assert_eq!(result.manifest()["execution"]["outcome"], "cancelled");
    assert_eq!(result.manifest()["execution"]["teardown"]["confirmed"], true);
    assert_eq!(result.manifest()["execution"]["accepted"], 0);
    assert!(TcpListener::bind(endpoint.expect("actual readiness")).is_ok());
}

#[cfg(unix)]
#[test]
fn dangling_final_bundle_link_is_rejected_before_listener_readiness() {
    let _guard = guard();
    let case = Case::new(1);
    fs::create_dir_all(case.bundle.parent().expect("output parent"))
        .expect("declared output directory");
    std::os::unix::fs::symlink(case.root.join(".zryna/absent-server-target"), &case.bundle)
        .expect("test-owned dangling link");
    let output = case
        .command("examples/wasi-server/status-200.zry")
        .output()
        .expect("existing output refusal");
    fs::remove_file(&case.bundle).expect("remove only test-owned link");
    assert_eq!(output.status.code(), Some(6));
    let response: Value =
        serde_json::from_slice(&output.stdout).expect("single rejection, no readiness");
    assert_eq!(response["diagnostics"][0]["code"], "ZRYNA-C4205");
}
