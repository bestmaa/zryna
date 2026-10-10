use super::*;

#[test]
fn strict_configuration_rejects_unknown_duplicate_unbounded_and_nonloopback_values() {
    let valid = r#"{"listen":"127.0.0.1:0","attempts":1,"header_bytes":1024,"body_bytes":32,"request_ms":100,"service_ms":3000}"#;
    for invalid in [
        valid.replace("127.0.0.1", "0.0.0.0"),
        valid.replace("127.0.0.1", "localhost"),
        valid.replace("127.0.0.1", "192.0.2.1"),
        valid.replace("1,\"header", "65,\"header"),
        valid.replace("1024", "8193"),
        valid.replace("32", "65537"),
        valid.replace("100,", "0,"),
        valid.replace("3000", "30001"),
        valid.replace("3000", "99"),
        valid.replace("\"attempts\":1", "\"attempts\":1,\"attempts\":1"),
        valid.replace("\"attempts\":1", "\"attempts\":1,\"grants\":[\"network\"]"),
        valid.replace("\"attempts\":1", "\"attempts\":-1"),
    ] {
        assert_eq!(Config::parse(invalid.as_bytes()).err(), Some(Error::Config), "{invalid}");
    }
    let configuration = config(1, 100);
    let runtime = Arc::new(RuntimeObservation::default());
    let prepared = prepared(configuration, "200", &runtime);
    let mut forged = configuration;
    forged.address = "0.0.0.0:0".parse().expect("forged address");
    let observation = Arc::new(Observation::default());
    assert!(matches!(Bound::start(forged, prepared, Arc::clone(&observation)), Err(Error::Config)));
    assert_eq!(observation.listeners.load(Ordering::SeqCst), 0);
    clean(&runtime);
}

#[test]
fn malformed_wire_frames_never_construct_a_store_and_a_following_frame_recovers() {
    let running = Running::start(config(16, 500), "200");
    let host = running.address;
    let prefix = format!("POST /local HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    let cases = [
        format!("{prefix}Content-Length: 0\r\nContent-Length: 0\r\n\r\n"),
        format!("{prefix}Content-Length: 0\r\ncOnTeNt-LeNgTh: 1\r\n\r\nx"),
        format!("{prefix}Content-Length: 0\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"),
        format!("{prefix}Content-Length: 0\r\nUpgrade: websocket\r\n\r\n"),
        format!("{prefix}Content-Length: 0\r\nExpect: 100-continue\r\n\r\n"),
        format!("{prefix}Content-Length: 33\r\n\r\n"),
        format!("{prefix}Content-Length: 2\r\n\r\nx"),
        format!("{prefix}Content-Length: +0\r\n\r\n"),
        format!("{prefix}Content-Length: 00\r\n\r\n"),
        format!("{prefix}Content-Length: 0\r\n Folded: no\r\n\r\n"),
        format!("{prefix}Content-Length: 0\r\nX: {}\r\n\r\n", "x".repeat(1024)),
        format!("{prefix}Content-Length: 0\r\n\r\nGET /next HTTP/1.1\r\n\r\n"),
        format!("{prefix}Content-Length: 0\r\n\r\n").replace("\r\n", "\n"),
        format!("{prefix}Content-Length: 0\r\n\r\n").replace("/local", "/bad#fragment"),
        format!("{prefix}Content-Length: 0\r\n\r\n").replace(&host.to_string(), "evil.example"),
    ];
    for (index, bytes) in cases.iter().enumerate() {
        assert!(exchange(host, bytes.as_bytes()).is_empty(), "rejected wire frame {index}");
        until(|| running.observation.rejected.load(Ordering::SeqCst) == index + 1);
        assert_eq!(running.runtime.stores_created.load(Ordering::SeqCst), 0);
        assert_eq!(running.observation.socket_handles.load(Ordering::SeqCst), 0);
        assert_eq!(running.observation.reserved_bytes.load(Ordering::SeqCst), 0);
    }
    assert!(exchange(host, &frame(host, b"recover")).starts_with(b"HTTP/1.1 200 "));
    running.finish();
}

