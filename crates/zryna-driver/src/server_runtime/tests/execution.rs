use super::*;

#[test]
fn verified_scalar_status_executes_real_incoming_handler_and_cleans_before_response() {
    for (expression, status) in [("200", 200), ("199 + 2", 201), ("599", 599)] {
        let observation = Arc::new(Observation::default());
        let server = server(
            expression,
            ServerOperation::Reply,
            EMPTY,
            Approval::deny_all(),
            envelope(),
            &observation,
        );
        assert_eq!(
            server.admit(&crate::input(b"request")).expect("admit").wait(),
            Ok((status, vec![]))
        );
        assert_eq!(server.usage(), Ok((0, 0)));
        assert_eq!(observation.created.load(Ordering::SeqCst), 5);
        assert_eq!(observation.incoming_bytes.load(Ordering::SeqCst), 17);
        assert_eq!(observation.clock_reads.load(Ordering::SeqCst), 0);
        assert_eq!(observation.denials.load(Ordering::SeqCst), 0);
        clean(&observation);
        server.shutdown().expect("joined");
    }
}

#[test]
fn host_approval_and_request_both_required_for_real_clock_callback() {
    for (document, approval, allowed) in [
        (CLOCK, Approval::monotonic_clock_reads(1).expect("approval"), true),
        (EMPTY, Approval::monotonic_clock_reads(1).expect("approval"), false),
        (EMPTY, Approval::deny_all(), false),
    ] {
        let observation = Arc::new(Observation::default());
        let server =
            server("202", ServerOperation::ClockRead, document, approval, envelope(), &observation);
        let result = server.admit(&crate::input(b"")).expect("admit").wait();
        if allowed {
            assert_eq!(result, Ok((202, vec![])));
        } else {
            assert_eq!(result, Err(Error::Denied));
        }
        assert_eq!(observation.clock_reads.load(Ordering::SeqCst), usize::from(allowed));
        assert_eq!(observation.denials.load(Ordering::SeqCst), usize::from(!allowed));
        assert_eq!(server.usage(), Ok((0, 0)));
        clean(&observation);
        server.shutdown().expect("joined");
    }
}

#[test]
fn actual_guest_resource_fuel_memory_and_callback_limits_trap_and_recover() {
    for (limited, expected, created) in [
        (Envelope { resources: 3, ..envelope() }, Error::Limit, 4),
        (Envelope { fuel: 1, ..envelope() }, Error::Guest, 2),
        (Envelope { memory_bytes: 65_535, ..envelope() }, Error::Guest, 0),
        (Envelope { callbacks: 5, ..envelope() }, Error::Limit, 5),
    ] {
        let observation = Arc::new(Observation::default());
        let server = server(
            "201",
            ServerOperation::Reply,
            EMPTY,
            Approval::deny_all(),
            limited,
            &observation,
        );
        for attempt in 1..=2 {
            assert_eq!(server.admit(&crate::input(b"")).expect("admit").wait(), Err(expected));
            assert_eq!(observation.stores_created.load(Ordering::SeqCst), attempt);
            assert_eq!(observation.created.load(Ordering::SeqCst), attempt * created);
            assert_eq!(server.usage(), Ok((0, 0)));
            clean(&observation);
        }
        server.shutdown().expect("joined");
    }
}

#[test]
fn invalid_scalar_status_traps_real_guest_without_publishing_or_leaking() {
    for status in ["199", "600", "-1"] {
        let observation = Arc::new(Observation::default());
        let server = server(
            status,
            ServerOperation::Reply,
            EMPTY,
            Approval::deny_all(),
            envelope(),
            &observation,
        );
        assert_eq!(server.admit(&crate::input(b"")).expect("admit").wait(), Err(Error::Guest));
        assert_eq!(server.usage(), Ok((0, 0)));
        clean(&observation);
        server.shutdown().expect("joined");
    }
}

#[test]
fn repeated_fresh_start_serve_shutdown_leaves_no_owned_state() {
    let observation = Arc::new(Observation::default());
    for _ in 0..12 {
        let server = server(
            "205",
            ServerOperation::Reply,
            EMPTY,
            Approval::deny_all(),
            envelope(),
            &observation,
        );
        assert_eq!(server.admit(&crate::input(b"ok")).expect("admit").wait(), Ok((205, vec![])));
        server.shutdown().expect("actual workers joined");
        clean(&observation);
    }
    assert_eq!(observation.engines_constructed.load(Ordering::SeqCst), 12);
    assert_eq!(observation.created.load(Ordering::SeqCst), 60);
}

#[test]
fn resource_registry_rejects_wrong_type_stale_and_duplicate_ownership() {
    use super::super::resources::{Kind, Resources};
    let observation = Arc::new(Observation::default());
    let mut resources = Resources::new(1, Arc::clone(&observation));
    let id = resources.insert(Kind::Fields, 7).expect("one owned object");
    assert!(matches!(resources.insert(Kind::Fields, 7), Err(Error::Limit)));
    assert!(matches!(resources.take(id, 8), Err(Error::Resource)));
    assert_eq!(observation.live(), (0, 1));
    drop(resources.take(id, 7).expect("correct owner"));
    assert!(matches!(resources.take(id, 7), Err(Error::Resource)));
    let replacement = resources.insert(Kind::Fields, 7).expect("recovered quota");
    assert_ne!(replacement, id, "retired IDs never select a replacement");
    assert!(matches!(resources.get_mut(id, 7), Err(Error::Resource)));
    drop(resources);
    clean(&observation);
}

#[test]
fn completed_guest_response_cannot_publish_after_lease_cancellation() {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let observation = Arc::new(Observation::default());
    let server = server(
        "201",
        ServerOperation::Reply,
        EMPTY,
        Approval::deny_all(),
        envelope(),
        &observation,
    );
    let pending = server.admit(&crate::input(b"ok")).expect("admit");
    let until = Instant::now() + Duration::from_secs(1);
    while !pending.response_ready().expect("worker state") && Instant::now() < until {
        thread::yield_now();
    }
    assert!(pending.response_ready().expect("worker state"), "successful candidate is ready");
    assert_eq!(observation.created.load(Ordering::SeqCst), 5);
    assert_eq!(
        observation.destroyed.load(Ordering::SeqCst),
        5,
        "real guest computed and completed its response before this cancellation"
    );
    pending.cancellation()().expect("quiescent lease cancelled");
    assert_eq!(
        pending.wait(),
        Err(Error::Lifecycle(crate::server_lifecycle::Error::Inactive)),
        "final lifecycle publication check suppresses an already-computed guest response"
    );
    assert_eq!(server.usage(), Ok((0, 0)));
    server.shutdown().expect("joined");
    clean(&observation);
}
