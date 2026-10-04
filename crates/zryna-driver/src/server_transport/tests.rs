use super::{
    Bound, Config, Error,
    io::{Control, Observation},
    source::{self, CapturedSource},
};
use crate::server_runtime::{Observation as RuntimeObservation, Prepared};
use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    sync::{Arc, atomic::Ordering},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[path = "negative_tests.rs"]
mod negative;
#[path = "publication_tests.rs"]
mod publication;

pub(super) fn config(attempts: u16, request_ms: u64) -> Config {
    Config::parse(format!(r#"{{"listen":"127.0.0.1:0","attempts":{attempts},"header_bytes":1024,"body_bytes":32,"request_ms":{request_ms},"service_ms":3000}}"#).as_bytes())
        .expect("closed loopback configuration")
}
pub(super) fn prepared(
    config: Config,
    status: &str,
    observation: &Arc<RuntimeObservation>,
) -> Prepared {
    source::prepare_source(
        &source::tests::production_frontend(),
        CapturedSource::from_bytes(
            "status.zry",
            format!("export function status(): i32 {{ return {status}; }}").into_bytes(),
        )
        .expect("capture"),
        "status",
        config.envelope(),
        Arc::clone(observation),
    )
    .expect("production pure-source binding")
}
pub(super) fn until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !predicate() {
        assert!(Instant::now() < deadline, "bounded observation deadline");
        super::io::pause();
    }
}
pub(super) struct Running {
    worker: Option<JoinHandle<Result<(), Error>>>,
    pub(super) address: SocketAddr,
    pub(super) control: Arc<Control>,
    pub(super) observation: Arc<Observation>,
    pub(super) runtime: Arc<RuntimeObservation>,
}
impl Running {
    pub(super) fn start(config: Config, status: &str) -> Self {
        let runtime = Arc::new(RuntimeObservation::default());
        let prepared = prepared(config, status, &runtime);
        Self::from_prepared(config, prepared, runtime)
    }
    pub(super) fn from_prepared(
        config: Config,
        prepared: Prepared,
        runtime: Arc<RuntimeObservation>,
    ) -> Self {
        let observation = Arc::new(Observation::default());
        let bound = Bound::start(config, prepared, Arc::clone(&observation)).expect("bounded bind");
        let address = bound.address();
        let control = bound.control();
        Self {
            worker: Some(thread::spawn(move || bound.run())),
            address,
            control,
            observation,
            runtime,
        }
    }
    pub(super) fn finish(mut self) {
        self.worker.take().expect("worker").join().expect("server joined").expect("server cleanup");
        assert_eq!(self.observation.listeners.load(Ordering::SeqCst), 0);
        assert_eq!(self.observation.socket_handles.load(Ordering::SeqCst), 0);
        assert_eq!(self.observation.reserved_bytes.load(Ordering::SeqCst), 0);
        clean(&self.runtime);
        assert!(
            TcpStream::connect_timeout(&self.address, Duration::from_millis(100)).is_err(),
            "listener really closed"
        );
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.control.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub(super) fn clean(observation: &RuntimeObservation) {
    assert_eq!(observation.live(), (0, 0));
    assert_eq!(observation.input_copies.load(Ordering::SeqCst), 0);
    assert_eq!(observation.input_copy_bytes.load(Ordering::SeqCst), 0);
    assert_eq!(
        observation.stores_created.load(Ordering::SeqCst),
        observation.stores_destroyed.load(Ordering::SeqCst)
    );
    assert_eq!(
        observation.created.load(Ordering::SeqCst),
        observation.destroyed.load(Ordering::SeqCst)
    );
}
pub(super) fn frame(address: SocketAddr, body: &[u8]) -> Vec<u8> {
    let mut bytes = format!("POST /local HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
    bytes.extend(body);
    bytes
}
pub(super) fn connect(address: SocketAddr) -> TcpStream {
    let stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(1)).expect("loopback client");
    stream.set_read_timeout(Some(Duration::from_secs(2))).expect("client timeout");
    stream.set_write_timeout(Some(Duration::from_secs(2))).expect("client timeout");
    stream
}
pub(super) fn exchange(address: SocketAddr, bytes: &[u8]) -> Vec<u8> {
    let mut stream = connect(address);
    stream.write_all(bytes).expect("client request");
    stream.shutdown(Shutdown::Write).expect("client frame ends");
    let mut response = Vec::new();
    let result = stream.take(512).read_to_end(&mut response);
    if let Err(error) = result {
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
    }
    response
}

#[test]
fn production_source_serves_actual_loopback_http_and_exhausts_attempt_budget() {
    let running = Running::start(config(2, 1000), "199 + 3");
    let expected = b"HTTP/1.1 202 Zryna\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    for body in [b"".as_slice(), b"owned input bytes"] {
        assert_eq!(exchange(running.address, &frame(running.address, body)), expected);
    }
    until(|| running.observation.served.load(Ordering::SeqCst) == 2);
    assert_eq!(running.runtime.stores_created.load(Ordering::SeqCst), 2);
    assert_eq!(running.runtime.clock_reads.load(Ordering::SeqCst), 0);
    running.finish();
}

#[test]
fn repeated_production_starts_close_real_listeners_sockets_and_stores() {
    for _ in 0..6 {
        let running = Running::start(config(1, 1000), "200");
        assert!(
            exchange(running.address, &frame(running.address, b"repeat"))
                .starts_with(b"HTTP/1.1 200 ")
        );
        running.finish();
    }
}

#[test]
fn idle_service_and_explicit_stop_have_bounded_listener_lifetimes() {
    for stop in [false, true] {
        let mut configuration = config(1, 40);
        configuration.service_timeout = Duration::from_millis(50);
        let running = Running::start(configuration, "200");
        if stop {
            running.control.stop();
        }
        assert_eq!(running.runtime.stores_created.load(Ordering::SeqCst), 0);
        running.finish();
    }
}
