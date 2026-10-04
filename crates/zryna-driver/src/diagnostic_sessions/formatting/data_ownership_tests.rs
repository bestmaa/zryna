use std::{ffi::OsString, path::PathBuf, process::Command};

use serde_json::Value;
use zryna_frontend::{ProviderExpectationV4, WorkerFrontendV4, WorkerLimitsV4, WorkerSpecV4};
use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};

use super::{FormattingDocument, FormattingError};
use crate::diagnostic_sessions::DiagnosticSession;

fn frontend() -> WorkerFrontendV4 {
    let output =
        crate::process_spawn::output(Command::new("node").args(["-p", "process.execPath"]))
            .expect("node path");
    assert!(output.status.success());
    let node = PathBuf::from(String::from_utf8(output.stdout).expect("node UTF-8").trim());
    WorkerFrontendV4::new(
        WorkerSpecV4::new(
            node,
            vec![OsString::from("src/worker-v4.mjs")],
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adapters/typescript-6"),
            ProviderExpectationV4::new("typescript-6", "6.0.3").expect("provider"),
            WorkerLimitsV4::default(),
        )
        .expect("worker"),
    )
}

fn sources(files: &[(&str, &str)]) -> SourceMap {
    SourceMap::build(
        files
            .iter()
            .map(|(path, text)| SourceFileInput {
                path: (*path).to_owned(),
                text: (*text).to_owned(),
            })
            .collect(),
    )
    .expect("sources")
}

fn erase_coordinates(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|key, _| !key.ends_with("span") && key != "source_map_identity");
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
fn m3_multifile_formatting_is_idempotent_and_preserves_verified_syntax() {
    let input = sources(&[
        (
            "src/main.zry",
            "// 🌱 keep\r\nimport{double}from\"./math.zry\";\nexport function score(value:i32):i32{return double(value);}",
        ),
        (
            "src/math.zry",
            "interface Pair extends ZrynaStruct{left:String;right:i32;}\nexport function double(value:i32):i32{return value+value;}",
        ),
    ]);
    let frontend = frontend();
    let syntax = frontend.analyze_verified_v4(&input).expect("verified syntax");
    let entry = NormalizedSourcePath::new("src/main.zry").expect("entry");
    let semantic_input = zryna_semantics::data_ownership_v1::SemanticInput::try_new(
        &syntax,
        &input,
        input.file_id(&entry).expect("entry id"),
    )
    .expect("semantic input");
    zryna_semantics::data_ownership_v1::lower(semantic_input).expect("semantics");
    let plans = FormattingDocument::prepare_data_ownership(&syntax, &input).expect("plans");
    assert_eq!(plans.len(), 2);
    let output = sources(
        &plans
            .iter()
            .map(|plan| (plan.path.as_str(), plan.complete.as_deref().expect("formatted")))
            .collect::<Vec<_>>(),
    );
    let reparsed = frontend.analyze_verified_v4(&output).expect("reparsed syntax");
    let semantic_input = zryna_semantics::data_ownership_v1::SemanticInput::try_new(
        &reparsed,
        &output,
        output.file_id(&entry).expect("entry id"),
    )
    .expect("reparsed semantic input");
    zryna_semantics::data_ownership_v1::lower(semantic_input).expect("reparsed semantics");
    let mut before = serde_json::to_value(&syntax).expect("syntax JSON");
    let mut after = serde_json::to_value(&reparsed).expect("syntax JSON");
    erase_coordinates(&mut before);
    erase_coordinates(&mut after);
    assert_eq!(before, after, "verified topology and token identities");
    let again =
        FormattingDocument::prepare_data_ownership(&reparsed, &output).expect("second plans");
    for (first, second) in plans.iter().zip(&again) {
        assert_eq!(first.complete, second.complete);
    }
}

#[test]
fn m3_range_rejects_partial_declarations_and_keeps_other_files() {
    let input = sources(&[
        (
            "src/main.zry",
            "import { double } from \"./math.zry\";\nexport function score(value: i32): i32 { return double(value); }",
        ),
        ("src/math.zry", "export function double(value:i32):i32{return value+value;}"),
    ]);
    let syntax = frontend().analyze_verified_v4(&input).expect("syntax");
    let mut session = DiagnosticSession::try_new().expect("session");
    let revision = session
        .admit_data_ownership_analysis(
            input,
            &syntax,
            &NormalizedSourcePath::new("src/main.zry").expect("entry"),
        )
        .expect("admitted");
    assert_eq!(
        session.format_source(revision, "src/main.zry", Some(1..10)),
        Err(FormattingError::Range)
    );
    let other = session.format_source(revision, "src/math.zry", None).expect("other module edit");
    assert_eq!(other.len(), 1);
    assert!(other[0].text.contains("value + value"));
}

