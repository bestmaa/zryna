//! Actual epoch callback continuation, after both guest Stores have begun execution.

use std::{
    thread,
    time::{Duration, Instant},
};

use super::*;

#[test]
fn cancelling_one_cpu_guest_continues_the_active_siblings_epoch_callback() {
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
    prepared.runtime_probe(false).expect("runtime-only CPU probes");
    let server = Server::start(prepared).expect("host");
    let first = server.admit(&crate::input(b"one")).expect("first");
    let sibling = server.admit(&crate::input(b"two")).expect("sibling");
    let until = Instant::now() + Duration::from_secs(1);
    while observation.created.load(Ordering::SeqCst) < 4 && Instant::now() < until {
        thread::yield_now();
    }
    assert_eq!(
        observation.created.load(Ordering::SeqCst),
        4,
        "both Stores initialized their epoch deadlines and real input resources"
    );
    first.cancellation()().expect("first actually torn down");
    assert!(first.wait().is_err());
    while observation.continued_epochs.load(Ordering::SeqCst) == 0 && Instant::now() < until {
        thread::yield_now();
    }
    assert!(
        observation.continued_epochs.load(Ordering::SeqCst) > 0,
        "an actual callback checked the sibling's independent token and continued"
    );
    assert_eq!(observation.live(), (1, 2), "unrevoked sibling is still executing");
    assert_eq!(server.usage().expect("only sibling reserved").0, 1);
    sibling.cancellation()().expect("sibling independently torn down");
    assert!(sibling.wait().is_err());
    server.shutdown().expect("joined");
    clean(&observation);
}
