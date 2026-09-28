//! Complete control-flow DTO evidence against the frozen TypeScript 6 worker.

use zryna_frontend::{
    native_lexer::lex, native_parser::v3::parse_v3_straight_line_candidate, syntax_v3,
};
use zryna_source::{SourceFileInput, SourceMap};

const POSITIVE_REQUEST: &str =
    include_str!("../../../tests/fixtures/m2-control-flow-positive-request.json");
const POSITIVE_RESULT: &[u8] =
    include_bytes!("../../../tests/fixtures/m2-control-flow-positive-result.json");
const NEGATIVE_REQUEST: &str =
    include_str!("../../../tests/fixtures/m2-control-flow-negative-request.json");
const NEGATIVE_RESULT: &[u8] =
    include_bytes!("../../../tests/fixtures/m2-control-flow-negative-result.json");
const STRAIGHT_NEGATIVE_REQUEST: &str =
    include_str!("../../../tests/fixtures/m2-semantic-negative-request.json");
const STRAIGHT_NEGATIVE_RESULT: &[u8] =
    include_bytes!("../../../tests/fixtures/m2-semantic-negative-result.json");
const COMPLETE_REQUEST: &str =
    include_str!("../../../tests/fixtures/typescript-adapter-v3-request.json");
const COMPLETE_RESULT: &[u8] =
    include_bytes!("../../../tests/fixtures/typescript-adapter-v3-result.json");
const UTF8_CRLF_FLOW: &str = include_str!("native_parser_v3_control_flow/flow.zry");
const UTF8_CRLF_RESULT: &[u8] = include_bytes!("native_parser_v3_control_flow/flow.snapshot.json");
const TYPE_FORMS: &str = include_str!("native_parser_v3_control_flow/types.zry");
const TYPE_FORMS_RESULT: &[u8] =
    include_bytes!("native_parser_v3_control_flow/types.snapshot.json");

fn sources(request: &str) -> SourceMap {
    let request: serde_json::Value = serde_json::from_str(request).expect("frozen request");
    let files = request["params"]["files"].as_array().expect("request files");
    SourceMap::build(
        files
            .iter()
            .rev()
            .map(|file| SourceFileInput {
                path: file["path"].as_str().expect("path").to_owned(),
                text: file["text"].as_str().expect("text").to_owned(),
            })
            .collect(),
    )
    .expect("source map")
}

fn assert_exact(request: &str, result: &[u8]) -> syntax_v3::RawProjectSyntaxSnapshot {
    let sources = sources(request);
    let lexed = lex(&sources).expect("bounded lexical stream");
    let native = parse_v3_straight_line_candidate(&sources, &lexed).expect("native candidate");
    let worker = syntax_v3::decode_snapshot(result).expect("frozen worker result");
    assert_eq!(native, worker);
    syntax_v3::verify_snapshot(native.clone(), &sources).expect("v3 verifier");
    native
}

fn parse_text(text: &str) -> Result<syntax_v3::RawProjectSyntaxSnapshot, String> {
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "src/main.zry".to_owned(),
        text: text.to_owned(),
    }])
    .expect("source map");
    let lexed = lex(&sources).expect("bounded lexical stream");
    parse_v3_straight_line_candidate(&sources, &lexed)
        .map_err(|error| error.diagnostic().code().to_owned())
}

#[test]
fn control_flow_positive_is_exact_frozen_worker_syntax() {
    let native = assert_exact(POSITIVE_REQUEST, POSITIVE_RESULT);
    assert_eq!(native.files[0].functions.len(), 6);
    assert_eq!(native.files[0].functions[5].body.blocks.len(), 4);
}

#[test]
fn semantic_negative_control_flow_is_still_exact_syntax() {
    let native = assert_exact(NEGATIVE_REQUEST, NEGATIVE_RESULT);
    assert_eq!(native.files[0].functions.len(), 4);
    assert!(native.diagnostics.is_empty());
}

#[test]
fn semantic_negative_straight_line_is_still_exact_syntax() {
    let native = assert_exact(STRAIGHT_NEGATIVE_REQUEST, STRAIGHT_NEGATIVE_RESULT);
    assert!(native.diagnostics.is_empty());
}