#[test]
fn m3_admitted_ownership_constructs_keep_topology_after_layout() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/m3-fixtures");
    let frontend = frontend();
    for fixture in [
        "pair-score-v4.zry",
        "owned-root-borrow-reads.zry",
        "fixed-array-subobject-clone-assignment.zry",
    ] {
        let text = std::fs::read_to_string(fixture_root.join(fixture)).expect("fixture");
        let original = sources(&[("src/main.zry", &text)]);
        let syntax = frontend.analyze_verified_v4(&original).expect("verified M3 fixture");
        let entry = original.verify_file_id(0).expect("entry");
        let input =
            zryna_semantics::data_ownership_v1::SemanticInput::try_new(&syntax, &original, entry)
                .expect("admitted fixture");
        zryna_semantics::data_ownership_v1::lower(input).expect("fixture semantics");
        let plan = FormattingDocument::prepare_data_ownership(&syntax, &original)
            .expect("plan")
            .pop()
            .expect("one file");
        let formatted = plan.complete.expect("formatted fixture");
        let result = sources(&[("src/main.zry", &formatted)]);
        let reparsed = frontend.analyze_verified_v4(&result).expect("reparsed fixture");
        let input = zryna_semantics::data_ownership_v1::SemanticInput::try_new(
            &reparsed,
            &result,
            result.verify_file_id(0).expect("entry"),
        )
        .expect("formatted semantic input");
        zryna_semantics::data_ownership_v1::lower(input).expect("formatted semantics");
        let mut before = serde_json::to_value(&syntax).expect("syntax JSON");
        let mut after = serde_json::to_value(&reparsed).expect("syntax JSON");
        erase_coordinates(&mut before);
        erase_coordinates(&mut after);
        assert_eq!(before, after, "{fixture}");
        let again = FormattingDocument::prepare_data_ownership(&reparsed, &result)
            .expect("second plan")
            .pop()
            .expect("one file");
        assert_eq!(again.complete, Ok(formatted), "{fixture}");
    }
}

#[test]
fn m3_document_limit_accepts_exact_cap_and_rejects_plus_one() {
    let maximum = crate::diagnostic_sessions::MAX_RESPONSE_BYTES / 8;
    let function = "export function f(): i32 {\n  return 1;\n}\n";
    let frontend = frontend();
    for length in [maximum, maximum + 1] {
        let text = format!("/*{}*/\n{function}", "x".repeat(length - function.len() - 5));
        assert_eq!(text.len(), length);
        let source = sources(&[("src/main.zry", &text)]);
        let syntax = frontend.analyze_verified_v4(&source).expect("syntax");
        let mut session = DiagnosticSession::try_new().expect("session");
        let revision = session
            .admit_data_ownership_analysis(
                source,
                &syntax,
                &NormalizedSourcePath::new("src/main.zry").expect("entry"),
            )
            .expect("semantic admission");
        let result = session.format_source(revision, "src/main.zry", None);
        if length == maximum {
            assert_eq!(result.expect("exact cap")[0].text.len(), maximum);
        } else {
            assert_eq!(result, Err(FormattingError::Limit));
        }
    }
}

#[test]
fn m3_two_file_setup_has_exact_canonical_layout() {
    let source = sources(&[
        (
            "main.zry",
            "import{double}from\"./math.zry\";export function score(value:i32):i32{return double(value);}",
        ),
        ("math.zry", "export function double(value:i32):i32{return value+value;}"),
    ]);
    let syntax = frontend().analyze_verified_v4(&source).expect("syntax");
    let plans = FormattingDocument::prepare_data_ownership(&syntax, &source).expect("plans");
    assert_eq!(
        plans[0].complete.as_deref(),
        Ok(
            "import {\n  double\n}\nfrom \"./math.zry\";\nexport function score(value: i32): i32 {\n  return double(value);\n}\n"
        )
    );
    assert_eq!(
        plans[1].complete.as_deref(),
        Ok("export function double(value: i32): i32 {\n  return value + value;\n}\n")
    );
    let mut session = DiagnosticSession::try_new().expect("session");
    session
        .admit_data_ownership_analysis(
            source,
            &syntax,
            &NormalizedSourcePath::new("main.zry").expect("entry"),
        )
        .expect("semantic admission");
}
