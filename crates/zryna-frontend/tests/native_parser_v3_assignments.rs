//! Root-block assignment evidence against the pinned protocol-v3 worker.

use std::fmt::Write as _;

use zryna_frontend::{
    native_lexer::lex,
    native_parser::v3::{parse_v3_import_candidate, parse_v3_straight_line_candidate},
    syntax_v3,
};
use zryna_source::{SourceFileInput, SourceMap};

const MAIN: &str = include_str!("native_parser_v3_assignments/main.zry");
const MATH: &str = include_str!("native_parser_v3_assignments/math.zry");
const ASSIGNMENTS: &[u8] = include_bytes!("native_parser_v3_assignments/assignments.snapshot.json");
const CONST_ASSIGNMENT: &str = include_str!("native_parser_v3_assignments/const-assignment.zry");
const CONST_SNAPSHOT: &[u8] =
    include_bytes!("native_parser_v3_assignments/const-assignment.snapshot.json");
const REJECTED: &str = include_str!("native_parser_v3_assignments/rejected.zry");
const REJECTED_RESPONSE: &str = include_str!("native_parser_v3_assignments/rejected.response.json");

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
fn two_file_assignments_match_pinned_worker_and_verify() {
    let main = MAIN.replace("\r\n", "\n").replace('\n', "\r\n");
    let sources =
        SourceMap::build(vec![source("src/math.zry", MATH), source("src/main.zry", &main)])
            .expect("canonical sources from reversed input");
    let native = parse(&sources).expect("assignment candidate");
    let worker = syntax_v3::decode_snapshot(ASSIGNMENTS).expect("pinned TypeScript 6 snapshot");
    assert_eq!(native, worker);
    let bodies: Vec<_> =
        native.files.iter().flat_map(|file| &file.functions).map(|f| &f.body).collect();
    assert_eq!(bodies.iter().map(|body| body.statements.len()).sum::<usize>(), 11);
    assert_eq!(
        bodies
            .iter()
            .flat_map(|body| &body.statements)
            .filter(|statement| {
                matches!(&statement.kind, syntax_v3::RawStatementKind::Assignment { .. })
            })
            .count(),
        4
    );
    syntax_v3::verify_snapshot(native, &sources).expect("existing v3 verifier");
}

#[test]
fn assignment_to_const_remains_a_syntax_candidate() {
    let sources = SourceMap::build(vec![source("src/main.zry", CONST_ASSIGNMENT)]).expect("source");
    let native = parse(&sources).expect("semantic invalidity is not parser authority");
    let worker = syntax_v3::decode_snapshot(CONST_SNAPSHOT).expect("pinned worker snapshot");
    assert_eq!(native, worker);
    syntax_v3::verify_snapshot(native, &sources).expect("syntax verifies without semantic claim");
}

#[test]
fn unsupported_assignment_forms_match_worker_rejection_and_fail_atomically() {
    let worker: serde_json::Value = serde_json::from_str(REJECTED_RESPONSE).expect("response");
    assert_eq!(worker["error"]["code"], "ZRYNA-F2002");
    assert_eq!(one(REJECTED), Err("ZRYNA-F2002".to_owned()));
    for text in [
        "function f(): i32 { let x: i32 = 1; x.y = 2; return x; }",
        "function f(): i32 { let x: i32 = 1; [x] = [2]; return x; }",
        "function f(): i32 { let x: i32 = 1; x = 2 return x; }",
        "function f(): i32 { let x: i32 = 1; x = (2); return x; }",
        "function f(): i32 { let x: i32 = 1; x; return x; }",
    ] {
        assert_eq!(one(text), Err("ZRYNA-F2002".to_owned()), "{text}");
    }
}

#[test]
fn malformed_later_function_or_file_cannot_expose_assignments() {
    assert_eq!(
        one("function good(): i32 { x = 1; return x; } function bad(): i32 { x += 1; return x; }"),
        Err("ZRYNA-F2002".to_owned())
    );
    let sources = SourceMap::build(vec![
        source("a.zry", "function good(): i32 { x = 1; return x; }"),
        source("z.zry", "function bad(): i32 { x = (1); return x; }"),
    ])
    .expect("two sources");
    assert_eq!(parse(&sources), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn exact_and_first_extra_assignment_statement_limit() {
    for (assignments, accepted) in [
        (syntax_v3::MAX_STATEMENTS_PER_FUNCTION - 1, true),
        (syntax_v3::MAX_STATEMENTS_PER_FUNCTION, false),
    ] {
        let mut text = String::from("export function many(): i32 {\n");
        for _ in 0..assignments {
            writeln!(&mut text, "value = 1;").expect("write source");
        }
        text.push_str("return 1;\n}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let result = parse(&sources);
        if accepted {
            let native = result.expect("exact statement bound without local declarations");
            assert_eq!(native.files[0].functions[0].body.statements.len(), assignments + 1);
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
    let malformed =
        SourceMap::build(vec![source("src/main.zry", "function f(): i32 { x = @; return x; }")])
            .expect("malformed source");
    let lexed = lex(&malformed).expect("recoverable lexical stream");
    assert!(!lexed.diagnostics().is_empty());
    assert!(parse_v3_straight_line_candidate(&malformed, &lexed).is_err());
}

#[test]
fn import_only_entry_still_rejects_assignments_in_functions() {
    let sources = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("source");
    let lexed = lex(&sources).expect("tokens");
    assert!(parse_v3_import_candidate(&sources, &lexed).is_err());
}
