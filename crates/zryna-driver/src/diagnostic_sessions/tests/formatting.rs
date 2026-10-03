use std::{ffi::OsString, path::PathBuf, process::Command};

use serde_json::Value;
use zryna_frontend::{
    FrontendCapabilities, ProviderExpectation, WorkerFrontend, WorkerLimits, WorkerSpec,
};

use super::sources;
use crate::diagnostic_sessions::{DiagnosticSession, FormattingError};

fn frontend() -> WorkerFrontend {
    let output =
        crate::process_spawn::output(Command::new("node").args(["-p", "process.execPath"]))
            .expect("node path");
    assert!(output.status.success());
    let node = PathBuf::from(String::from_utf8(output.stdout).expect("node UTF-8").trim());
    let expected = ProviderExpectation::new(
        "typescript-6",
        "6.0.3",
        2,
        FrontendCapabilities { module_resolution: false, semantic_diagnostics: false },
    )
    .expect("provider");
    WorkerFrontend::new(
        WorkerSpec::new(
            node,
            vec![OsString::from("src/worker.mjs")],
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adapters/typescript-6"),
            expected,
            WorkerLimits::default(),
        )
        .expect("worker"),
    )
}

fn erase_coordinates(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|key, _| {
                !key.ends_with("span") && key != "source_map_identity" && key != "source_map_id"
            });
            for value in object.values_mut() {
                erase_coordinates(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                erase_coordinates(value);
            }
        }
        _ => {}
    }
}

#[test]
fn formatting_goldens_are_idempotent_and_preserve_verified_meaning() {
    let frontend = frontend();
    let cases = [
        (
            "export   function sum( a:i32,b : i32 ) : i32{return a + b;}",
            "export function sum(a: i32, b: i32): i32 {\n  return a + b;\n}\n",
        ),
        (
            "// 🌱 preserved\r\nexport function f():i32 {return -2147483648}\r\n",
            "// 🌱 preserved\nexport function f(): i32 {\n  return -2147483648\n}\n",
        ),
        (
            "export function f(x:i32):i32{return/* exact  text */x+1;// end\r\n}",
            "export function f(x: i32): i32 {\n  return /* exact  text */ x + 1;\n  // end\n}\n",
        ),
        (
            "/* a\r\n b */export function f():i32{return 1+2+3;}\n\nexport function g():i32{return 4;}",
            "/* a\r\n b */ export function f(): i32 {\n  return 1 + 2 + 3;\n}\nexport function g(): i32 {\n  return 4;\n}\n",
        ),
    ];
    for (input, expected) in cases {
        let original = sources("src/main.zry", input);
        let syntax = crate::analyze_sources(&frontend, &original).expect("verified original");
        let original_ir =
            crate::lower_verified_syntax(&syntax, &original).expect("original semantics");
        let mut session = DiagnosticSession::try_new().expect("session");
        let revision = session.admit_analysis(original, &syntax).expect("admission");
        let edits = session.format_source(revision, "src/main.zry", None).expect("formatting");
        assert_eq!(edits.len(), 1);
        let actual = &edits[0].text;
        assert_eq!(actual, expected);
        let reformatted = sources("src/main.zry", actual);
        let reparsed = crate::analyze_sources(&frontend, &reformatted).expect("verified formatted");
        let new_ir =
            crate::lower_verified_syntax(&reparsed, &reformatted).expect("formatted semantics");
        assert_eq!(
            zryna_backend_javascript::emit(original_ir.program()).expect("original ESM"),
            zryna_backend_javascript::emit(new_ir.program()).expect("formatted ESM")
        );
        let mut before = serde_json::to_value(&syntax).expect("syntax value");
        let mut after = serde_json::to_value(&reparsed).expect("syntax value");
        erase_coordinates(&mut before);
        erase_coordinates(&mut after);
        assert_eq!(before, after, "verified syntax topology and literal/name identities");
        let next = session.admit_analysis(reformatted, &reparsed).expect("readmission");
        assert!(
            session.format_source(next, "src/main.zry", None).expect("second format").is_empty()
        );
        assert_eq!(
            session.format_source(revision, "src/main.zry", None),
            Err(FormattingError::Stale)
        );
    }
}

