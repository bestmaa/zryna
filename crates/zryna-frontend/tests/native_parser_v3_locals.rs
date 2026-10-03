//! Root-block local-declaration evidence against the pinned protocol-v3 worker.

use std::fmt::Write as _;

use zryna_frontend::{
    native_lexer::lex,
    native_parser::v3::{parse_v3_import_candidate, parse_v3_straight_line_candidate},
    syntax_v3,
};
use zryna_source::{SourceFileInput, SourceMap};

const MAIN: &str = include_str!("native_parser_v3_locals/main.zry");
const MATH: &str = include_str!("native_parser_v3_locals/math.zry");
const LOCALS: &[u8] = include_bytes!("native_parser_v3_locals/locals.snapshot.json");
const USE_BEFORE: &str =
    include_str!("../../../tests/m2-fixtures/invalid/use-before-declaration/main.zry");
const USE_BEFORE_SNAPSHOT: &[u8] =
    include_bytes!("native_parser_v3_locals/use-before.snapshot.json");
const REJECTED: &str = include_str!("native_parser_v3_locals/rejected.zry");
const REJECTED_RESPONSE: &str = include_str!("native_parser_v3_locals/rejected.response.json");

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
fn two_file_locals_match_pinned_worker_and_verify() {
    let sources =
        SourceMap::build(vec![source("src/math.zry", MATH), source("src/main.zry", MAIN)])
            .expect("canonical sources from reversed input");
    let native = parse(&sources).expect("local candidate");
    let worker = syntax_v3::decode_snapshot(LOCALS).expect("pinned TypeScript 6 snapshot");
    assert_eq!(native, worker);
    let bodies: Vec<_> =
        native.files.iter().flat_map(|file| &file.functions).map(|f| &f.body).collect();
    assert_eq!(bodies.iter().map(|body| body.statements.len()).sum::<usize>(), 6);
    assert_eq!(
        bodies
            .iter()
            .flat_map(|body| &body.statements)
            .filter(|statement| {
                matches!(&statement.kind, syntax_v3::RawStatementKind::LocalDeclaration { .. })
            })
            .count(),
        3
    );
    assert_eq!(bodies.iter().map(|body| body.expressions.len()).sum::<usize>(), 10);
    syntax_v3::verify_snapshot(native, &sources).expect("existing v3 verifier");
}

#[test]
fn frozen_use_before_declaration_is_syntax_only() {
    let sources = SourceMap::build(vec![source("src/main.zry", USE_BEFORE)]).expect("source");
    let native = parse(&sources).expect("unresolved name remains a syntax candidate");
    let worker =
        syntax_v3::decode_snapshot(USE_BEFORE_SNAPSHOT).expect("pinned worker syntax snapshot");
    assert_eq!(native, worker);
    syntax_v3::verify_snapshot(native, &sources).expect("source-bound syntax verifies");
}

#[test]
fn unsupported_local_forms_match_worker_rejection_class_and_fail_atomically() {
    let worker: serde_json::Value = serde_json::from_str(REJECTED_RESPONSE).expect("response");
    assert_eq!(worker["error"]["code"], "ZRYNA-F2002");
    assert_eq!(one(REJECTED), Err("ZRYNA-F2002".to_owned()));
    for text in [
        "function f(): i32 { let x: i32; return 1; }",
        "function f(): i32 { let x = 1; return x; }",
        "function f(): i32 { const x: i32 = 1 return x; }",
        "function f(): i32 { var x: i32 = 1; return x; }",
        "function f(): i32 { let x: i32 = 1 == 2; return x; }",
        "function f(): i32 { let x: i32 = 1; return x + (call(1)); }",
    ] {
        assert_eq!(one(text), Err("ZRYNA-F2002".to_owned()), "{text}");
    }
}

#[test]
fn local_errors_name_the_annotation_or_expression() {
    let text = "function f(): i32 { let x: i32 = (1); return x; }";
    let sources = SourceMap::build(vec![source("src/main.zry", text)]).expect("source");
    let lexed = lex(&sources).expect("tokens");
    let error = parse_v3_straight_line_candidate(&sources, &lexed).expect_err("local rejects");
    assert_eq!(error.diagnostic().code(), "ZRYNA-F2002");
    assert_eq!(
        error.diagnostic().message(),
        "expression uses unsupported syntax 'ParenthesizedExpression'"
    );
    let span = error.diagnostic().primary_span().expect("whole rejected expression");
    assert_eq!(&text[span.start() as usize..span.end() as usize], "(1)");
}

#[test]
fn later_bad_function_or_file_cannot_expose_earlier_locals() {
    assert_eq!(
        one(
            "function good(): i32 { let x: i32 = 1; return x; } function bad(): i32 { return (1); }"
        ),
        Err("ZRYNA-F2002".to_owned())
    );
    let sources = SourceMap::build(vec![
        source("a.zry", "function good(): i32 { let x: i32 = 1; return x; }"),
        source("z.zry", "function bad(): i32 { let x: i32 = 1; return (x); }"),
    ])
    .expect("two sources");
    assert_eq!(parse(&sources), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn import_only_entry_still_rejects_local_functions() {
    let sources = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("source");
    let lexed = lex(&sources).expect("tokens");
    assert!(parse_v3_import_candidate(&sources, &lexed).is_err());
}

#[test]
fn exact_and_first_extra_statement_limit_with_final_return() {
    for (locals, accepted) in [
        (syntax_v3::MAX_STATEMENTS_PER_FUNCTION - 1, true),
        (syntax_v3::MAX_STATEMENTS_PER_FUNCTION, false),
    ] {
        let mut text = String::from("export function many(): i32 {\n");
        for index in 0..locals {
            writeln!(&mut text, "const v{index}: i32 = 1;").expect("write source");
        }
        text.push_str("return 1;\n}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let result = parse(&sources);
        if accepted {
            let native = result.expect("exact statement bound");
            assert_eq!(native.files[0].functions[0].body.statements.len(), locals + 1);
            syntax_v3::verify_snapshot(native, &sources).expect("exact bound verifies");
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
    let malformed = SourceMap::build(vec![source(
        "src/main.zry",
        "function f(): i32 { let x: i32 = @; return x; }",
    )])
    .expect("malformed source");
    let lexed = lex(&malformed).expect("recoverable lexical stream");
    assert!(!lexed.diagnostics().is_empty());
    assert!(parse_v3_straight_line_candidate(&malformed, &lexed).is_err());
}
