//! Internal import-only protocol-v3 candidate evidence against the pinned provider.

use std::fmt::Write as _;

use zryna_frontend::{native_lexer::lex, native_parser::v3::parse_v3_import_candidate, syntax_v3};
use zryna_source::{SourceFileInput, SourceMap};

const A_PATH: &str = "crates/zryna-frontend/tests/native_parser_v3/imports-a.zry";
const Z_PATH: &str = "crates/zryna-frontend/tests/native_parser_v3/imports-z.zry";
const A_SOURCE: &str = include_str!("native_parser_v3/imports-a.zry");
const Z_SOURCE: &str = include_str!("native_parser_v3/imports-z.zry");
const BOOTSTRAP: &[u8] = include_bytes!("native_parser_v3/imports.snapshot.json");
const EXISTING_V3_REQUEST: &str =
    include_str!("../../../tests/fixtures/typescript-adapter-v3-request.json");
const EXISTING_V3_RESULT: &[u8] =
    include_bytes!("../../../tests/fixtures/typescript-adapter-v3-result.json");

fn source(path: &str, text: &str) -> SourceFileInput {
    SourceFileInput { path: path.to_owned(), text: text.to_owned() }
}

fn candidate(text: &str) -> (SourceMap, syntax_v3::RawProjectSyntaxSnapshot) {
    let sources = SourceMap::build(vec![source("src/main.zry", text)]).expect("bounded source");
    let lexed = lex(&sources).expect("bounded lexical stream");
    let raw = parse_v3_import_candidate(&sources, &lexed).expect("import-only candidate");
    (sources, raw)
}

#[test]
fn frozen_imports_match_bootstrap_in_file_and_source_order() {
    let sources = SourceMap::build(vec![source(Z_PATH, Z_SOURCE), source(A_PATH, A_SOURCE)])
        .expect("bounded reversed inputs");
    let lexed = lex(&sources).expect("bounded lexical stream");
    let native = parse_v3_import_candidate(&sources, &lexed).expect("import-only candidate");
    let bootstrap = syntax_v3::decode_snapshot(BOOTSTRAP).expect("frozen provider snapshot");
    assert_eq!(native, bootstrap);
    assert_eq!(native.files[0].imports.len(), 2);
    assert_eq!(native.files[0].imports[0].bindings[0].imported.text, "plus");
    assert_eq!(native.files[0].imports[0].bindings[0].local.text, "add");
    assert!(native.files[0].imports[0].bindings[0].as_span.is_some());
    assert_eq!(native.files[0].imports[0].bindings[1].local.text, "truth");
    assert!(native.files[0].imports[0].bindings[1].as_span.is_none());
    assert_eq!(native.files[0].imports[1].specifier.text, "../lib/logic.zry");
    assert!(native.files.iter().all(|file| file.functions.is_empty()));
    syntax_v3::verify_snapshot(native, &sources).expect("existing source-bound v3 verifier");
}

#[test]
fn import_projection_matches_the_existing_complete_v3_oracle() {
    let request: serde_json::Value =
        serde_json::from_str(EXISTING_V3_REQUEST).expect("frozen v3 request");
    let text = request["params"]["files"][0]["text"].as_str().expect("frozen source text");
    let import_line = text.lines().next().expect("leading import");
    let sources = SourceMap::build(vec![source("src/main.zry", import_line)])
        .expect("bounded import projection");
    let lexed = lex(&sources).expect("bounded lexical stream");
    let native = parse_v3_import_candidate(&sources, &lexed).expect("import-only projection");
    let bootstrap =
        syntax_v3::decode_snapshot(EXISTING_V3_RESULT).expect("frozen complete v3 snapshot");
    assert_eq!(native.files[0].imports, bootstrap.files[0].imports);
    syntax_v3::verify_snapshot(native, &sources).expect("projection remains verifiable");
}

