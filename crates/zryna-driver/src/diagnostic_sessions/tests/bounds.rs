use std::{sync::Arc, time::Duration};

use serde_json::json;
use zryna_diagnostics::Diagnostic;

use super::{ready_session, request, sources};
use crate::diagnostic_sessions::{
    Correlation, DiagnosticSession, MAX_IN_FLIGHT_REQUESTS, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES,
    MAX_SESSION_CACHE_BYTES, QUERY_DEADLINE, QueryReason, QueryStatus, RevisionRecord,
    checked_cache_charge, response::encode_success, source_fingerprint_and_charge,
};

#[test]
fn request_bytes_are_inclusive_and_first_extra_recovers() {
    let (mut session, revision, now) = ready_session("let x = 1;\n");
    let mut exact = request("bytes", revision, 100_000, "diagnostics", json!({}));
    exact.resize(MAX_REQUEST_BYTES, b' ');
    let pending = session
        .begin_diagnostics(&exact, now)
        .unwrap_or_else(|response| panic!("exact request limit must pass: {response:?}"));
    assert_eq!(session.finish_diagnostics(pending, now).status(), QueryStatus::Ok);

    exact.push(b' ');
    let rejected =
        session.begin_diagnostics(&exact, now).expect_err("first extra request byte must reject");
    assert_eq!(
        (rejected.status(), rejected.reason()),
        (QueryStatus::OverBudget, Some(QueryReason::RequestBytes))
    );

    let recovered = session
        .begin_diagnostics(&request("bytes", revision, 100_000, "diagnostics", json!({})), now)
        .unwrap_or_else(|response| panic!("byte rejection must not poison state: {response:?}"));
    assert_eq!(session.finish_diagnostics(recovered, now).status(), QueryStatus::Ok);
}

#[test]
fn request_depth_is_inclusive_and_checked_before_shape() {
    let (mut session, revision, now) = ready_session("let x = 1;\n");
    let base = format!(
        "{{\"query_version\":1,\"request_id\":\"depth\",\"snapshot\":\"{}\",\"revision\":{},\"method\":\"diagnostics\",\"params\":{{}},\"limits\":{{\"work\":100000,\"results\":10000}}}}",
        revision.handle(),
        revision.revision()
    );
    let nested = format!("{}0{}", "[".repeat(65), "]".repeat(65));
    let malformed_deep = format!("{base}{nested}");
    let response = session
        .begin_diagnostics(malformed_deep.as_bytes(), now)
        .expect_err("depth 65 must reject before trailing shape");
    assert_eq!(
        (response.status(), response.reason()),
        (QueryStatus::OverBudget, Some(QueryReason::RequestDepth))
    );
}

#[test]
fn queue_limit_is_exact_and_cancellation_recovers() {
    let (mut session, revision, now) = ready_session("let x = 1;\n");
    let mut pending = Vec::new();
    for index in 0..MAX_IN_FLIGHT_REQUESTS {
        let bytes = request(&format!("q{index}"), revision, 100_000, "diagnostics", json!({}));
        pending.push(
            session
                .begin_diagnostics(&bytes, now)
                .unwrap_or_else(|response| panic!("slot {index} must fit: {response:?}")),
        );
    }
    let extra = request("extra", revision, 100_000, "diagnostics", json!({}));
    let response =
        session.begin_diagnostics(&extra, now).expect_err("first extra queue slot must reject");
    assert_eq!(
        (response.status(), response.reason()),
        (QueryStatus::OverBudget, Some(QueryReason::Session))
    );
    assert!(session.cancel("q0"));
    let replacement = session
        .begin_diagnostics(&extra, now)
        .unwrap_or_else(|response| panic!("cancelled slot must be reusable: {response:?}"));
    assert_eq!(session.finish_diagnostics(replacement, now).status(), QueryStatus::Ok);
    drop(pending);
}

