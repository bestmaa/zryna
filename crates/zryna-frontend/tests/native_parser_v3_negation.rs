//! Direct-identifier negation evidence against the pinned protocol-v3 worker.

use std::fmt::Write as _;

use zryna_frontend::{
    native_lexer::lex,
    native_parser::v3::{parse_v3_import_candidate, parse_v3_straight_line_candidate},
    syntax_v3,
};
use zryna_source::{SourceFileInput, SourceMap};

const MAIN: &str = include_str!("native_parser_v3_negation/main.zry");
const MATH: &str = include_str!("native_parser_v3_negation/math.zry");
const NEGATION: &[u8] = include_bytes!("native_parser_v3_negation/negation.snapshot.json");
const REJECTED: &str = include_str!("native_parser_v3_negation/rejected.zry");
const REJECTED_RESPONSE: &str = include_str!("native_parser_v3_negation/rejected.response.json");

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
fn two_file_identifier_negation_matches_pinned_worker_and_verifies() {
    let main = MAIN.replace("\r\n", "\n").replace('\n', "\r\n");
    let sources =
        SourceMap::build(vec![source("src/math.zry", MATH), source("src/main.zry", &main)])
            .expect("canonical sources from reversed input");
    let native = parse(&sources).expect("direct identifier negation candidate");
    let worker = syntax_v3::decode_snapshot(NEGATION).expect("pinned TypeScript 6 snapshot");
    assert_eq!(native, worker);
    let expressions = &native.files[0].functions[0].body.expressions;
    assert!(matches!(expressions[0].kind, syntax_v3::RawExpressionKind::Reference { .. }));
    assert!(matches!(
        expressions[1].kind,
        syntax_v3::RawExpressionKind::Negation { operand: 0, .. }
    ));
    let has_signed_literal = native
        .files
        .iter()
        .flat_map(|file| &file.functions)
        .flat_map(|function| &function.body.expressions)
        .any(|expression| {
            matches!(&expression.kind, syntax_v3::RawExpressionKind::I32Literal { spelling } if spelling == "-1")
        });
    assert!(has_signed_literal);
    syntax_v3::verify_snapshot(native, &sources).expect("existing v3 verifier");
}

#[test]
fn parenthesized_negation_remains_outside_the_native_candidate() {
    assert_eq!(one("function value(x: i32): i32 { return -(-x); }"), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn worker_rejected_prefix_decrement_is_pinned_separately() {
    let worker: serde_json::Value =
        serde_json::from_str(REJECTED_RESPONSE).expect("pinned worker response");
    assert_eq!(worker["error"]["code"], "ZRYNA-F2002");
    assert_eq!(one(REJECTED), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn negated_identifiers_obey_actual_depth_on_both_addition_sides() {
    let max = syntax_v3::MAX_NESTING_DEPTH as usize;
    for (atoms, accepted) in [(max - 1, true), (max, false)] {
        let text =
            format!("function sum(x: i32): i32 {{ return -x{}; }}", " + 1".repeat(atoms - 1));
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        if accepted {
            syntax_v3::verify_snapshot(parse(&sources).expect("exact depth"), &sources)
                .expect("exact depth verifies");
        } else {
            assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
        }
    }
    for (atoms, accepted) in [(max - 1, true), (max, false)] {
        let text =
            format!("function sum(x: i32): i32 {{ return 1 + -x{}; }}", " + 1".repeat(atoms - 2));
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        if accepted {
            syntax_v3::verify_snapshot(parse(&sources).expect("exact right-side depth"), &sources)
                .expect("exact right-side depth verifies");
        } else {
            assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
        }
    }
    for (atoms, accepted) in [(max, true), (max + 1, false)] {
        let expression = format!("{} + -x", vec!["1"; atoms - 1].join(" + "));
        let text = format!("function sum(x: i32): i32 {{ return {expression}; }}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        if accepted {
            syntax_v3::verify_snapshot(parse(&sources).expect("late right operand"), &sources)
                .expect("late right operand verifies");
        } else {
            assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
        }
    }
}

#[test]
fn negated_references_obey_exact_and_first_extra_expression_inventory() {
    // Each declaration contributes eight nodes; the final return contributes eight or nine.
    for (last, accepted) in [("-x + -x + -x", true), ("-x + -x + x + x", false)] {
        let mut text = String::from("function many(x: i32): i32 {\n");
        for index in 0..2_047 {
            writeln!(&mut text, "const value{index}: i32 = -x + -x + -x;").expect("write source");
        }
        writeln!(&mut text, "return {last};\n}}").expect("write source");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        if accepted {
            let native = parse(&sources).expect("exact expression inventory");
            assert_eq!(
                native.files[0].functions[0].body.expressions.len(),
                syntax_v3::MAX_EXPRESSIONS_PER_FUNCTION
            );
            syntax_v3::verify_snapshot(native, &sources).expect("exact inventory verifies");
        } else {
            assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
        }
    }
}

#[test]
fn later_malformed_function_or_file_cannot_expose_negation() {
    assert_eq!(
        one("function good(x: i32): i32 { return -x; } function bad(): i32 { return --1; }"),
        Err("ZRYNA-F2002".to_owned())
    );
    let sources = SourceMap::build(vec![
        source("a.zry", "function good(x: i32): i32 { return -x; }"),
        source("z.zry", "function bad(): i32 { return -(-1); }"),
    ])
    .expect("two sources");
    assert_eq!(parse(&sources), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn lexical_failure_and_foreign_source_map_are_atomic() {
    let sources = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("source");
    let lexed = lex(&sources).expect("tokens");
    let foreign = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("foreign map");
    assert!(parse_v3_straight_line_candidate(&foreign, &lexed).is_err());
    let malformed =
        SourceMap::build(vec![source("src/main.zry", "function f(x: i32): i32 { return -@; }")])
            .expect("malformed source");
    let lexed = lex(&malformed).expect("recoverable lexical stream");
    assert!(!lexed.diagnostics().is_empty());
    assert!(parse_v3_straight_line_candidate(&malformed, &lexed).is_err());
}

#[test]
fn import_only_entry_still_rejects_negated_functions() {
    let sources = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("source");
    let lexed = lex(&sources).expect("tokens");
    assert!(parse_v3_import_candidate(&sources, &lexed).is_err());
}
