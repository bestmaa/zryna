//! Compact signed-literal evidence against the pinned protocol-v3 worker.

use std::fmt::Write as _;

use zryna_frontend::{
    native_lexer::lex,
    native_parser::v3::{parse_v3_import_candidate, parse_v3_straight_line_candidate},
    syntax_v3,
};
use zryna_source::{SourceFileInput, SourceMap};

const MAIN: &str = include_str!("native_parser_v3_signed/main.zry");
const MATH: &str = include_str!("native_parser_v3_signed/math.zry");
const SIGNED: &[u8] = include_bytes!("native_parser_v3_signed/signed.snapshot.json");

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
fn two_file_signed_literals_match_pinned_worker_and_verify() {
    let main = MAIN.replace("\r\n", "\n").replace('\n', "\r\n");
    let sources =
        SourceMap::build(vec![source("src/math.zry", MATH), source("src/main.zry", &main)])
            .expect("canonical sources from reversed input");
    let native = parse(&sources).expect("signed literal candidate");
    let worker = syntax_v3::decode_snapshot(SIGNED).expect("pinned TypeScript 6 snapshot");
    assert_eq!(native, worker);
    let signed: Vec<_> = native
        .files
        .iter()
        .flat_map(|file| &file.functions)
        .flat_map(|function| &function.body.expressions)
        .filter_map(|expression| match &expression.kind {
            syntax_v3::RawExpressionKind::I32Literal { spelling } if spelling.starts_with('-') => {
                Some(spelling.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(signed, ["-2147483648", "-1", "-2", "-3", "-4"]);
    syntax_v3::verify_snapshot(native, &sources).expect("existing v3 verifier");
}

#[test]
fn noncanonical_signed_forms_remain_outside_this_candidate() {
    for expression in ["-01", "-00", "+1", "--1", "(-1)"] {
        let text = format!("function value(x: i32): i32 {{ return {expression}; }}");
        assert_eq!(one(&text), Err("ZRYNA-F2002".to_owned()), "{expression}");
    }
}

#[test]
fn signed_spelling_uses_one_node_at_64_two_at_65_and_rejects_66() {
    for (bytes, expression_count) in [
        (syntax_v3::MAX_LITERAL_BYTES, Some(1)),
        (syntax_v3::MAX_LITERAL_BYTES + 1, Some(2)),
        (syntax_v3::MAX_LITERAL_BYTES + 2, None),
    ] {
        let spelling = format!("-{}", "1".repeat(bytes - 1));
        let text = format!("function value(): i32 {{ return {spelling}; }}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let result = parse(&sources);
        if let Some(expression_count) = expression_count {
            let native = result.expect("exact signed spelling");
            assert_eq!(native.files[0].functions[0].body.expressions.len(), expression_count);
            syntax_v3::verify_snapshot(native, &sources).expect("exact spelling verifies");
        } else {
            assert_eq!(result, Err("ZRYNA-F2002".to_owned()));
        }
    }
}

#[test]
fn malformed_later_function_or_file_cannot_expose_signed_literals() {
    assert_eq!(
        one("function good(): i32 { return -1; } function bad(): i32 { return -01; }"),
        Err("ZRYNA-F2002".to_owned())
    );
    let sources = SourceMap::build(vec![
        source("a.zry", "function good(): i32 { return -1; }"),
        source("z.zry", "function bad(): i32 { return -01; }"),
    ])
    .expect("two sources");
    assert_eq!(parse(&sources), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn signed_atoms_obey_exact_and_first_extra_addition_depth() {
    for (count, accepted) in [
        (syntax_v3::MAX_NESTING_DEPTH as usize, true),
        (syntax_v3::MAX_NESTING_DEPTH as usize + 1, false),
    ] {
        let expression = vec!["-1"; count].join(" + ");
        let text = format!("function sum(): i32 {{ return {expression}; }}");
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
fn signed_atoms_obey_exact_and_first_extra_expression_inventory() {
    for (extra, accepted) in [(3, true), (4, false)] {
        let mut text = String::from("function many(): i32 {\n");
        for index in 0..2_340 {
            writeln!(&mut text, "const value{index}: i32 = -1 + -1 + -1 + -1;")
                .expect("write source");
        }
        for index in 0..extra {
            writeln!(&mut text, "const extra{index}: i32 = -1;").expect("write source");
        }
        text.push_str("return -1;\n}");
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
        SourceMap::build(vec![source("src/main.zry", "function f(): i32 { return -@; }")])
            .expect("malformed source");
    let lexed = lex(&malformed).expect("recoverable lexical stream");
    assert!(!lexed.diagnostics().is_empty());
    assert!(parse_v3_straight_line_candidate(&malformed, &lexed).is_err());
}

#[test]
fn import_only_entry_still_rejects_signed_functions() {
    let sources = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("source");
    let lexed = lex(&sources).expect("tokens");
    assert!(parse_v3_import_candidate(&sources, &lexed).is_err());
}
