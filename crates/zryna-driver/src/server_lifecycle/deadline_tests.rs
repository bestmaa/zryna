//! Autonomous deadline-worker regressions.

use super::*;

#[test]
fn completion_rechecks_deadline_after_resource_destruction() {
    struct DelayedDrop(Instant, Arc<AtomicUsize>);
    impl Drop for DelayedDrop {
        fn drop(&mut self) {
            thread::sleep(self.0.saturating_duration_since(Instant::now()));
            self.1.fetch_add(1, Ordering::SeqCst);
        }
    }
    let server = Server::start(limits()).expect("start");
    let mut timed = input(b"body");
    timed.deadline = Instant::now() + Duration::from_millis(100);
    let request = server.admit(&timed).expect("admit");
    let destroyed = Arc::new(AtomicUsize::new(0));
    request
        .retain(DelayedDrop(timed.deadline + Duration::from_millis(10), Arc::clone(&destroyed)))
        .expect("resource");
    assert_eq!(request.finish(200, b"late"), Err(Error::Deadline));
    assert_eq!(destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(server.usage(), Ok((0, 0)));
    server.shutdown().expect("joined");
}

#[test]
fn failing_expired_resource_does_not_strand_other_retired_reservations() {
    struct FailingDrop(mpsc::Sender<()>);
    impl Drop for FailingDrop {
        fn drop(&mut self) {
            self.0.send(()).expect("destruction attempted");
            panic!("injected deadline destructor failure");
        }
    }
    let server = Server::start(limits()).expect("start");
    let mut timed = input(b"body");
    timed.deadline = Instant::now() + Duration::from_millis(100);
    let first = server.admit(&timed).expect("first");
    let second = server.admit(&timed).expect("second");
    let (tx, rx) = mpsc::channel();
    first.retain(FailingDrop(tx)).expect("first resource");
    let destroyed = attach(&second);
    rx.recv_timeout(Duration::from_secs(2)).expect("deadline drop attempted");
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        tx.send(server.shutdown()).expect("report");
    });
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(Err(Error::Host)));
    worker.join().expect("shutdown joined without stranded reservations");
    assert_eq!(destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(second.check(), Err(Error::Inactive));
}

#[test]
fn deadline_worker_reclaims_idle_request_without_another_callback() {
    struct Signal(mpsc::Sender<()>);
    impl Drop for Signal {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let server = Server::start(limits()).expect("start");
    let mut short = input(b"body");
    short.deadline = Instant::now() + Duration::from_millis(100);
    let request = server.admit(&short).expect("admit");
    let (tx, rx) = mpsc::channel();
    request.retain(Signal(tx)).expect("resource");
    rx.recv_timeout(Duration::from_secs(2)).expect("actual autonomous deadline destruction");
    let wait_limit = Instant::now() + Duration::from_secs(2);
    while server.usage() != Ok((0, 0)) {
        assert!(Instant::now() < wait_limit, "deadline reservations were not released");
        thread::yield_now();
    }
    assert!(matches!(request.check(), Err(Error::Inactive | Error::Deadline)));
    assert!(matches!(request.finish(200, b"late"), Err(Error::Inactive | Error::Deadline)));
    assert_eq!(server.usage(), Ok((0, 0)));
    server.admit(&input(b"again")).expect("recovery");
    server.shutdown().expect("joined");
}

#[test]
fn earlier_deadline_wakes_worker_waiting_on_a_later_request() {
    struct Signal(mpsc::Sender<()>);
    impl Drop for Signal {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let server = Server::start(limits()).expect("start");
    let long = server.admit(&input(b"long")).expect("long request");
    let mut short = input(b"short");
    short.deadline = Instant::now() + Duration::from_millis(100);
    let short = server.admit(&short).expect("short request");
    let (tx, rx) = mpsc::channel();
    short.retain(Signal(tx)).expect("resource");
    rx.recv_timeout(Duration::from_secs(2)).expect("worker woke for earlier deadline");
    assert_eq!(long.check(), Ok(()));
    server.shutdown().expect("joined");
}
