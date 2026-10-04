use std::{
    sync::{atomic::AtomicBool, mpsc},
    thread,
    time::{Duration, Instant},
};

use super::super::host::Pause;
use super::*;

struct Release(Option<mpsc::Sender<()>>);
impl Release {
    fn now(mut self) {
        self.0.take().expect("release").send(()).expect("worker");
    }
}
impl Drop for Release {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

fn paused(
    envelope: Envelope,
    observation: &Arc<Observation>,
) -> (Server, mpsc::Receiver<()>, Release) {
    paused_failure(envelope, observation, false)
}

fn paused_failure(
    envelope: Envelope,
    observation: &Arc<Observation>,
    panic_after_release: bool,
) -> (Server, mpsc::Receiver<()>, Release) {
    let mut prepared = prepare(
        "204",
        ServerOperation::ClockRead,
        CLOCK,
        Approval::monotonic_clock_reads(1).expect("approval"),
        envelope,
        observation,
    )
    .expect("prepare");
    let (entered, wait) = mpsc::sync_channel(1);
    let (release, released) = mpsc::channel();
    prepared.pause = Some(Arc::new(Pause {
        claimed: AtomicBool::new(false),
        entered,
        release: std::sync::Mutex::new(released),
        panic_after_release,
    }));
    (Server::start(prepared).expect("start"), wait, Release(Some(release)))
}

fn one() -> Envelope {
    Envelope { requests: crate::Limits { requests: 1, ..crate::limits() }, ..envelope() }
}

#[test]
fn panicking_real_host_callback_destroys_actual_store_and_request_recovers() {
    let observation = Arc::new(Observation::default());
    let (server, entered, release) = paused_failure(one(), &observation, true);
    let pending = server.admit(&crate::input(b"ok")).expect("admit");
    entered.recv_timeout(Duration::from_secs(2)).expect("real callback entered");
    assert_eq!(observation.live(), (1, 2));
    release.now();
    assert_eq!(pending.wait(), Err(Error::Host));
    clean(&observation);
    assert_eq!(server.usage(), Ok((0, 0)));
    assert_eq!(
        server.admit(&crate::input(b"new")).expect("recovered host").wait(),
        Ok((204, vec![]))
    );
    server.shutdown().expect("actual worker joined");
    clean(&observation);
}

#[test]
fn cancellation_waits_actual_store_drop_and_retains_quota_until_cleanup() {
    let observation = Arc::new(Observation::default());
    let (server, entered, release) = paused(one(), &observation);
    let pending = server.admit(&crate::input(b"ok")).expect("admit");
    entered.recv_timeout(Duration::from_secs(2)).expect("real callback entered");
    assert_eq!(observation.live(), (1, 2));
    let cancel = pending.cancellation();
    let (finished, done) = mpsc::channel();
    let cancellation = thread::spawn(move || finished.send(cancel()).expect("observer"));
    let until = Instant::now() + Duration::from_secs(1);
    while !pending.revoked() && Instant::now() < until {
        thread::yield_now();
    }
    assert!(pending.revoked(), "actual retained revoker ran");
    assert!(done.recv_timeout(Duration::from_millis(30)).is_err(), "cleanup still in callback");
    assert_eq!(server.usage().expect("quota").0, 1);
    assert!(matches!(
        server.admit(&crate::input(b"new")),
        Err(Error::Lifecycle(crate::server_lifecycle::Error::Limit))
    ));
    release.now();
    assert_eq!(done.recv_timeout(Duration::from_secs(2)).expect("joined cancellation"), Ok(()));
    cancellation.join().expect("cancel worker joined");
    assert!(pending.wait().is_err(), "cancelled response cannot escape");
    clean(&observation);
    assert_eq!(server.usage(), Ok((0, 0)));
    assert_eq!(
        server.admit(&crate::input(b"new")).expect("quota recovered").wait(),
        Ok((204, vec![]))
    );
    server.shutdown().expect("joined");
    clean(&observation);
}

#[test]
fn autonomous_deadline_revokes_inflight_guest_and_awaits_store_cleanup() {
    let observation = Arc::new(Observation::default());
    let (server, entered, release) = paused(one(), &observation);
    let input = crate::Input {
        deadline: Instant::now() + Duration::from_millis(100),
        ..crate::input(b"ok")
    };
    let pending = server.admit(&input).expect("admit");
    entered.recv_timeout(Duration::from_secs(2)).expect("real callback entered");
    // Observe expiry without calling Request::check: the lifecycle deadline worker owns revocation.
    let until = Instant::now() + Duration::from_secs(1);
    while !pending.revoked() && Instant::now() < until {
        thread::yield_now();
    }
    assert!(pending.revoked(), "deadline worker autonomously ran the retained revoker");
    assert_eq!(observation.live().0, 1, "deadline cannot fabricate Store destruction");
    assert_eq!(server.usage().expect("quota").0, 1);
    assert!(matches!(
        server.admit(&crate::input(b"new")),
        Err(Error::Lifecycle(crate::server_lifecycle::Error::Limit))
    ));
    release.now();
    assert!(pending.wait().is_err(), "expired response suppressed");
    while server.usage().expect("quota").0 != 0 && Instant::now() < until {
        thread::yield_now();
    }
    assert_eq!(server.usage(), Ok((0, 0)));
    clean(&observation);
    server.shutdown().expect("joined");
}

#[test]
fn shutdown_joins_inflight_guest_and_suppresses_response() {
    let observation = Arc::new(Observation::default());
    let (server, entered, release) = paused(one(), &observation);
    let pending = server.admit(&crate::input(b"")).expect("admit");
    entered.recv_timeout(Duration::from_secs(2)).expect("real callback entered");
    let (finished, done) = mpsc::channel();
    let shutdown = thread::spawn(move || finished.send(server.shutdown()).expect("observer"));
    assert!(done.recv_timeout(Duration::from_millis(30)).is_err());
    assert_eq!(observation.live().0, 1);
    release.now();
    assert_eq!(done.recv_timeout(Duration::from_secs(2)).expect("teardown complete"), Ok(()));
    shutdown.join().expect("host worker joined");
    assert!(pending.wait().is_err());
    clean(&observation);
}

#[test]
fn simultaneous_cancellation_callers_both_wait_for_actual_guest_teardown() {
    let observation = Arc::new(Observation::default());
    let (server, entered, release) = paused(one(), &observation);
    let pending = server.admit(&crate::input(b"ok")).expect("admit");
    entered.recv_timeout(Duration::from_secs(2)).expect("callback paused");
    let first = pending.cancellation();
    let second = pending.cancellation();
    let (first_send, first_done) = mpsc::channel();
    let first_worker = thread::spawn(move || first_send.send(first()).expect("first observer"));
    let until = Instant::now() + Duration::from_secs(1);
    while !pending.revoked() && Instant::now() < until {
        thread::yield_now();
    }
    assert!(pending.revoked(), "first retirement detached the lease and invoked its revoker");
    let (starting, started) = mpsc::channel();
    let (second_send, second_done) = mpsc::channel();
    let second_worker = thread::spawn(move || {
        starting.send(()).expect("second scheduled");
        second_send.send(second()).expect("second observer");
    });
    started.recv_timeout(Duration::from_secs(1)).expect("second cancellation started");
    assert!(first_done.recv_timeout(Duration::from_millis(30)).is_err());
    assert!(
        second_done.recv_timeout(Duration::from_millis(30)).is_err(),
        "a detached lifecycle entry cannot certify actual Store destruction"
    );
    assert_eq!(observation.live(), (1, 2));
    assert_eq!(server.usage().expect("quota").0, 1);
    release.now();
    assert_eq!(first_done.recv_timeout(Duration::from_secs(2)).expect("first complete"), Ok(()));
    assert_eq!(second_done.recv_timeout(Duration::from_secs(2)).expect("second complete"), Ok(()));
    first_worker.join().expect("first caller joined");
    second_worker.join().expect("second caller joined");
    assert!(pending.wait().is_err());
    assert_eq!(server.usage(), Ok((0, 0)));
    clean(&observation);
    server.shutdown().expect("joined");
}
