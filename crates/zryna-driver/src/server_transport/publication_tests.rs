use super::super::io::Socket;
use super::*;
use crate::{server_lifecycle::Input, server_runtime::Server};

#[test]
fn cancellation_after_guest_cleanup_suppresses_real_socket_publication() {
    let configuration = config(1, 1000);
    let observation = Arc::new(RuntimeObservation::default());
    let server = Server::start(prepared(configuration, "200", &observation)).expect("runtime");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test listener");
    let mut client = connect(listener.local_addr().expect("address"));
    let (stream, _) = listener.accept().expect("accepted socket");
    let transport = Arc::new(Observation::default());
    let socket = Socket::new(
        stream,
        Arc::new(Control::default()),
        Arc::clone(&transport),
        configuration.buffer_reservation(),
        Instant::now() + Duration::from_secs(1),
    )
    .expect("owned socket");
    let input = Input { method: "GET", path: "/candidate", body: b"", deadline: socket.deadline };
    let pending = server.admit(&input).expect("lease");
    until(|| pending.response_ready().expect("ready"));
    clean(&observation);
    pending.cancellation()().expect("cancel before publication commit");
    assert!(
        pending
            .publish(move |status| {
                socket.respond(status).map_err(|_| crate::server_lifecycle::Error::Host)
            })
            .is_err()
    );
    let mut bytes = Vec::new();
    let _ = client.read_to_end(&mut bytes);
    assert!(bytes.is_empty());
    assert_eq!(transport.socket_handles.load(Ordering::SeqCst), 0);
    assert_eq!(transport.reserved_bytes.load(Ordering::SeqCst), 0);
    assert_eq!(server.usage(), Ok((0, 0)));
    server.shutdown().expect("joined");
}

#[test]
fn publication_commit_holds_quota_until_callback_finishes_and_orders_cancellation() {
    let configuration = config(1, 1000);
    let observation = Arc::new(RuntimeObservation::default());
    let server = Server::start(prepared(configuration, "200", &observation)).expect("runtime");
    let input = Input {
        method: "GET",
        path: "/commit",
        body: b"",
        deadline: Instant::now() + Duration::from_secs(1),
    };
    let pending = server.admit(&input).expect("lease");
    until(|| pending.response_ready().expect("ready"));
    clean(&observation);
    assert_eq!(server.usage().expect("charged pending response").0, 1);
    let cancel = pending.cancellation();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let publisher = thread::spawn(move || {
        pending.publish(move |_| {
            entered_tx.send(()).expect("commit entered");
            release_rx.recv_timeout(Duration::from_secs(1)).expect("bounded callback cleanup");
            Ok(())
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(1)).expect("commit lock held");
    let (cancel_tx, cancel_rx) = std::sync::mpsc::channel();
    let canceller = thread::spawn(move || {
        let result = cancel();
        cancel_tx.send(result).expect("cancel result");
    });
    assert!(
        matches!(
            cancel_rx.recv_timeout(Duration::from_millis(20)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ),
        "cancellation cannot certify unfinished publication cleanup"
    );
    release_tx.send(()).expect("finish cleanup");
    assert_eq!(publisher.join().expect("publication joined"), Ok(()));
    assert_eq!(cancel_rx.recv_timeout(Duration::from_secs(1)).expect("cancel joined"), Ok(()));
    canceller.join().expect("cancel thread");
    assert_eq!(server.usage(), Ok((0, 0)));
    server.shutdown().expect("shutdown joined");
}
