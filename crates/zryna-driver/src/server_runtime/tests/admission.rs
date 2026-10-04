use super::*;

#[test]
fn unknown_duplicate_command_environment_filesystem_and_unapproved_grants_fail_before_engine() {
    let mut documents = vec![b"".to_vec(), b"{".to_vec(), vec![b' '; 2049],
        CLOCK.to_vec(),
        br#"{"world":"zryna:capability-profiles/server@0.1.0","requests":[],"approved":["clock"]}"#.to_vec(),
        br#"{"world":"zryna:capability-profiles/server@0.1.0","world":"zryna:capability-profiles/server@0.1.0","requests":[]}"#.to_vec(),
        br#"{"world":"zryna:capability-profiles/server@0.1.0","requests":["clock","clock"]}"#.to_vec(),
    ];
    for capability in ["environment", "filesystem", "network", "random", "unknown"] {
        documents.push(format!(r#"{{"world":"zryna:capability-profiles/server@0.1.0","requests":["{capability}"]}}"#).into_bytes());
    }
    for world in ["command", "pure", "server@0.2.12"] {
        documents.push(
            format!(r#"{{"world":"zryna:capability-profiles/{world}","requests":[]}}"#)
                .into_bytes(),
        );
    }
    for document in documents {
        let observation = Arc::new(Observation::default());
        assert!(matches!(
            prepare(
                "200",
                ServerOperation::Reply,
                &document,
                Approval::deny_all(),
                envelope(),
                &observation
            ),
            Err(Error::Grant)
        ));
        assert_eq!(observation.engines_constructed.load(Ordering::SeqCst), 0);
        clean(&observation);
    }
}

#[test]
fn clock_read_request_bounds_do_not_enable_timers_or_subscriptions() {
    for (reads, subscriptions, timers, approved, accepted) in [
        (1, 0, 0, 1, true),
        (16, 0, 0, 16, true),
        (0, 0, 0, 1, false),
        (17, 0, 0, 16, false),
        (2, 0, 0, 1, false),
        (1, 1, 0, 1, false),
        (1, 0, 1, 1, false),
    ] {
        let document = serde_json::to_vec(&serde_json::json!({
            "world": "zryna:capability-profiles/server@0.1.0",
            "requests": ["clock"],
            "clock": { "monotonic_reads": reads, "subscriptions": subscriptions, "timers": timers }
        }))
        .expect("clock request");
        let grants = super::super::grants::Grants::admit(
            &document,
            Approval::monotonic_clock_reads(approved).expect("bounded approval"),
        );
        assert_eq!(grants.is_ok(), accepted);
    }
    assert!(Approval::monotonic_clock_reads(0).is_err());
    assert!(Approval::monotonic_clock_reads(17).is_err());
}

#[test]
fn invalid_envelopes_fail_before_host_and_seal_rejects_substituted_limits() {
    for limited in [
        Envelope { memory_bytes: 65_537, ..envelope() },
        Envelope { fuel: 0, ..envelope() },
        Envelope { resources: 9, ..envelope() },
        Envelope { callbacks: 17, ..envelope() },
        Envelope {
            requests: crate::Limits { buffer_bytes: 4 * 1024 * 1024 + 1, ..crate::limits() },
            ..envelope()
        },
    ] {
        let observation = Arc::new(Observation::default());
        assert!(matches!(
            prepare(
                "200",
                ServerOperation::Reply,
                EMPTY,
                Approval::deny_all(),
                limited,
                &observation
            ),
            Err(Error::Limit)
        ));
        assert_eq!(observation.engines_constructed.load(Ordering::SeqCst), 0);
        clean(&observation);
    }
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
    prepared.envelope.fuel -= 1;
    assert!(matches!(Server::start(prepared), Err(Error::Artifact)));
    clean(&observation);
    let mut prepared = prepare(
        "200",
        ServerOperation::ClockRead,
        EMPTY,
        Approval::deny_all(),
        envelope(),
        &observation,
    )
    .expect("prepare denied clock");
    prepared.grants = super::super::grants::Grants::admit(
        CLOCK,
        Approval::monotonic_clock_reads(1).expect("separate approval"),
    )
    .expect("grants");
    assert!(matches!(Server::start(prepared), Err(Error::Artifact)));
    clean(&observation);
}

#[test]
fn malformed_expired_oversized_requests_create_no_store_and_valid_request_recovers() {
    let observation = Arc::new(Observation::default());
    let server = server(
        "203",
        ServerOperation::Reply,
        EMPTY,
        Approval::deny_all(),
        envelope(),
        &observation,
    );
    for (method, path) in [
        ("DELETE", "/"),
        ("GET", "relative"),
        ("POST", "/bad path"),
        ("GET", "/bad\r\n"),
        ("GET", "/#fragment"),
    ] {
        let input = crate::Input { method, path, ..crate::input(b"") };
        assert!(matches!(
            server.admit(&input),
            Err(Error::Lifecycle(crate::server_lifecycle::Error::Malformed))
        ));
    }
    let expired = crate::Input { deadline: std::time::Instant::now(), ..crate::input(b"") };
    assert!(matches!(
        server.admit(&expired),
        Err(Error::Lifecycle(crate::server_lifecycle::Error::Deadline))
    ));
    assert!(matches!(
        server.admit(&crate::input(b"123456789")),
        Err(Error::Lifecycle(crate::server_lifecycle::Error::Limit))
    ));
    assert_eq!(server.usage(), Ok((0, 0)));
    assert_eq!(observation.created.load(Ordering::SeqCst), 0);
    clean(&observation);
    assert_eq!(server.admit(&crate::input(b"ok")).expect("recover").wait(), Ok((203, vec![])));
    clean(&observation);
    server.shutdown().expect("joined");
}
