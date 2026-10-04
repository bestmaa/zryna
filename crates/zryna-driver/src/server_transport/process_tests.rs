//! Real child-process/client corpus. OS kill evidence does not certify guest destructors.

use super::{
    cli,
    io::Observation,
    source,
    tests::{connect, exchange, frame},
};
use crate::server_runtime::Observation as RuntimeObservation;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::SocketAddr,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

const CHILD_MARKER: &str = "ZRYNA_PRIVATE_SERVER_CHILD";
const CHILD_TEST: &str = "server_transport::process_tests::private_fixture_entry";

fn input(configuration: &str, source: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "selector":"server-loopback-private-v1", "configuration":configuration,
        "source":source, "export":"status",
    }))
    .expect("fixed private process input")
}
fn configuration(attempts: u16) -> String {
    format!(
        r#"{{"listen":"127.0.0.1:0","attempts":{attempts},"header_bytes":1024,"body_bytes":32,"request_ms":1000,"service_ms":3000}}"#
    )
}

#[test]
fn private_fixture_entry() {
    if std::env::var_os(CHILD_MARKER).is_none() {
        let valid = input(&configuration(1), "export function status(): i32 { return 200; }");
        assert!(cli::Command::parse(&valid).is_ok());
        let public_selector = String::from_utf8(valid.clone())
            .expect("JSON")
            .replace("server-loopback-private-v1", "server");
        assert!(cli::Command::parse(public_selector.as_bytes()).is_err());
        assert!(
            cli::Command::parse(&vec![
                b'x';
                usize::try_from(cli::MAX_INPUT)
                    .expect("bounded fixture input")
                    + 1
            ])
            .is_err()
        );
        assert!(cli::Command::parse(b"{}").is_err());
        return;
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(cli::MAX_INPUT + 1)
        .read_to_end(&mut bytes)
        .expect("bounded stdin capture");
    let command = cli::Command::parse(&bytes).expect("private process command admission");
    drop(bytes);
    let transport = Arc::new(Observation::default());
    let runtime = Arc::new(RuntimeObservation::default());
    let bound = command
        .start(&source::tests::production_frontend(), Arc::clone(&transport), Arc::clone(&runtime))
        .expect("production source preparation before listener");
    println!("ZRYNA_PRIVATE_READY {}", bound.address());
    std::io::stdout().flush().expect("ready receipt flush");
    bound.run().expect("joined server shutdown");
    let receipt = serde_json::json!({
        "listeners":transport.listeners.load(Ordering::SeqCst),
        "socket_handles":transport.socket_handles.load(Ordering::SeqCst),
        "reserved_bytes":transport.reserved_bytes.load(Ordering::SeqCst),
        "served":transport.served.load(Ordering::SeqCst),
        "stores":runtime.stores.load(Ordering::SeqCst),
        "resources":runtime.resources.load(Ordering::SeqCst),
        "stores_created":runtime.stores_created.load(Ordering::SeqCst),
        "stores_destroyed":runtime.stores_destroyed.load(Ordering::SeqCst),
        "input_copies":runtime.input_copies.load(Ordering::SeqCst),
        "clock_reads":runtime.clock_reads.load(Ordering::SeqCst),
    });
    println!("ZRYNA_PRIVATE_DONE {receipt}");
}

struct Process {
    child: super::process_fixture::Child,
    output: Option<BufReader<Box<dyn Read + Send>>>,
    readiness_worker: Option<std::thread::JoinHandle<()>>,
    started: Instant,
    ready_seen: bool,
}
impl Process {
    fn start(bytes: &[u8]) -> Self {
        let spawned =
            super::process_fixture::spawn(&["--exact", CHILD_TEST, "--nocapture"], CHILD_MARKER)
                .expect("contained private CLI process");
        let child = spawned.child;
        let mut input = spawned.input;
        input.write_all(bytes).expect("command bytes");
        drop(input);
        let output = BufReader::new(spawned.output);
        Self {
            child,
            output: Some(output),
            readiness_worker: None,
            started: Instant::now(),
            ready_seen: false,
        }
    }
    fn ready(&mut self) -> SocketAddr {
        let mut output = self.output.take().expect("stdout reader");
        let (sender, receiver) = std::sync::mpsc::channel();
        self.readiness_worker = Some(std::thread::spawn(move || {
            let result = (|| {
                loop {
                    let mut line = String::new();
                    let count = Read::take(&mut output, 4097)
                        .read_line(&mut line)
                        .map_err(|error| error.to_string())?;
                    if count == 0 || count > 4096 {
                        return Err("invalid child readiness output".to_owned());
                    }
                    if let Some((_, address)) = line.split_once("ZRYNA_PRIVATE_READY ") {
                        let address = address
                            .trim()
                            .parse::<SocketAddr>()
                            .map_err(|error| error.to_string())?;
                        return Ok((address, output));
                    }
                }
            })();
            let _ = sender.send(result);
        }));
        let (address, output) = receiver
            .recv_timeout(Duration::from_secs(40))
            .expect("bounded readiness deadline")
            .expect("real child readiness");
        self.output = Some(output);
        self.ready_seen = true;
        self.readiness_worker.take().expect("readiness reader").join().expect("reader joined");
        address
    }
    fn exit(&mut self) -> std::process::ExitStatus {
        let deadline = self.started + Duration::from_secs(40);
        loop {
            if let Some(status) = self.child.try_wait().expect("child status") {
                return status;
            }
            assert!(Instant::now() < deadline, "child exceeded bounded process deadline");
            super::io::pause();
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // The Unix production frontend owns a separate process group. Before readiness, allow
        // its30s session +2s cleanup budget to finish rather than orphaning it by killing the
        // fixture's outer group. READY is emitted only after normal frontend cleanup completes.
        let cleanup_deadline = self.started + Duration::from_secs(40);
        while !self.ready_seen && Instant::now() < cleanup_deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                break;
            }
            super::io::pause();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(worker) = self.readiness_worker.take() {
            let _ = worker.join();
        }
    }
}

#[test]
fn fixed_private_cli_and_client_corpus_serves_then_reports_real_cleanup() {
    let mut child = Process::start(&input(
        include_str!("corpus/config.json"),
        include_str!("corpus/status.zry"),
    ));
    let address = child.ready();
    for body in [b"".as_slice(), b"corpus body"] {
        assert_eq!(
            exchange(address, &frame(address, body)),
            b"HTTP/1.1 201 Zryna\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
    }
    assert!(child.exit().success());
    let mut output = String::new();
    child.output.as_mut().expect("stdout").read_to_string(&mut output).expect("cleanup receipt");
    let receipt = output
        .lines()
        .find_map(|line| line.split_once("ZRYNA_PRIVATE_DONE ").map(|(_, json)| json))
        .expect("actual child cleanup receipt");
    let receipt: serde_json::Value = serde_json::from_str(receipt).expect("receipt JSON");
    for key in [
        "listeners",
        "socket_handles",
        "reserved_bytes",
        "stores",
        "resources",
        "input_copies",
        "clock_reads",
    ] {
        assert_eq!(receipt[key], 0, "{key}");
    }
    assert_eq!(receipt["served"], 2);
    assert_eq!(receipt["stores_created"], 2);
    assert_eq!(receipt["stores_destroyed"], 2);
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
}

#[test]
fn private_process_rejects_nonloopback_and_forbidden_source_before_readiness() {
    for (configuration, source) in [
        (
            configuration(1).replace("127.0.0.1", "0.0.0.0"),
            "export function status(): i32 { return 200; }",
        ),
        (configuration(1), "export function status(): i32 { return Math.random(); }"),
    ] {
        let mut child = Process::start(&input(&configuration, source));
        assert!(!child.exit().success());
        let mut output = String::new();
        child
            .output
            .as_mut()
            .expect("stdout")
            .read_to_string(&mut output)
            .expect("rejection output");
        assert!(!output.contains("ZRYNA_PRIVATE_READY"));
    }
}

#[test]
fn forced_process_termination_closes_os_listener_and_partial_client_connection() {
    let mut child =
        Process::start(&input(&configuration(1), "export function status(): i32 { return 200; }"));
    let address = child.ready();
    let mut client = connect(address);
    client.write_all(b"P").expect("partial frame holds host socket");
    child.child.kill().expect("terminate only this task-owned fixture process");
    assert!(!child.exit().success());
    let mut response = Vec::new();
    if let Err(error) = client.read_to_end(&mut response) {
        assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ),
            "OS kill must close the connection, not time out: {error}"
        );
    }
    assert!(response.is_empty());
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    // The OS reclaims process-owned descriptors and threads; guest Drop was not observed here.
}