#[test]
fn formatting_rejects_malformed_unsupported_and_semantic_errors_without_edits() {
    let frontend = frontend();
    for input in [
        "export function f(",
        "class A {}",
        "export function f():i32{return missing;}",
        "export function f(x:i32):i32{if(x){return 1;}return 0;}",
        "export function f():String{return \"x\";}",
        "export function f():i32{return\n1;}",
        "import { a } from './a.zry';",
    ] {
        let map = sources("src/main.zry", input);
        let syntax = crate::analyze_sources(&frontend, &map).expect("provider diagnostics");
        let mut session = DiagnosticSession::try_new().expect("session");
        let revision = match session.admit_analysis(map.clone(), &syntax) {
            Ok(revision) => revision,
            Err(crate::diagnostic_sessions::DiagnosticSessionError::Diagnostics(_)) => {
                // The transport retains unavailable analysis when provider diagnostics cannot render.
                session.admit_unready(map).expect("unready admission")
            }
            Err(error) => panic!("unexpected admission: {error:?}"),
        };
        for _ in 0..2 {
            assert_eq!(
                session.format_source(revision, "src/main.zry", None),
                Err(FormattingError::Unavailable)
            );
        }
    }
}

#[test]
fn formatting_range_preserves_outside_bytes_and_rejects_partial_functions() {
    let input = "// 😀\r\nexport function f():i32{return 1;} /*keep*/ export function g():i32{return 2;}\r\n";
    let frontend = frontend();
    let map = sources("src/main.zry", input);
    let syntax = crate::analyze_sources(&frontend, &map).expect("syntax");
    let first = syntax.files()[0].functions()[0].span();
    let mut session = DiagnosticSession::try_new().expect("session");
    let revision = session.admit_analysis(map, &syntax).expect("admission");
    let edits = session
        .format_source(revision, "src/main.zry", Some(first.start()..first.end()))
        .expect("range");
    assert_eq!(edits.len(), 1);
    let edit = &edits[0];
    assert_eq!((edit.start, edit.end), (first.start(), first.end()));
    let output =
        format!("{}{}{}", &input[..edit.start as usize], edit.text, &input[edit.end as usize..]);
    assert!(output.starts_with("// 😀\r\nexport function f(): i32 {\n"));
    assert!(output.ends_with(" /*keep*/ export function g():i32{return 2;}\r\n"));
    for selected in [
        first.start() + 1..first.end(),
        first.start()..first.end() - 1,
        4..5,
        std::ops::Range { start: 20, end: 10 },
    ] {
        assert_eq!(
            session.format_source(revision, "src/main.zry", Some(selected)),
            Err(FormattingError::Range)
        );
    }
    assert!(
        session
            .format_source(revision, "src/main.zry", Some(0..0))
            .expect("empty range")
            .is_empty()
    );
    let point = first.start() + 1;
    assert!(
        session
            .format_source(revision, "src/main.zry", Some(point..point))
            .expect("empty selection inside a function")
            .is_empty()
    );
    assert_eq!(
        session.format_source(revision, "src/foreign.zry", None),
        Err(FormattingError::Unavailable)
    );
}

#[test]
fn formatting_result_limit_is_inclusive_and_foreign_sessions_reject() {
    let frontend = frontend();
    let function = "export function f(): i32 {\n  return 1;\n}\n";
    for length in [131_072, 131_073] {
        let text = format!("/*{}*/ {function}", "a".repeat(length - function.len() - 5));
        let map = sources("src/main.zry", &text);
        let syntax = crate::analyze_sources(&frontend, &map).expect("bounded source syntax");
        let mut session = DiagnosticSession::try_new().expect("session");
        let revision = session.admit_analysis(map, &syntax).expect("admission");
        let formatted = session.format_source(revision, "src/main.zry", None);
        if length == 131_072 {
            assert!(formatted.expect("exact limit").is_empty());
        } else {
            assert_eq!(formatted, Err(FormattingError::Limit));
        }
        let mut foreign = DiagnosticSession::try_new().expect("independent session");
        foreign.admit_unready(sources("src/main.zry", &text)).expect("foreign revision");
        assert_eq!(
            foreign.format_source(revision, "src/main.zry", None),
            Err(FormattingError::Stale)
        );
    }
}
