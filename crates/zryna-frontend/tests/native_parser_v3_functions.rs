//! Focused straight-line protocol-v3 candidate evidence against the pinned provider.

use std::fmt::Write as _;

use zryna_frontend::{
    native_lexer::lex,
    native_parser::v3::{parse_v3_import_candidate, parse_v3_straight_line_candidate},
    syntax_v3,
};
use zryna_source::{SourceFileInput, SourceMap};

const MAIN: &str = include_str!("native_parser_v3_functions/main.zry");
const MATH: &str = include_str!("native_parser_v3_functions/math.zry");
const BOOTSTRAP: &[u8] = include_bytes!("native_parser_v3_functions/functions.snapshot.json");
const COMPLETE_REQUEST: &str =
    include_str!("../../../tests/fixtures/typescript-adapter-v3-request.json");
const COMPLETE_RESULT: &[u8] =
    include_bytes!("../../../tests/fixtures/typescript-adapter-v3-result.json");
const REJECTED: &str = include_str!("native_parser_v3_functions/rejected.zry");
const REJECTED_RESPONSE: &str = include_str!("native_parser_v3_functions/rejected.response.json");

fn source(path: &str, text: &str) -> SourceFileInput {
    SourceFileInput { path: path.to_owned(), text: text.to_owned() }
}

fn candidate(text: &str) -> Result<syntax_v3::RawProjectSyntaxSnapshot, String> {
    let sources = SourceMap::build(vec![source("src/main.zry", text)]).expect("bounded source");
    let lexed = lex(&sources).expect("bounded lexical stream");
    parse_v3_straight_line_candidate(&sources, &lexed)
        .map_err(|error| error.diagnostic().code().to_owned())
}

#[test]
fn frozen_two_file_functions_match_bootstrap_and_verify() {
    let sources =
        SourceMap::build(vec![source("src/math.zry", MATH), source("src/main.zry", MAIN)])
            .expect("bounded reversed input");
    let lexed = lex(&sources).expect("bounded lexical project");
    let native = parse_v3_straight_line_candidate(&sources, &lexed).expect("v3 candidate");
    let bootstrap = syntax_v3::decode_snapshot(BOOTSTRAP).expect("pinned TypeScript 6 snapshot");
    assert_eq!(native, bootstrap);
    assert_eq!(native.files[0].imports.len(), 1);
    assert_eq!(native.files[0].functions.len(), 2);
    assert!(native.files[0].functions[0].export_span.is_none());
    assert!(native.files[0].functions[1].export_span.is_some());
    assert_eq!(native.files[1].functions.len(), 2);
    syntax_v3::verify_snapshot(native, &sources).expect("existing source-bound v3 verifier");
}

#[test]
fn existing_complete_oracle_import_and_helper_prefix_matches_exactly() {
    let request: serde_json::Value = serde_json::from_str(COMPLETE_REQUEST).expect("request");
    let text = request["params"]["files"][0]["text"].as_str().expect("source");
    let prefix = format!("{}\n", text.lines().take(2).collect::<Vec<_>>().join("\n"));
    let sources = SourceMap::build(vec![source("src/main.zry", &prefix)]).expect("prefix source");
    let lexed = lex(&sources).expect("prefix tokens");
    let native = parse_v3_straight_line_candidate(&sources, &lexed).expect("prefix candidate");
    let bootstrap = syntax_v3::decode_snapshot(COMPLETE_RESULT).expect("complete v3 oracle");
    assert_eq!(native.files[0].imports, bootstrap.files[0].imports);
    assert_eq!(native.files[0].functions, bootstrap.files[0].functions[..1]);
    syntax_v3::verify_snapshot(native, &sources).expect("prefix verifies");
}

#[test]
fn import_only_entry_still_rejects_functions() {
    let sources = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("source");
    let lexed = lex(&sources).expect("tokens");
    assert!(parse_v3_import_candidate(&sources, &lexed).is_err());
}