#[test]
fn abandoned_queue_slots_expire_at_the_exact_deadline() {
    let (mut session, revision, now) = ready_session("let x = 1;\n");
    let mut abandoned = Vec::new();
    for index in 0..MAX_IN_FLIGHT_REQUESTS {
        abandoned.push(
            session
                .begin_diagnostics(
                    &request(
                        &format!("abandoned{index}"),
                        revision,
                        100_000,
                        "diagnostics",
                        json!({}),
                    ),
                    now,
                )
                .unwrap_or_else(|response| panic!("slot {index} must fit: {response:?}")),
        );
    }
    let replacement = session
        .begin_diagnostics(
            &request("after-deadline", revision, 100_000, "diagnostics", json!({})),
            now + QUERY_DEADLINE,
        )
        .unwrap_or_else(|response| panic!("deadline must recover all slots: {response:?}"));
    assert_eq!(
        session.finish_diagnostics(replacement, now + QUERY_DEADLINE).status(),
        QueryStatus::Ok
    );
    let expired = abandoned.remove(0);
    let response = session.finish_diagnostics(expired, now + QUERY_DEADLINE);
    assert_eq!(
        (response.status(), response.reason()),
        (QueryStatus::Cancelled, Some(QueryReason::Deadline))
    );
}

#[test]
fn source_cache_charge_counts_exact_utf8_path_text_and_report_bytes() {
    let path = "src/diagnostics/main.zry";
    let map = sources(path, "😀\r\n");
    let (_, source_bytes) = source_fingerprint_and_charge(&map)
        .unwrap_or_else(|error| panic!("source charge must succeed: {error}"));
    assert_eq!(source_bytes, path.len() + "😀\r\n".len());
    let mut session =
        DiagnosticSession::try_new().unwrap_or_else(|error| panic!("session must exist: {error}"));
    let report = zryna_diagnostics::protocol_v2::render_json(&[], &map)
        .unwrap_or_else(|error| panic!("empty report must render: {error}"));
    session
        .admit_diagnostics(map, &[])
        .unwrap_or_else(|error| panic!("revision must fit: {error}"));
    assert_eq!(session.cache_bytes(), source_bytes + report.len());
}

#[test]
fn cache_charge_limit_is_inclusive_and_first_extra_rejects() {
    assert_eq!(
        checked_cache_charge(MAX_SESSION_CACHE_BYTES - 1, 1, 0)
            .unwrap_or_else(|error| panic!("exact cache limit must fit: {error}")),
        MAX_SESSION_CACHE_BYTES
    );
    assert!(matches!(
        checked_cache_charge(MAX_SESSION_CACHE_BYTES, 1, 0),
        Err(crate::diagnostic_sessions::DiagnosticSessionError::SessionCacheExhausted)
    ));
}

#[test]
fn cache_retention_accepts_exact_limit_and_evicts_for_first_extra() {
    let (mut session, _, _) = ready_session("");
    let active = Arc::clone(
        session.retained.back().unwrap_or_else(|| panic!("ready session must retain a record")),
    );
    session.retained.clear();
    session.cache_bytes = 0;
    let exact = Arc::new(RevisionRecord {
        description: active.description,
        sources: active.sources.clone(),
        report: active.report.clone(),
        definitions: active.definitions.clone(),
        cache_bytes: MAX_SESSION_CACHE_BYTES,
        formatting: Vec::new(),
    });
    session.retained.push_back(exact);
    session.cache_bytes = MAX_SESSION_CACHE_BYTES;
    assert_eq!(session.cache_bytes(), MAX_SESSION_CACHE_BYTES);

    let next =
        session.admit_diagnostics(sources("src/main.zry", "x"), &[]).unwrap_or_else(|error| {
            panic!("new revision must deterministically evict old cache: {error}")
        });
    assert_eq!(session.active_revision(), Some(next));
    assert_eq!(session.retained_revisions(), 1);
    assert!(session.cache_bytes() < MAX_SESSION_CACHE_BYTES);
}