#[test]
fn absolute_deadline_and_stop_reclaim_partial_request_sockets_without_guest_state() {
    for stop in [false, true] {
        let running = Running::start(config(1, 80), "200");
        let mut client = connect(running.address);
        client.write_all(b"P").expect("partial header");
        until(|| running.observation.socket_handles.load(Ordering::SeqCst) == 2);
        if stop {
            running.control.stop();
        }
        let started = Instant::now();
        let mut response = Vec::new();
        let _ = client.read_to_end(&mut response);
        assert!(response.is_empty());
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(running.runtime.stores_created.load(Ordering::SeqCst), 0);
        running.finish();
    }
}

#[test]
fn stop_interrupts_an_actual_executing_store_and_closes_the_client_socket() {
    let configuration = config(1, 2000);
    let runtime = Arc::new(RuntimeObservation::default());
    let mut prepared = prepared(configuration, "200", &runtime);
    prepared.runtime_probe(false).expect("test-only CPU probe isolates epoch cancellation");
    let running = Running::from_prepared(configuration, prepared, runtime);
    let mut client = connect(running.address);
    client.write_all(&frame(running.address, b"cpu")).expect("CPU request");
    until(|| running.runtime.stores.load(Ordering::SeqCst) == 1);
    running.control.stop();
    until(|| running.runtime.stores_destroyed.load(Ordering::SeqCst) == 1);
    let mut response = Vec::new();
    let _ = client.read_to_end(&mut response);
    assert!(response.is_empty());
    assert_eq!(running.runtime.stores_destroyed.load(Ordering::SeqCst), 1);
    running.finish();
}

#[test]
fn guest_traps_reclaim_transport_and_lifecycle_slots_before_next_attempt() {
    let running = Running::start(config(2, 1000), "600");
    for _ in 0..2 {
        assert!(exchange(running.address, &frame(running.address, b"trap")).is_empty());
    }
    until(|| running.observation.rejected.load(Ordering::SeqCst) == 2);
    assert_eq!(running.runtime.stores_created.load(Ordering::SeqCst), 2);
    running.finish();
}

#[test]
fn incomplete_body_and_dripping_header_do_not_extend_the_absolute_deadline() {
    for body in [false, true] {
        let running = Running::start(config(1, 80), "200");
        let mut client = connect(running.address);
        let started = Instant::now();
        if body {
            client.write_all(format!("POST /local HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: 2\r\n\r\nx", running.address).as_bytes()).expect("incomplete body");
        } else {
            client.write_all(b"G").expect("initial partial header");
            for byte in b"ET /local " {
                if running.observation.rejected.load(Ordering::SeqCst) != 0 {
                    break;
                }
                thread::park_timeout(Duration::from_millis(15));
                if client.write_all(&[*byte]).is_err() {
                    break;
                }
            }
        }
        let mut response = Vec::new();
        if let Err(error) = client.read_to_end(&mut response) {
            assert!(matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ));
        }
        assert!(response.is_empty());
        assert!(started.elapsed() < Duration::from_millis(500));
        assert_eq!(running.runtime.stores_created.load(Ordering::SeqCst), 0);
        running.finish();
    }
}

#[test]
fn private_clock_denial_never_reads_a_provider_and_closes_real_transport() {
    use crate::server_runtime::{Approval, Preparation};
    use zryna_backend_webassembly::ServerOperation;
    use zryna_source::{SourceFileInput, SourceMap};
    let configuration = config(1, 1000);
    let runtime = Arc::new(RuntimeObservation::default());
    let source = SourceMap::build(vec![SourceFileInput {
        path: "status.zry".into(),
        text: "export function status(): i32 { return 200; }".into(),
    }])
    .expect("source");
    // Existing fixed private policy probe, not accepted source/interface grant integration.
    let prepared = Prepared::new(
        &source::tests::production_frontend(),
        source,
        Preparation {
            export: "status",
            operation: ServerOperation::ClockRead,
            document: br#"{"world":"zryna:capability-profiles/server@0.1.0","requests":[]}"#,
            approval: Approval::deny_all(),
            envelope: configuration.envelope(),
        },
        Arc::clone(&runtime),
    )
    .expect("denial probe");
    let running = Running::from_prepared(configuration, prepared, runtime);
    assert!(exchange(running.address, &frame(running.address, b"denied")).is_empty());
    until(|| running.observation.rejected.load(Ordering::SeqCst) == 1);
    assert_eq!(running.runtime.clock_reads.load(Ordering::SeqCst), 0);
    assert_eq!(running.runtime.denials.load(Ordering::SeqCst), 1);
    running.finish();
}
