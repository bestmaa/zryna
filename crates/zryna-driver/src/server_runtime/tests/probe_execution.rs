use std::{
    thread,
    time::{Duration, Instant},
};

use super::*;

#[test]
fn real_cpu_loop_is_interrupted_by_autonomous_deadline_with_fuel_remaining() {
    let observation = Arc::new(Observation::default());
    let mut prepared = prepare(
        "200",
        ServerOperation::Reply,
        EMPTY,
        Approval::deny_all(),
        envelope(),
        &observation,
    )
    .expect("prepare");
    prepared.runtime_probe(false).expect("runtime-only spin fixture");
    let server = Server::start(prepared).expect("private probe host");
    let started = Instant::now();
    let input =
        crate::Input { deadline: started + Duration::from_millis(100), ..crate::input(b"") };
    let pending = server.admit(&input).expect("admit");
    assert!(pending.wait().is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(
        observation.remaining_fuel.load(Ordering::SeqCst) > 500_000_000,
        "actual epoch/deadline interruption precedes fuel exhaustion"
    );
    server.shutdown().expect("joined");
    clean(&observation);
}

#[test]
fn real_cpu_loop_is_interrupted_by_cancellation_with_fuel_remaining() {
    let observation = Arc::new(Observation::default());
    let mut prepared = prepare(
        "200",
        ServerOperation::Reply,
        EMPTY,
        Approval::deny_all(),
        envelope(),
        &observation,
    )
    .expect("prepare");
    prepared.runtime_probe(false).expect("runtime-only spin fixture");
    let server = Server::start(prepared).expect("private probe host");
    let pending = server.admit(&crate::input(b"")).expect("admit");
    let until = Instant::now() + Duration::from_secs(1);
    while observation.created.load(Ordering::SeqCst) < 2 && Instant::now() < until {
        thread::yield_now();
    }
    assert_eq!(observation.created.load(Ordering::SeqCst), 2);
    let cancel = pending.cancellation();
    thread::sleep(Duration::from_millis(5));
    cancel().expect("actual CPU guest joined");
    assert!(pending.wait().is_err());
    assert!(observation.remaining_fuel.load(Ordering::SeqCst) > 500_000_000);
    server.shutdown().expect("joined");
    clean(&observation);
}

#[test]
fn actual_guest_memory_growth_traps_at_one_page_and_destroys_store() {
    let observation = Arc::new(Observation::default());
    let mut prepared = prepare(
        "200",
        ServerOperation::Reply,
        EMPTY,
        Approval::deny_all(),
        envelope(),
        &observation,
    )
    .expect("prepare");
    prepared.runtime_probe(true).expect("runtime-only growth fixture");
    let server = Server::start(prepared).expect("private probe host");
    assert_eq!(server.admit(&crate::input(b"")).expect("admit").wait(), Err(Error::Guest));
    assert!(observation.remaining_fuel.load(Ordering::SeqCst) > 0);
    assert_eq!(
        observation.returned_calls.load(Ordering::SeqCst),
        0,
        "memory growth must trap inside the actual guest call, not merely return -1"
    );
    assert_eq!(server.usage(), Ok((0, 0)));
    server.shutdown().expect("joined");
    clean(&observation);
}
