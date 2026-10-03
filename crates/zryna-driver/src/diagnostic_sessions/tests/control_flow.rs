use std::{ffi::OsString, path::PathBuf, process::Command, time::Instant};

use serde_json::{Value, json};
use zryna_frontend::{ProviderExpectationV3, WorkerFrontendV3, WorkerLimitsV3, WorkerSpecV3};
use zryna_source::NormalizedSourcePath;

use super::{request, sources};
use crate::diagnostic_sessions::{
    DiagnosticSession, DiagnosticSessionError, FormattingError, QueryReason, QueryStatus,
};

const PATH: &str = "src/main.zry";

fn frontend() -> WorkerFrontendV3 {
    let output =
        crate::process_spawn::output(Command::new("node").args(["-p", "process.execPath"]))
            .expect("node path");
    assert!(output.status.success());
    let node = PathBuf::from(String::from_utf8(output.stdout).expect("node UTF-8").trim());
    let expected = ProviderExpectationV3::new("typescript-6", "6.0.3").expect("provider");
    WorkerFrontendV3::new(
        WorkerSpecV3::new(
            node,
            vec![OsString::from("src/worker-v3.mjs")],
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adapters/typescript-6"),
            expected,
            WorkerLimitsV3::default(),
        )
        .expect("worker"),
    )
}

fn analyze(text: &str) -> (DiagnosticSession, Value) {
    let map = sources(PATH, text);
    let mut session = DiagnosticSession::try_new().expect("session");
    let entry = NormalizedSourcePath::new(PATH).expect("entry");
    let revision = match frontend().analyze_verified_v3(&map) {
        Ok(syntax) => session.admit_control_flow_analysis(map, &syntax, &entry),
        Err(error) => session.admit_worker_failure(map, &error),
    }
    .expect("M2 admission");
    let now = Instant::now();
    let pending = session
        .begin_diagnostics(&request("m2", revision, 100_000, "diagnostics", json!({})), now)
        .expect("ready request");
    let response = session.finish_diagnostics(pending, now);
    assert_eq!(response.status(), QueryStatus::Ok);
    let report = serde_json::from_str(response.encoded().expect("encoded report"))
        .expect("valid JSON response");
    (session, report)
}

fn codes(report: &Value) -> Vec<&str> {
    report["result"]["report"]["diagnostics"]
        .as_array()
        .expect("diagnostic array")
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().expect("code"))
        .collect()
}

#[test]
fn control_flow_program_has_ready_diagnostics_without_scalar_rejection() {
    let text = concat!(
        "function add_one(value: i32): i32 { return value + 1; }\n",
        "export function choose(flag: bool, limit: i32): i32 {\n",
        "  let value: i32 = 0;\n",
        "  const step: i32 = 1;\n",
        "  while (value < limit) { value = value + step; }\n",
        "  if (flag) { return add_one(value); } else { return value; }\n",
        "}\n",
    );
    let (_, report) = analyze(text);
    assert_eq!(codes(&report), Vec::<&str>::new());
}

#[test]
fn invalid_control_flow_is_reported_from_m2_semantics() {
    let (_, immutable) =
        analyze("export function bad(): i32 { const value: i32 = 1; value = 2; return value; }");
    assert!(codes(&immutable).contains(&"ZRYNA-M2005"));
    let (_, unresolved) = analyze("export function bad(): i32 { return missing; }");
    assert!(codes(&unresolved).contains(&"ZRYNA-M2004"));
    let (_, wrong_type) =
        analyze("export function bad(): i32 { const value: i32 = true; return 1; }");
    assert!(codes(&wrong_type).contains(&"ZRYNA-M2006"));
}

#[test]
fn incomplete_source_has_ready_rejection() {
    let (_, report) = analyze("export function bad(value: i32): i32 { if (value < 1) {");
    assert!(!codes(&report).is_empty());
}

#[test]
fn unsupported_top_level_declaration_stays_rejected() {
    let (_, report) =
        analyze("const value: i32 = 1;\nexport function get(): i32 { return value; }");
    assert_eq!(codes(&report), ["ZRYNA-F1103"]);
    assert!(
        report["result"]["report"]["diagnostics"][0]["guidance"]
            .as_str()
            .expect("guidance")
            .contains("top-level")
    );
}

#[test]
fn foreign_syntax_or_entry_cannot_be_admitted() {
    let first = sources(PATH, "export function get(): i32 { return 1; }");
    let syntax = frontend().analyze_verified_v3(&first).expect("verified v3 syntax");
    let mut session = DiagnosticSession::try_new().expect("session");
    let entry = NormalizedSourcePath::new(PATH).expect("entry");
    let changed = sources(PATH, "export function get(): i32 { return 2; }");
    assert!(matches!(
        session.admit_control_flow_analysis(changed, &syntax, &entry),
        Err(DiagnosticSessionError::SemanticAuthority)
    ));
    let absent = NormalizedSourcePath::new("src/other.zry").expect("path");
    assert!(matches!(
        session.admit_control_flow_analysis(first, &syntax, &absent),
        Err(DiagnosticSessionError::SemanticAuthority)
    ));
    assert!(session.active_revision().is_none());
}

#[test]
fn switching_profiles_invalidates_pending_m2_work() {
    let (mut session, _) = analyze("export function value(): i32 { return 1; }");
    let revision = session.active_revision().expect("M2 revision");
    let now = Instant::now();
    let pending = session
        .begin_diagnostics(&request("old", revision, 100_000, "diagnostics", json!({})), now)
        .expect("M2 query");
    session
        .admit_diagnostics(sources(PATH, "export function value(): i32 { return 2; }"), &[])
        .expect("scalar replacement");
    assert_eq!(session.finish_diagnostics(pending, now).status(), QueryStatus::Stale);
}

#[test]
fn formatting_and_definition_respect_m2_semantics_and_revision() {
    let text =
        "export function value(flag:bool):i32{const x:i32=1;if(flag){return x;}else{return 0;}}";
    let (mut session, report) = analyze(text);
    assert!(codes(&report).is_empty());
    let revision = session.active_revision().expect("M2 revision");
    let edits = session.format_source(revision, PATH, None).expect("verified M2 formatting");
    assert_eq!(edits.len(), 1);
    assert!(edits[0].text.contains("const x: i32 = 1;"));
    assert_eq!(session.format_source(revision, PATH, Some(0..5)), Err(FormattingError::Range));

    let now = Instant::now();
    let pending = session
        .begin_definition(
            &request(
                "m2-definition",
                revision,
                100_000,
                "definition",
                json!({"path": PATH, "byte_offset": 16}),
            ),
            now,
        )
        .expect("bounded definition request");
    let definition = session.finish_definition(pending, now);
    assert_eq!(
        (definition.status(), definition.reason()),
        (QueryStatus::Unavailable, Some(QueryReason::Analysis))
    );

    let bad = sources(PATH, "export function bad(): i32 { const x: i32 = 1; x = 2; return x; }");
    let syntax = frontend().analyze_verified_v3(&bad).expect("invalid semantic syntax");
    let entry = NormalizedSourcePath::new(PATH).expect("entry");
    let bad_revision = session
        .admit_control_flow_analysis(bad, &syntax, &entry)
        .expect("ready semantic rejection");
    assert_eq!(session.format_source(bad_revision, PATH, None), Err(FormattingError::Unavailable));
    assert_eq!(session.format_source(revision, PATH, None), Err(FormattingError::Stale));
}