#[test]
fn response_bytes_are_inclusive_and_first_extra_rejects_whole_result() {
    let correlation = Correlation {
        query_version: 1,
        request_id: "q".to_owned(),
        snapshot: "s".to_owned(),
        revision: 1,
    };
    let empty =
        encode_success(&correlation, "\"\"").unwrap_or_else(|| panic!("small response must fit"));
    let overhead = empty.len() - 2;
    let exact_report = format!("\"{}\"", "x".repeat(MAX_RESPONSE_BYTES - overhead - 2));
    let exact = encode_success(&correlation, &exact_report)
        .unwrap_or_else(|| panic!("exact response byte limit must fit"));
    assert_eq!(exact.len(), MAX_RESPONSE_BYTES);
    assert!(encode_success(&correlation, &format!("{exact_report} ")).is_none());
}

#[test]
fn work_charge_is_exact_and_warm_replay_is_identical() {
    let (mut session, _, now) = ready_session("let old = 0;\n");
    let revision = session
        .admit_diagnostics(sources("src/main.zry", "let x = 1;\n"), &[])
        .unwrap_or_else(|error| panic!("active revision must be admitted: {error}"));
    assert_eq!(session.retained_revisions(), 2);
    let report_len = session
        .retained
        .back()
        .and_then(|record| record.report.as_ref())
        .map_or_else(|| panic!("ready record must have a report"), |report| report.len());
    let work = u64::try_from(report_len + 1).unwrap_or_else(|_| panic!("report must fit u64"));
    let bytes = request("warm", revision, work, "diagnostics", json!({}));
    let first = session
        .begin_diagnostics(&bytes, now)
        .unwrap_or_else(|response| panic!("exact work must begin: {response:?}"));
    let first = session.finish_diagnostics(first, now);
    assert_eq!(first.status(), QueryStatus::Ok);

    let second = session
        .begin_diagnostics(&bytes, now + Duration::from_secs(1))
        .unwrap_or_else(|response| panic!("warm replay must begin: {response:?}"));
    let second = session.finish_diagnostics(second, now + Duration::from_secs(1));
    assert_eq!(first.encoded(), second.encoded());

    let low = request("low", revision, work - 1, "diagnostics", json!({}));
    let pending = session
        .begin_diagnostics(&low, now)
        .unwrap_or_else(|response| panic!("low-work request passes admission: {response:?}"));
    let response = session.finish_diagnostics(pending, now);
    assert_eq!(
        (response.status(), response.reason()),
        (QueryStatus::OverBudget, Some(QueryReason::Work))
    );

    let recovered = session
        .begin_diagnostics(&request("recovered", revision, work, "diagnostics", json!({})), now)
        .unwrap_or_else(|response| panic!("work rejection must not poison state: {response:?}"));
    assert_eq!(session.finish_diagnostics(recovered, now).status(), QueryStatus::Ok);
}

#[test]
fn terminal_v2_report_is_preserved_without_a_partial_prefix() {
    let map = sources("src/main.zry", "");
    let diagnostics = (0..=zryna_diagnostics::protocol_v2::MAX_DIAGNOSTICS)
        .map(|_| Diagnostic::error("ZRYNA-T1003", None, "inert", "inert"))
        .collect::<Vec<_>>();
    let expected = zryna_diagnostics::protocol_v2::render_json(&diagnostics, &map)
        .unwrap_or_else(|error| panic!("exhaustion terminal must render: {error}"));
    let mut session =
        DiagnosticSession::try_new().unwrap_or_else(|error| panic!("session must exist: {error}"));
    let revision = session
        .admit_diagnostics(map, &diagnostics)
        .unwrap_or_else(|error| panic!("terminal report must be retained: {error}"));
    let now = std::time::Instant::now();
    let pending = session
        .begin_diagnostics(&request("terminal", revision, 100_000, "diagnostics", json!({})), now)
        .unwrap_or_else(|response| panic!("terminal query must begin: {response:?}"));
    let response = session.finish_diagnostics(pending, now);
    assert_eq!(response.status(), QueryStatus::Ok);
    assert!(response.encoded().is_some_and(|encoded| encoded.contains(&expected)));
    assert!(expected.contains("ZRYNA-D2001"));
    assert!(!expected.contains("ZRYNA-T1003"));
}
