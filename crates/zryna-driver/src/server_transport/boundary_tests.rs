use super::{Running, clean, connect, until};
use crate::server_transport::Config;
use std::{
    io::{ErrorKind, Read, Write},
    net::{Shutdown, SocketAddr},
    sync::atomic::Ordering,
    time::Duration,
};

const BODY_BYTES: usize = 65536;
const HEADER_BYTES: usize = 8192;
const HEADER_FIELDS: usize = 32;
const METHOD: &str = "POST";
const PATH: &str = "/maximum";
const RESPONSE: &[u8] = b"HTTP/1.1 202 Zryna\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

fn frame(address: SocketAddr, body_bytes: usize, header_bytes: usize, fields: usize) -> Vec<u8> {
    let mut bytes = format!(
        "{METHOD} {PATH} HTTP/1.1\r\nHost: {address}\r\nContent-Length: {body_bytes}\r\nConnection: close\r\n"
    )
    .into_bytes();
    assert!(fields >= 4);
    for index in 0..fields - 4 {
        bytes.extend(format!("X-Boundary-{index}: value\r\n").as_bytes());
    }
    bytes.extend(b"X-Padding: ");
    let padding = header_bytes.checked_sub(bytes.len() + 4).expect("header has padding room");
    bytes.extend(std::iter::repeat_n(b'x', padding));
    bytes.extend(b"\r\n\r\n");
    assert_eq!(bytes.len(), header_bytes, "complete header includes final CRLFCRLF");
    assert_eq!(
        std::str::from_utf8(&bytes)
            .expect("ASCII header")
            .split("\r\n")
            .skip(1)
            .filter(|line| !line.is_empty())
            .count(),
        fields,
        "field count includes Host, Content-Length and Connection"
    );
    bytes.extend((0u8..=255).cycle().take(body_bytes));
    assert_eq!(bytes.len(), header_bytes + body_bytes);
    bytes
}

fn closed_connection(error: &std::io::Error) {
    assert!(
        matches!(
            error.kind(),
            ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe
        ),
        "expected terminal connection closure, got {error}"
    );
}

fn exchange(address: SocketAddr, bytes: &[u8], accepted: bool) {
    let mut stream = connect(address);
    stream.set_read_timeout(Some(Duration::from_secs(5))).expect("bounded read");
    stream.set_write_timeout(Some(Duration::from_secs(5))).expect("bounded write");
    match stream.write_all(bytes) {
        Ok(()) => {
            if let Err(error) = stream.shutdown(Shutdown::Write) {
                assert!(!accepted, "accepted request must half-close: {error}");
                if error.kind() != ErrorKind::NotConnected {
                    closed_connection(&error);
                }
            }
        }
        Err(error) => {
            assert!(!accepted, "accepted request must transmit completely: {error}");
            closed_connection(&error);
        }
    }
    let mut response = Vec::new();
    if let Err(error) = stream.take(512).read_to_end(&mut response) {
        assert!(!accepted, "accepted response must reach EOF: {error}");
        closed_connection(&error);
    }
    assert_eq!(response, if accepted { RESPONSE } else { b"" });
}

fn retired(running: &Running, served: usize, rejected: usize) {
    until(|| {
        running.observation.served.load(Ordering::SeqCst) == served
            && running.observation.rejected.load(Ordering::SeqCst) == rejected
    });
    assert_eq!(running.observation.accepted.load(Ordering::SeqCst), served + rejected);
    assert_eq!(running.observation.socket_handles.load(Ordering::SeqCst), 0);
    assert_eq!(running.observation.reserved_bytes.load(Ordering::SeqCst), 0);
    clean(&running.runtime);
    assert_eq!(running.runtime.stores_created.load(Ordering::SeqCst), served);
    assert_eq!(running.runtime.stores_destroyed.load(Ordering::SeqCst), served);
    assert_eq!(running.runtime.created.load(Ordering::SeqCst), served * 5);
    assert_eq!(running.runtime.destroyed.load(Ordering::SeqCst), served * 5);
    assert_eq!(
        running.runtime.incoming_bytes.load(Ordering::SeqCst),
        served * (METHOD.len() + PATH.len() + BODY_BYTES)
    );
    assert_eq!(running.runtime.engines_constructed.load(Ordering::SeqCst), 1);
    assert_eq!(running.runtime.clock_reads.load(Ordering::SeqCst), 0);
    assert_eq!(running.runtime.denials.load(Ordering::SeqCst), 0);
}

fn boundary(body_bytes: usize, header_bytes: usize, fields: usize) {
    let config = Config::parse(
        br#"{"listen":"127.0.0.1:0","attempts":3,"header_bytes":8192,"body_bytes":65536,"request_ms":5000,"service_ms":30000}"#,
    )
    .expect("existing maximum limits");
    let running = Running::start(config, "199 + 3");
    let maximum = frame(running.address, BODY_BYTES, HEADER_BYTES, HEADER_FIELDS);
    exchange(running.address, &maximum, true);
    retired(&running, 1, 0);
    let extra = frame(running.address, body_bytes, header_bytes, fields);
    exchange(running.address, &extra, false);
    retired(&running, 1, 1);
    exchange(running.address, &maximum, true);
    retired(&running, 2, 1);
    running.finish();
}

#[test]
fn maximum_body_and_first_extra_reject_then_recover_with_real_store_cleanup() {
    boundary(BODY_BYTES + 1, HEADER_BYTES, HEADER_FIELDS);
}

#[test]
fn maximum_complete_header_and_first_extra_reject_then_recover_with_real_store_cleanup() {
    boundary(BODY_BYTES, HEADER_BYTES + 1, HEADER_FIELDS);
}

#[test]
fn maximum_header_fields_and_first_extra_reject_then_recover_with_real_store_cleanup() {
    boundary(BODY_BYTES, HEADER_BYTES, HEADER_FIELDS + 1);
}