#[test]
fn complete_two_file_v3_fixture_is_exact_frozen_worker_syntax() {
    let native = assert_exact(COMPLETE_REQUEST, COMPLETE_RESULT);
    assert_eq!(native.files.len(), 2);
    assert_eq!(native.files[0].functions[1].body.blocks.len(), 5);
}

#[test]
fn utf8_crlf_branches_loop_and_child_return_have_exact_worker_spans() {
    let text = UTF8_CRLF_FLOW.replace("\r\n", "\n").replace('\n', "\r\n");
    let sources = SourceMap::build(vec![SourceFileInput { path: "src/flow.zry".to_owned(), text }])
        .expect("source map");
    let lexed = lex(&sources).expect("bounded lexical stream");
    let native = parse_v3_straight_line_candidate(&sources, &lexed).expect("native candidate");
    let worker = syntax_v3::decode_snapshot(UTF8_CRLF_RESULT).expect("frozen worker result");
    assert_eq!(native, worker);
    syntax_v3::verify_snapshot(native, &sources).expect("v3 verifier");
}

#[test]
fn missing_and_named_type_syntax_and_trailing_parameter_comma_match_worker() {
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "src/types.zry".to_owned(),
        text: TYPE_FORMS.to_owned(),
    }])
    .expect("source map");
    let lexed = lex(&sources).expect("bounded lexical stream");
    let native = parse_v3_straight_line_candidate(&sources, &lexed).expect("native candidate");
    let worker = syntax_v3::decode_snapshot(TYPE_FORMS_RESULT).expect("frozen worker result");
    assert_eq!(native, worker);
    syntax_v3::verify_snapshot(native, &sources).expect("v3 verifier");
}

#[test]
fn malformed_control_flow_rejects_the_whole_candidate() {
    for body in [
        "if (true) return 1;",
        "if (true) {} else return 1;",
        "if (true) {} else if (false) {}",
        "while (true) return 1;",
        "while (true) { return; }",
        "if () {}",
        "if (true) { return (1); }",
    ] {
        let text = format!("function f(): i32 {{ {body} }}");
        assert_eq!(parse_text(&text), Err("ZRYNA-F2002".to_owned()), "{body}");
    }
    let text = "function good(): i32 { if (true) { return 1; } return 2; } \
                function bad(): i32 { while (true) { return (3); } }";
    assert_eq!(parse_text(text), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn mixed_block_condition_and_return_depth_match_worker_boundaries() {
    for (children, code) in [(126, None), (127, Some("ZRYNA-F1002"))] {
        let body = format!("{}if (true) {{}}{}", "{".repeat(children), "}".repeat(children));
        let text = format!("function f(): i32 {{ {body} }}");
        if let Some(code) = code {
            assert_eq!(parse_text(&text), Err(code.to_owned()));
        } else {
            assert_eq!(
                parse_text(&text).expect("exact nested condition").files[0].functions[0]
                    .body
                    .blocks
                    .len(),
                128
            );
        }
    }
    let body = format!("{}return 1;{}", "{".repeat(127), "}".repeat(127));
    let text = format!("function f(): i32 {{ {body} }}");
    assert_eq!(parse_text(&text), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn blocks_and_statements_have_independent_exact_function_limits() {
    for (children, accepted) in [(4_095, true), (4_096, false)] {
        let text = format!("function f(): i32 {{ {} }}", "{}".repeat(children));
        if accepted {
            let native = parse_text(&text).expect("exact blocks");
            let body = &native.files[0].functions[0].body;
            assert_eq!(body.blocks.len(), syntax_v3::MAX_BLOCKS_PER_FUNCTION);
            assert_eq!(body.statements.len(), children);
        } else {
            assert_eq!(parse_text(&text), Err("ZRYNA-F1002".to_owned()));
        }
    }
    for (returns, accepted) in [(4_096, true), (4_097, false)] {
        let text = format!("function f(): i32 {{ {} }}", "return 1;".repeat(returns));
        if accepted {
            let native = parse_text(&text).expect("exact statements");
            let body = &native.files[0].functions[0].body;
            assert_eq!(body.blocks.len(), 1);
            assert_eq!(body.statements.len(), syntax_v3::MAX_STATEMENTS_PER_FUNCTION);
        } else {
            assert_eq!(parse_text(&text), Err("ZRYNA-F1002".to_owned()));
        }
    }
}