#[test]
fn unsupported_or_trailing_tokens_never_yield_a_partial_import_candidate() {
    let first = "import { a } from \"./a.zry\";";
    for tail in [
        " export function f(): i32 { return 1; }",
        " export const value = 1;",
        " trailing",
        " import { b } from \"./b.zry\"",
        " import { b from \"./b.zry\";",
        " import b from \"./b.zry\";",
    ] {
        let text = format!("{first}{tail}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let lexed = lex(&sources).expect("lexical stream");
        assert!(parse_v3_import_candidate(&sources, &lexed).is_err(), "{tail}");
    }
    for text in [
        "import value from \"./value.zry\";",
        "import * as value from \"./value.zry\";",
        "import { } from \"./value.zry\";",
        "import { value } from \"value.zry\";",
        "import { type value } from \"./value.zry\";",
        "import { value } from \"./value.zry\"",
        "import { value } from \"./value\\u002ezry\";",
    ] {
        let sources = SourceMap::build(vec![source("src/main.zry", text)]).expect("source");
        let lexed = lex(&sources).expect("lexical stream");
        assert!(parse_v3_import_candidate(&sources, &lexed).is_err(), "{text}");
    }
}

#[test]
fn rejected_later_file_cannot_expose_earlier_imports() {
    let sources = SourceMap::build(vec![
        source("a.zry", "import { first } from './first.zry';"),
        source("z.zry", "import { second } from './second.zry'; export const extra = 1;"),
    ])
    .expect("bounded source set");
    let lexed = lex(&sources).expect("bounded lexical project");
    let error = parse_v3_import_candidate(&sources, &lexed).expect_err("complete project fails");
    assert_eq!(error.diagnostic().code(), "ZRYNA-F2002");
}

#[test]
fn exact_and_first_extra_imported_names_fail_atomically() {
    for (count, accepted) in [
        (syntax_v3::MAX_IMPORTED_NAMES_PER_DECLARATION, true),
        (syntax_v3::MAX_IMPORTED_NAMES_PER_DECLARATION + 1, false),
    ] {
        let names = (0..count).map(|index| format!("n{index}")).collect::<Vec<_>>();
        let text = format!("import {{ {} }} from \"./many.zry\";", names.join(", "));
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let lexed = lex(&sources).expect("lexical stream");
        let result = parse_v3_import_candidate(&sources, &lexed);
        if accepted {
            let raw = result.expect("exact imported-name limit");
            assert_eq!(raw.files[0].imports[0].bindings.len(), count);
            syntax_v3::verify_snapshot(raw, &sources).expect("exact-bound candidate verifies");
        } else {
            assert_eq!(result.expect_err("first extra name").diagnostic().code(), "ZRYNA-F1002");
        }
    }
}

#[test]
fn exact_and_first_extra_module_imports_fail_atomically() {
    for (count, accepted) in
        [(syntax_v3::MAX_IMPORTS_PER_MODULE, true), (syntax_v3::MAX_IMPORTS_PER_MODULE + 1, false)]
    {
        let mut text = String::new();
        for index in 0..count {
            writeln!(&mut text, "import {{ n{index} }} from \"./m{index}.zry\";")
                .expect("write to string");
        }
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let lexed = lex(&sources).expect("lexical stream");
        let result = parse_v3_import_candidate(&sources, &lexed);
        if accepted {
            let raw = result.expect("exact import limit");
            assert_eq!(raw.files[0].imports.len(), count);
            syntax_v3::verify_snapshot(raw, &sources).expect("exact-bound candidate verifies");
        } else {
            assert_eq!(result.expect_err("first extra import").diagnostic().code(), "ZRYNA-F1002");
        }
    }
}

#[test]
fn lexical_errors_and_foreign_source_maps_fail_before_candidate_construction() {
    let sources = SourceMap::build(vec![source("src/main.zry", "import { a } from './a.zry';")])
        .expect("source");
    let lexed = lex(&sources).expect("lexical stream");
    let unrelated = SourceMap::build(vec![source("src/main.zry", "import { a } from './a.zry';")])
        .expect("independent source authority");
    assert!(parse_v3_import_candidate(&unrelated, &lexed).is_err());
    let malformed = SourceMap::build(vec![source("src/main.zry", "import @;")]).expect("source");
    let lexed = lex(&malformed).expect("recoverable lexical stream");
    assert!(!lexed.diagnostics().is_empty());
    assert!(parse_v3_import_candidate(&malformed, &lexed).is_err());
}

#[test]
fn empty_source_has_no_omitted_tokens() {
    let (sources, raw) = candidate("// only trivia\n");
    assert!(raw.files[0].imports.is_empty());
    syntax_v3::verify_snapshot(raw, &sources).expect("empty v3 candidate verifies");
}