#[test]
fn parenthesized_return_matches_the_pinned_worker_rejection_class() {
    let response: serde_json::Value =
        serde_json::from_str(REJECTED_RESPONSE).expect("frozen worker response");
    assert_eq!(response["error"]["code"], "ZRYNA-F2002");
    assert_eq!(candidate(REJECTED), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn unsupported_or_later_tokens_reject_the_whole_project() {
    let prefix = "import { addPair } from './math.zry'; function helper(x: i32): i32 { return x; }";
    for tail in [
        " export const value = 1;",
        " export function broken(: i32): i32 { return 1; }",
        " import { later } from './later.zry';",
        " trailing",
        " function later(): i32 { return 1 }",
        " function later(): i32 { if (true) { return 1; } return 2; }",
        " function later(): i32 { let x: i32 = 1; x += 2; return x; }",
        " function later(): i32 { return helper(1); }",
    ] {
        assert_eq!(candidate(&format!("{prefix}{tail}")), Err("ZRYNA-F2002".to_owned()), "{tail}");
    }
    for text in [
        "export function f(): i32 { return (1); }",
        "export function f(): i32 { return -true; }",
        "export function f(): i32 { return 1 * 2; }",
        "export function f(): any { return 1; }",
        "export function f(x: i32): i32 { return x; return x; }",
        "export function f(x: i32,): i32 { return x; }",
    ] {
        assert_eq!(candidate(text), Err("ZRYNA-F2002".to_owned()), "{text}");
    }
}

#[test]
fn rejected_later_file_cannot_expose_earlier_functions() {
    let sources = SourceMap::build(vec![
        source("a.zry", "export function first(): i32 { return 1; }"),
        source("z.zry", "export function second(): i32 { return 2; } export const x = 3;"),
    ])
    .expect("source set");
    let lexed = lex(&sources).expect("lexical project");
    let error = parse_v3_straight_line_candidate(&sources, &lexed).expect_err("complete rejection");
    assert_eq!(error.diagnostic().code(), "ZRYNA-F2002");
}

#[test]
fn exact_and_first_extra_parameters_are_atomic() {
    for (count, accepted) in [
        (syntax_v3::MAX_PARAMETERS_PER_FUNCTION, true),
        (syntax_v3::MAX_PARAMETERS_PER_FUNCTION + 1, false),
    ] {
        let parameters = (0..count).map(|index| format!("p{index}: i32")).collect::<Vec<_>>();
        let text = format!("export function many({}): i32 {{ return 1; }}", parameters.join(", "));
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let lexed = lex(&sources).expect("tokens");
        let result = parse_v3_straight_line_candidate(&sources, &lexed);
        if accepted {
            let raw = result.expect("exact parameter boundary");
            assert_eq!(raw.files[0].functions[0].parameters.len(), count);
            syntax_v3::verify_snapshot(raw, &sources).expect("exact boundary verifies");
        } else {
            assert_eq!(
                result.expect_err("first extra parameter").diagnostic().code(),
                "ZRYNA-F1002"
            );
        }
    }
}

#[test]
fn exact_and_first_extra_functions_are_atomic() {
    for (count, accepted) in [
        (syntax_v3::MAX_FUNCTIONS_PER_MODULE, true),
        (syntax_v3::MAX_FUNCTIONS_PER_MODULE + 1, false),
    ] {
        let mut text = String::new();
        for index in 0..count {
            writeln!(&mut text, "export function f{index}(): i32 {{ return 1; }}")
                .expect("write to string");
        }
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let lexed = lex(&sources).expect("tokens");
        let result = parse_v3_straight_line_candidate(&sources, &lexed);
        if accepted {
            let raw = result.expect("exact function boundary");
            assert_eq!(raw.files[0].functions.len(), count);
            syntax_v3::verify_snapshot(raw, &sources).expect("exact boundary verifies");
        } else {
            assert_eq!(
                result.expect_err("first extra function").diagnostic().code(),
                "ZRYNA-F1002"
            );
        }
    }
}

#[test]
fn expression_depth_accepts_exact_and_rejects_first_extra() {
    for (count, accepted) in [
        (syntax_v3::MAX_NESTING_DEPTH as usize, true),
        (syntax_v3::MAX_NESTING_DEPTH as usize + 1, false),
    ] {
        let expression = vec!["1"; count].join(" + ");
        let text = format!("export function sum(): i32 {{ return {expression}; }}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let lexed = lex(&sources).expect("tokens");
        let result = parse_v3_straight_line_candidate(&sources, &lexed);
        if accepted {
            let raw = result.expect("exact depth");
            assert_eq!(raw.files[0].functions[0].body.expressions.len(), 2 * count - 1);
            syntax_v3::verify_snapshot(raw, &sources).expect("exact depth verifies");
        } else {
            assert_eq!(result.expect_err("first extra depth").diagnostic().code(), "ZRYNA-F1002");
        }
    }
}

#[test]
fn lexical_errors_and_foreign_source_maps_reject_before_construction() {
    let sources = SourceMap::build(vec![source("src/main.zry", "function f(): i32 { return 1; }")])
        .expect("source");
    let lexed = lex(&sources).expect("tokens");
    let unrelated =
        SourceMap::build(vec![source("src/main.zry", "function f(): i32 { return 1; }")])
            .expect("independent source authority");
    assert!(parse_v3_straight_line_candidate(&unrelated, &lexed).is_err());
    let malformed =
        SourceMap::build(vec![source("src/main.zry", "function f(): i32 { return @; }")])
            .expect("source");
    let lexed = lex(&malformed).expect("recoverable lexical stream");
    assert!(!lexed.diagnostics().is_empty());
    assert!(parse_v3_straight_line_candidate(&malformed, &lexed).is_err());
}
