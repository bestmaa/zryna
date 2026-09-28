//! Zero-argument call evidence against the pinned protocol-v3 worker.

use std::fmt::Write as _;

use zryna_frontend::{
    native_lexer::lex,
    native_parser::v3::{parse_v3_import_candidate, parse_v3_straight_line_candidate},
    syntax_v3,
};
use zryna_source::{SourceFileInput, SourceMap};

const MAIN: &str = include_str!("native_parser_v3_calls/main.zry");
const MATH: &str = include_str!("native_parser_v3_calls/math.zry");
const CALLS: &[u8] = include_bytes!("native_parser_v3_calls/calls.snapshot.json");
const UNRESOLVED: &str = include_str!("native_parser_v3_calls/unresolved.zry");
const UNRESOLVED_SNAPSHOT: &[u8] =
    include_bytes!("native_parser_v3_calls/unresolved.snapshot.json");
const REJECTED: &str = include_str!("native_parser_v3_calls/rejected.zry");
const REJECTED_RESPONSE: &str = include_str!("native_parser_v3_calls/rejected.response.json");
const ARGUMENTED: &str = include_str!("native_parser_v3_calls/argumented.zry");

fn source(path: &str, text: &str) -> SourceFileInput {
    SourceFileInput { path: path.to_owned(), text: text.to_owned() }
}

fn parse(sources: &SourceMap) -> Result<syntax_v3::RawProjectSyntaxSnapshot, String> {
    let lexed = lex(sources).expect("bounded lexical stream");
    parse_v3_straight_line_candidate(sources, &lexed)
        .map_err(|error| error.diagnostic().code().to_owned())
}

fn one(text: &str) -> Result<syntax_v3::RawProjectSyntaxSnapshot, String> {
    let sources = SourceMap::build(vec![source("src/main.zry", text)]).expect("bounded source");
    parse(&sources)
}

#[test]
fn two_file_calls_match_pinned_worker_and_verify() {
    let main = MAIN.replace("\r\n", "\n").replace('\n', "\r\n");
    let sources =
        SourceMap::build(vec![source("src/math.zry", MATH), source("src/main.zry", &main)])
            .expect("canonical sources from reversed input");
    let native = parse(&sources).expect("call candidate");
    let worker = syntax_v3::decode_snapshot(CALLS).expect("pinned TypeScript 6 snapshot");
    assert_eq!(native, worker);
    assert_eq!(
        native
            .files
            .iter()
            .flat_map(|file| &file.functions)
            .flat_map(|function| &function.body.expressions)
            .filter(|expression| matches!(
                &expression.kind,
                syntax_v3::RawExpressionKind::Call { .. }
            ))
            .count(),
        4
    );
    syntax_v3::verify_snapshot(native, &sources).expect("existing v3 verifier");
}

#[test]
fn unresolved_callee_remains_a_syntax_candidate() {
    let sources = SourceMap::build(vec![source("src/main.zry", UNRESOLVED)]).expect("source");
    let native = parse(&sources).expect("unresolved callee is a semantic question");
    let worker = syntax_v3::decode_snapshot(UNRESOLVED_SNAPSHOT).expect("pinned worker snapshot");
    assert_eq!(native, worker);
    syntax_v3::verify_snapshot(native, &sources).expect("syntax verifies without name resolution");
}

#[test]
fn unsupported_callees_match_worker_rejection_and_fail_atomically() {
    let worker: serde_json::Value = serde_json::from_str(REJECTED_RESPONSE).expect("response");
    assert_eq!(worker["error"]["code"], "ZRYNA-F2002");
    assert_eq!(one(REJECTED), Err("ZRYNA-F2002".to_owned()));
    for text in [
        "function f(): i32 { return object.method(); }",
        "function f(): i32 { return helper<i32>(); }",
        "function f(): i32 { return helper()(); }",
        "function f(): i32 { return helper(; }",
        "function f(): i32 { helper(); return 1; }",
        "function f(): i32 { return helper(); helper(); }",
    ] {
        assert_eq!(one(text), Err("ZRYNA-F2002".to_owned()), "{text}");
    }
}

#[test]
fn argumented_calls_are_syntax_candidates() {
    let sources = SourceMap::build(vec![source("src/main.zry", ARGUMENTED)]).expect("source");
    let native = parse(&sources).expect("direct argumented call");
    syntax_v3::verify_snapshot(native, &sources).expect("existing v3 verifier");
}

#[test]
fn malformed_later_function_or_file_cannot_expose_calls() {
    assert_eq!(
        one(
            "function good(): i32 { return helper(); } function bad(): i32 { return helper(1,,2); }"
        ),
        Err("ZRYNA-F2002".to_owned())
    );
    let sources = SourceMap::build(vec![
        source("a.zry", "function good(): i32 { return helper(); }"),
        source("z.zry", "function bad(): i32 { return (helper)(); }"),
    ])
    .expect("two sources");
    assert_eq!(parse(&sources), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn call_atoms_obey_exact_and_first_extra_addition_depth() {
    for (count, accepted) in [
        (syntax_v3::MAX_NESTING_DEPTH as usize, true),
        (syntax_v3::MAX_NESTING_DEPTH as usize + 1, false),
    ] {
        let expression = vec!["helper()"; count].join(" + ");
        let text = format!("export function sum(): i32 {{ return {expression}; }}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let result = parse(&sources);
        if accepted {
            let native = result.expect("exact depth");
            assert_eq!(native.files[0].functions[0].body.expressions.len(), 2 * count - 1);
            syntax_v3::verify_snapshot(native, &sources).expect("exact depth verifies");
        } else {
            assert_eq!(result, Err("ZRYNA-F1002".to_owned()));
        }
    }
}

#[test]
fn call_atoms_obey_exact_and_first_extra_expression_inventory() {
    for (extra, accepted) in [(3, true), (4, false)] {
        let mut text = String::from("export function many(): i32 {\n");
        for index in 0..2_340 {
            writeln!(
                &mut text,
                "const value{index}: i32 = helper() + helper() + helper() + helper();"
            )
            .expect("write source");
        }
        for index in 0..extra {
            writeln!(&mut text, "const extra{index}: i32 = helper();").expect("write source");
        }
        text.push_str("return helper();\n}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let result = parse(&sources);
        if accepted {
            let native = result.expect("exact expression inventory");
            assert_eq!(
                native.files[0].functions[0].body.expressions.len(),
                syntax_v3::MAX_EXPRESSIONS_PER_FUNCTION
            );
            syntax_v3::verify_snapshot(native, &sources).expect("exact inventory verifies");
        } else {
            assert_eq!(result, Err("ZRYNA-F1002".to_owned()));
        }
    }
}

#[test]
fn lexical_failure_and_foreign_source_map_are_atomic() {
    let sources = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("source");
    let lexed = lex(&sources).expect("tokens");
    let foreign = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("foreign map");
    assert!(parse_v3_straight_line_candidate(&foreign, &lexed).is_err());
    let malformed =
        SourceMap::build(vec![source("src/main.zry", "function f(): i32 { return helper(@); }")])
            .expect("malformed source");
    let lexed = lex(&malformed).expect("recoverable lexical stream");
    assert!(!lexed.diagnostics().is_empty());
    assert!(parse_v3_straight_line_candidate(&malformed, &lexed).is_err());
}

#[test]
fn import_only_entry_still_rejects_calls_in_functions() {
    let sources = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("source");
    let lexed = lex(&sources).expect("tokens");
    assert!(parse_v3_import_candidate(&sources, &lexed).is_err());
}
