//! Independent frozen v5 DTOs, not generated expectations or frontend activation.

use serde_json::Value;
use zryna_frontend::{native_lexer::lex, native_parser::v5::parse_v5_candidate};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v5;

#[path = "native_parser_v5/support.rs"]
mod support;
use support::{corpus, inputs, root};

fn source(text: &str) -> SourceMap {
    SourceMap::build(vec![SourceFileInput { path: "main.zry".into(), text: text.into() }])
        .expect("source")
}

fn candidate(sources: &SourceMap) -> v5::RawProjectSyntaxSnapshot {
    parse_v5_candidate(sources, &lex(sources).expect("lexical limits"))
        .expect("native v5 candidate")
}

#[test]
fn all_twelve_original_frozen_candidates_match_and_replay() {
    let corpus = corpus();
    let cases = corpus["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 12);
    for case in cases {
        let mut files = inputs(case);
        let sources = SourceMap::build(files.clone()).expect("source map");
        let raw = candidate(&sources);
        let expected: Value = serde_json::from_slice(
            &std::fs::read(root().join(case["reference"].as_str().expect("reference")))
                .expect("original DTO"),
        )
        .expect("DTO JSON");
        assert_eq!(serde_json::to_value(&raw).expect("candidate JSON"), expected, "{}", case["id"]);
        v5::verify_snapshot(raw.clone(), &sources).expect("mandatory independent verifier");
        files.reverse();
        let reversed = SourceMap::build(files).expect("canonical reversed inventory");
        assert_eq!(raw, candidate(&reversed), "deterministic raw replay {}", case["id"]);
        v5::verify_snapshot(candidate(&reversed), &reversed).expect("replayed verifier");
    }
}

#[test]
fn identical_bytes_in_another_source_map_are_not_lexical_authority() {
    let a = source("function identity<T extends ZrynaValue>(x: T): T { return x; }");
    let b = source(a.source(a.verify_file_id(0).expect("file")).expect("source").text());
    let tokens = lex(&a).expect("tokens");
    let error = parse_v5_candidate(&b, &tokens).expect_err("foreign identity");
    assert_eq!(error.diagnostic().code(), "ZRYNA-F2002");
    candidate(&a);
}

#[test]
fn malformed_or_excluded_generic_sources_reject_the_entire_candidate() {
    for text in [
        "interface Box<T extends ZrynaValue> extends ZrynaStruct { value: T }",
        "interface Choice<T extends ZrynaValue> extends ZrynaEnum { some: T }",
        "function f<T>(x: T): T { return x; }",
        "function f<T extends ZrynaValue = i32>(x: T): T { return x; }",
        "function f<T extends ZrynaValue, E extends ZrynaValue, U extends ZrynaValue>(): i32 { return 1; }",
        "function f(): i32 { return identity<i32, i32, i32>(1); }",
        "function f(): i32 { return identity<>(1); }",
        "function f<T extends ZrynaValue>(x: T): T { return\nx; }",
        "function f(): i32 { return 1 }",
        "function f(): i32 { return 1; } export class Unsupported {}",
        "function f(): i32 { return identity<i32>(1);",
    ] {
        let sources = source(text);
        let error =
            parse_v5_candidate(&sources, &lex(&sources).expect("lexable source")).expect_err(text);
        assert_eq!(error.diagnostic().code(), "ZRYNA-F2002", "{text}");
        assert_eq!(candidate(&source("function f(): i32 { return 1; }")).files.len(), 1);
    }
}

#[test]
fn unknown_bounds_are_preserved_for_the_owning_declaration_diagnostic() {
    let sources = source("function identity<T extends UnknownBound>(x: T): T { return x; }");
    let raw = candidate(&sources);
    assert_eq!(
        raw.files[0].functions[0].type_parameters.as_ref().expect("parameters").parameters[0]
            .bound
            .text,
        "UnknownBound"
    );
    let errors = v5::verify_snapshot(raw, &sources).expect_err("owning bound admission");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "ZRYNA-D7001");
}

#[test]
fn independently_forged_generic_ranges_and_arena_ownership_never_seal() {
    let sources = source(
        "function identity<T extends ZrynaValue>(x: T): T { return x; }\nfunction run(): i32 { return identity<i32>(7); }",
    );
    let raw = candidate(&sources);
    v5::verify_snapshot(raw.clone(), &sources).expect("native control");
    let mut foreign = raw.clone();
    foreign.files[0].functions[0].type_parameters.as_mut().expect("parameters").parameters[0]
        .name
        .span
        .file = 1;
    assert!(v5::verify_snapshot(foreign, &sources).is_err());
    let mut comma = raw.clone();
    comma.files[0].functions[0]
        .type_parameters
        .as_mut()
        .expect("parameters")
        .comma_spans
        .push(raw.files[0].functions[0].name.span);
    assert!(v5::verify_snapshot(comma, &sources).is_err());
    let mut orphan = raw;
    let extra = orphan.files[0].type_syntax[0].clone();
    orphan.files[0].type_syntax.push(extra);
    assert!(v5::verify_snapshot(orphan, &sources).is_err());
}
