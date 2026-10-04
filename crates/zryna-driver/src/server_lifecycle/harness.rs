//! Internal lifecycle negative-test harness; no WIT/component execution proof is claimed.

mod deadline_tests;
#[path = "mod.rs"]
mod server_lifecycle;

use server_lifecycle::{Error, Input, Limits, Server};
use std::{
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

fn limits() -> Limits {
    Limits {
        requests: 2,
        request_bytes: 8,
        response_bytes: 8,
        buffer_bytes: 128,
        timeout: Duration::from_secs(5),
    }
}

fn input(body: &[u8]) -> Input<'_> {
    Input {
        method: "POST",
        path: "/local",
        body,
        deadline: Instant::now() + Duration::from_secs(4),
    }
}

struct Resource(Arc<AtomicUsize>);
impl Drop for Resource {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn attach(request: &server_lifecycle::Request) -> Arc<AtomicUsize> {
    let destroyed = Arc::new(AtomicUsize::new(0));
    request.retain(Resource(Arc::clone(&destroyed))).expect("retain resource");
    destroyed
}

#[test]
fn bounded_input_response_and_cleanup() {
    let server = Server::start(limits()).expect("start");
    let request = server.admit(&input(b"12345678")).expect("exact input limit");
    assert_eq!(request.route().expect("route"), ("POST".into(), "/local".into()));
    let mut body = [0; 8];
    assert_eq!(request.copy_input(&mut body), Ok(8));
    assert_eq!(&body, b"12345678");
    assert_eq!(request.copy_input(&mut [0; 7]), Err(Error::Limit));
    let destroyed = attach(&request);
    assert_eq!(server.usage(), Ok((1, 26)));
    assert_eq!(request.finish(200, b"abcdefgh"), Ok(b"abcdefgh".to_vec()));
    assert_eq!(destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(server.usage(), Ok((0, 0)));
    server.shutdown().expect("worker joined");
}

#[test]
fn malformed_request_matrix_retains_nothing_and_recovers() {
    let server = Server::start(limits()).expect("start");
    for (method, path) in [
        ("", "/"),
        ("DELETE", "/"),
        ("post", "/"),
        ("GET", ""),
        ("GET", "relative"),
        ("GET", "/bad\r\n"),
        ("GET", "/bad path"),
        ("GET", "/#fragment"),
        ("GET", "/é"),
    ] {
        let mut bad = input(b"");
        bad.method = method;
        bad.path = path;
        assert!(matches!(server.admit(&bad), Err(Error::Malformed)), "{method:?} {path:?}");
        assert_eq!(server.usage(), Ok((0, 0)));
    }
    let path = format!("/{}", "a".repeat(256));
    let mut bad = input(b"");
    bad.path = &path;
    assert!(matches!(server.admit(&bad), Err(Error::Malformed)));
    assert!(matches!(server.admit(&input(b"123456789")), Err(Error::Limit)));
    let mut expired = input(b"");
    expired.deadline = Instant::now();
    assert!(matches!(server.admit(&expired), Err(Error::Deadline)));
    let mut far = input(b"");
    far.deadline = Instant::now() + Duration::from_secs(6);
    assert!(matches!(server.admit(&far), Err(Error::Limit)));
    server.admit(&input(b"ok")).expect("recovery").finish(200, b"ok").expect("response");
    assert_eq!(server.usage(), Ok((0, 0)));
}

#[test]
fn closed_configuration_rejects_zero_first_extra_and_overflow() {
    let base = limits();
    let invalid = [
        Limits { requests: 0, ..base },
        Limits { requests: 65, ..base },
        Limits { request_bytes: 0, ..base },
        Limits { request_bytes: 65_537, ..base },
        Limits { response_bytes: 0, ..base },
        Limits { response_bytes: 65_537, ..base },
        Limits { buffer_bytes: 0, ..base },
        Limits { buffer_bytes: 8 * 1024 * 1024 + 1, ..base },
        Limits { requests: usize::MAX, ..base },
        Limits { buffer_bytes: usize::MAX, ..base },
        Limits { timeout: Duration::ZERO, ..base },
        Limits { timeout: Duration::from_secs(5) + Duration::from_nanos(1), ..base },
    ];
    for config in invalid {
        assert!(matches!(Server::start(config), Err(Error::Limit)));
    }
    Server::start(Limits {
        requests: 64,
        request_bytes: 65_536,
        response_bytes: 65_536,
        buffer_bytes: 8 * 1024 * 1024,
        timeout: Duration::from_secs(5),
    })
    .expect("exact ceilings")
    .shutdown()
    .expect("join");
}

#[test]
fn concurrency_and_buffer_reservations_are_exact_and_reusable() {
    let server = Server::start(limits()).expect("start");
    let a = server.admit(&input(b"a")).expect("first");
    let b = server.admit(&input(b"b")).expect("second");
    assert!(matches!(server.admit(&input(b"c")), Err(Error::Limit)));
    drop(a);
    drop(server.admit(&input(b"c")).expect("reuse after destruction"));
    drop(b);
    assert_eq!(server.usage(), Ok((0, 0)));
    let server = Server::start(Limits { buffer_bytes: 20, ..limits() }).expect("start");
    let a = server.admit(&input(b"12")).expect("exact memory reservation");
    assert_eq!(server.usage(), Ok((1, 20)));
    assert!(matches!(server.admit(&input(b"")), Err(Error::Limit)));
    drop(a);
    assert!(matches!(server.admit(&input(b"123")), Err(Error::Limit)));
    assert_eq!(server.usage(), Ok((0, 0)));
}

#[test]
fn invalid_and_oversized_responses_consume_resources() {
    let server = Server::start(limits()).expect("start");
    for (status, body, expected) in [
        (199, &b""[..], Error::Malformed),
        (600, &b""[..], Error::Malformed),
        (200, &b"123456789"[..], Error::Limit),
    ] {
        let request = server.admit(&input(b"")).expect("admit");
        let destroyed = attach(&request);
        assert_eq!(request.finish(status, body), Err(expected));
        assert_eq!(destroyed.load(Ordering::SeqCst), 1);
        assert_eq!(server.usage(), Ok((0, 0)));
    }
}

#[test]
fn cancellation_is_idempotent_and_stale_handles_do_not_cancel_replacements() {
    let server = Server::start(limits()).expect("start");
    let first = server.admit(&input(b"one")).expect("first");
    let destroyed = attach(&first);
    let old = first.cancellation();
    old.cancel().expect("cancel");
    old.clone().cancel().expect("idempotent");
    assert_eq!(destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(first.check(), Err(Error::Inactive));
    let second = server.admit(&input(b"two")).expect("replacement");
    old.cancel().expect("stale cancel");
    drop(first);
    assert_eq!(second.check(), Ok(()));
    assert_eq!(second.finish(200, b"two"), Ok(b"two".to_vec()));
    assert_eq!(server.usage(), Ok((0, 0)));
}

#[test]
fn denied_capability_revokes_request_without_environment_or_filesystem_grants() {
    let server = Server::start(limits()).expect("start");
    let request = server.admit(&input(b"secret")).expect("admit");
    let destroyed = attach(&request);
    assert_eq!(request.deny_capability(), Err(Error::Denied));
    assert_eq!(destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(request.finish(200, b"ok"), Err(Error::Inactive));
    assert_eq!(server.usage(), Ok((0, 0)));
}

#[test]
fn shutdown_and_host_drop_revoke_outstanding_handles_and_join() {
    for explicit in [true, false] {
        let server = Server::start(limits()).expect("start");
        let request = server.admit(&input(b"body")).expect("admit");
        let destroyed = attach(&request);
        let cancel = request.cancellation();
        if explicit {
            server.shutdown().expect("joined");
        } else {
            drop(server);
        }
        assert_eq!(destroyed.load(Ordering::SeqCst), 1);
        assert_eq!(request.check(), Err(Error::Inactive));
        cancel.cancel().expect("cancel after host destruction");
        assert_eq!(request.finish(200, b"late"), Err(Error::Inactive));
    }
}

#[test]
fn panic_unwind_destroys_request_and_server() {
    let destroyed = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&destroyed);
    let result = std::panic::catch_unwind(move || {
        let server = Server::start(limits()).expect("start");
        let request = server.admit(&input(b"body")).expect("admit");
        request.retain(Resource(observed)).expect("resource");
        panic!("injected adapter failure");
    });
    assert!(result.is_err());
    assert_eq!(destroyed.load(Ordering::SeqCst), 1);
}

#[test]
fn quota_cannot_be_reused_while_old_authority_is_still_being_destroyed() {
    struct BlockingDrop {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl Drop for BlockingDrop {
        fn drop(&mut self) {
            self.entered.send(()).expect("destructor entered");
            self.release.recv_timeout(Duration::from_secs(2)).expect("release destructor");
        }
    }
    let server = Server::start(Limits { requests: 1, ..limits() }).expect("start");
    let request = server.admit(&input(b"body")).expect("admit");
    let (tx, rx) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    request.retain(BlockingDrop { entered: tx, release: wait }).expect("resource");
    let cancel = request.cancellation();
    let worker = thread::spawn(move || cancel.cancel());
    rx.recv_timeout(Duration::from_secs(2)).expect("destructor started outside lock");
    assert_eq!(server.usage(), Ok((1, 22)));
    assert!(matches!(server.admit(&input(b"new")), Err(Error::Limit)));
    release.send(()).expect("unblock");
    worker.join().expect("thread").expect("cancel");
    assert_eq!(server.usage(), Ok((0, 0)));
    server.admit(&input(b"new")).expect("reuse only after destruction");
}

#[test]
fn concurrent_admission_does_not_overbook_and_cancellation_publication_is_linearized() {
    let server = Arc::new(Server::start(Limits { requests: 4, ..limits() }).expect("start"));
    let barrier = Arc::new(Barrier::new(17));
    let admitted = Arc::new(AtomicUsize::new(0));
    thread::scope(|scope| {
        for _ in 0..16 {
            let server = Arc::clone(&server);
            let barrier = Arc::clone(&barrier);
            let admitted = Arc::clone(&admitted);
            scope.spawn(move || {
                let request = server.admit(&input(b"race"));
                if request.is_ok() {
                    admitted.fetch_add(1, Ordering::SeqCst);
                } else {
                    assert!(matches!(request, Err(Error::Limit)));
                }
                barrier.wait();
                drop(request);
            });
        }
        barrier.wait();
    });
    assert_eq!(admitted.load(Ordering::SeqCst), 4);
    assert_eq!(server.usage(), Ok((0, 0)));
    for _ in 0..32 {
        let request = server.admit(&input(b"race")).expect("admit");
        let destroyed = attach(&request);
        let cancel = request.cancellation();
        let barrier = Arc::new(Barrier::new(2));
        let other = Arc::clone(&barrier);
        let worker = thread::spawn(move || {
            other.wait();
            cancel.cancel()
        });
        barrier.wait();
        let response = request.finish(200, b"ok");
        worker.join().expect("thread").expect("cancel");
        assert!(response == Ok(b"ok".to_vec()) || response == Err(Error::Inactive));
        assert_eq!(destroyed.load(Ordering::SeqCst), 1);
        assert_eq!(server.usage(), Ok((0, 0)));
    }
}

#[test]
fn repeated_start_request_shutdown_has_no_retained_resources() {
    for _ in 0..32 {
        let server = Server::start(limits()).expect("fresh start");
        let request = server.admit(&input(b"body")).expect("admit");
        let destroyed = attach(&request);
        server.shutdown().expect("owned worker joined");
        assert_eq!(destroyed.load(Ordering::SeqCst), 1);
        assert_eq!(request.check(), Err(Error::Inactive));
    }
}

#[test]
fn resource_destructor_failure_stops_host_and_never_certifies_cleanup() {
    struct FailingDrop;
    impl Drop for FailingDrop {
        fn drop(&mut self) {
            panic!("injected destructor failure");
        }
    }
    let server = Server::start(limits()).expect("start");
    let request = server.admit(&input(b"body")).expect("admit");
    request.retain(FailingDrop).expect("resource");
    let sibling = server.admit(&input(b"body")).expect("sibling");
    let destroyed = attach(&sibling);
    assert_eq!(request.cancellation().cancel(), Err(Error::Host));
    assert_eq!(destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(sibling.check(), Err(Error::Inactive));
    assert!(matches!(server.admit(&input(b"")), Err(Error::Inactive)));
    assert_eq!(server.shutdown(), Err(Error::Host));
}

#[test]
fn rejected_resource_transfer_destroys_only_rejected_resource() {
    let server = Server::start(limits()).expect("start");
    let request = server.admit(&input(b"body")).expect("admit");
    let retained = attach(&request);
    let rejected = Arc::new(AtomicUsize::new(0));
    assert_eq!(request.retain(Resource(Arc::clone(&rejected))), Err(Error::Limit));
    assert_eq!(rejected.load(Ordering::SeqCst), 1);
    assert_eq!(retained.load(Ordering::SeqCst), 0);
    request.cancellation().cancel().expect("cancel");
    assert_eq!(retained.load(Ordering::SeqCst), 1);
    let late = Arc::new(AtomicUsize::new(0));
    assert_eq!(request.retain(Resource(Arc::clone(&late))), Err(Error::Inactive));
    assert_eq!(late.load(Ordering::SeqCst), 1);
}

#[test]
fn shutdown_waits_for_concurrent_resource_destruction() {
    struct BlockingDrop {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl Drop for BlockingDrop {
        fn drop(&mut self) {
            self.entered.send(()).expect("entered");
            self.release.recv_timeout(Duration::from_secs(2)).expect("released");
        }
    }
    let server = Server::start(limits()).expect("start");
    let request = server.admit(&input(b"body")).expect("admit");
    let (entered, wait) = mpsc::channel();
    let (release, released) = mpsc::channel();
    request.retain(BlockingDrop { entered, release: released }).expect("resource");
    let cancel = request.cancellation();
    let cancel_worker = thread::spawn(move || cancel.cancel());
    wait.recv_timeout(Duration::from_secs(2)).expect("destruction began");
    let (finished, completion) = mpsc::channel();
    let shutdown_worker = thread::spawn(move || {
        finished.send(server.shutdown()).expect("report");
    });
    assert!(matches!(
        completion.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    release.send(()).expect("release");
    cancel_worker.join().expect("canceller").expect("cancel");
    assert_eq!(completion.recv_timeout(Duration::from_secs(2)), Ok(Ok(())));
    shutdown_worker.join().expect("shutdown joined");
    assert_eq!(request.check(), Err(Error::Inactive));
}

#[test]
fn hard_body_and_path_boundaries_accept_exact_then_reject_first_extra() {
    let server = Server::start(Limits {
        request_bytes: 65_536,
        response_bytes: 65_536,
        buffer_bytes: 8 * 1024 * 1024,
        ..limits()
    })
    .expect("start");
    let body = vec![42; 65_536];
    let path = format!("/{}", "a".repeat(255));
    let mut exact = input(&body);
    exact.path = &path;
    let request = server.admit(&exact).expect("exact boundaries");
    assert_eq!(server.usage(), Ok((1, 65_536 * 2 + 260)));
    assert_eq!(request.finish(599, &body), Ok(body.clone()));
    let extra = vec![42; 65_537];
    assert!(matches!(server.admit(&input(&extra)), Err(Error::Limit)));
    let request = server.admit(&input(b"")).expect("admit");
    let destroyed = attach(&request);
    assert_eq!(request.finish(200, &extra), Err(Error::Limit));
    assert_eq!(destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(server.usage(), Ok((0, 0)));
}

#[test]
fn old_host_handles_never_select_new_host_requests() {
    let old = Server::start(limits()).expect("old host");
    let request = old.admit(&input(b"old")).expect("old request");
    let cancel = request.cancellation();
    old.shutdown().expect("old shutdown");
    let new = Server::start(limits()).expect("new host");
    let fresh = new.admit(&input(b"new")).expect("new request");
    let destroyed = attach(&fresh);
    cancel.cancel().expect("stale host cancel");
    drop(request);
    assert_eq!(fresh.check(), Ok(()));
    assert_eq!(destroyed.load(Ordering::SeqCst), 0);
    assert_eq!(fresh.finish(200, b"new"), Ok(b"new".to_vec()));
    assert_eq!(destroyed.load(Ordering::SeqCst), 1);
    new.shutdown().expect("new shutdown");
}
