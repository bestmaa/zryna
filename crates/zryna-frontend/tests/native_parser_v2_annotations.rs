//! Focused protocol-v2 multiple-annotation recovery evidence.

use std::fmt::Write as _;

use zryna_frontend::{
    native_lexer::lex,
    native_parser::{parse_v2_candidate, parse_v2_recovering_candidate},
    syntax_v2,
};
use zryna_source::{SourceFileInput, SourceMap};

const MULTIPLE_ANNOTATIONS_SOURCE: &str =
    include_str!("native_parser_v2/native-parser-v2-multiple-annotations.zry");
const MULTIPLE_ANNOTATIONS_BOOTSTRAP: &[u8] =
    include_bytes!("native_parser_v2/native-parser-v2-multiple-annotations.snapshot.json");

fn source(path: &str, text: &str) -> SourceFileInput {
    SourceFileInput { path: path.to_owned(), text: text.to_owned() }
}

#[test]
fn multiple_primitive_annotations_match_frozen_bootstrap_and_retain_sibling() {
    let sources = SourceMap::build(vec![source(
        "crates/zryna-frontend/tests/native_parser_v2/native-parser-v2-multiple-annotations.zry",
        MULTIPLE_ANNOTATIONS_SOURCE,
    )])
    .expect("bounded differential source");
    let lexed = lex(&sources).expect("bounded lexical stream");
    let bootstrap = syntax_v2::decode_snapshot(MULTIPLE_ANNOTATIONS_BOOTSTRAP)
        .expect("frozen provider snapshot");
    let strict = parse_v2_candidate(&sources, &lexed).expect_err("first unsupported annotation");
    assert_eq!(strict.diagnostic().code(), "ZRYNA-F2002");
    let native = parse_v2_recovering_candidate(&sources, &lexed).expect("bounded recovery");
    assert_eq!(native, bootstrap);
    assert_eq!(native.diagnostics.len(), 3);
    assert_eq!(native.files[0].functions[0].name.text, "retained");
    let verified = syntax_v2::verify_snapshot(native, &sources).expect("verified error snapshot");
    assert_eq!(verified.diagnostics().len(), 3);
}

#[test]
fn multiple_annotation_recovery_is_bounded_and_keeps_malformed_signatures_rejected() {
    for (count, accepted) in [
        (syntax_v2::MAX_PROVIDER_DIAGNOSTICS, true),
        (syntax_v2::MAX_PROVIDER_DIAGNOSTICS + 1, false),
    ] {
        let parameters = (0..count).map(|index| format!("p{index}: string")).collect::<Vec<_>>();
        let text = format!("export function f({}): i32 {{ return 1; }}", parameters.join(", "));
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let lexed = lex(&sources).expect("lexical stream");
        let result = parse_v2_recovering_candidate(&sources, &lexed);
        if accepted {
            let raw = result.expect("exact parameter and diagnostic limit");
            assert_eq!(raw.diagnostics.len(), count);
            assert!(raw.files[0].functions.is_empty());
            syntax_v2::verify_snapshot(raw, &sources).expect("verified error snapshot");
        } else {
            assert_eq!(
                result.expect_err("first extra parameter").diagnostic().code(),
                "ZRYNA-F1002"
            );
        }
    }

    let parameters = (0..syntax_v2::MAX_PARAMETERS_PER_FUNCTION)
        .map(|index| format!("p{index}: string"))
        .collect::<Vec<_>>();
    let text = format!("export function f({}): boolean {{ return true; }}", parameters.join(", "));
    let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
    let lexed = lex(&sources).expect("lexical stream");
    assert_eq!(
        parse_v2_recovering_candidate(&sources, &lexed)
            .expect("diagnostic truncation retains a rejected function")
            .diagnostics
            .last()
            .expect("terminal diagnostic")
            .code
            .as_str(),
        "ZRYNA-F2003"
    );

    let prior = "export function f(x: string): i32 { return 1; }\n";
    let text = format!(
        "{}export function g(x: string, y: number): i32 {{ return 1; }}",
        prior.repeat(syntax_v2::MAX_PROVIDER_DIAGNOSTICS - 1)
    );
    let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
    let lexed = lex(&sources).expect("lexical stream");
    assert_eq!(
        parse_v2_recovering_candidate(&sources, &lexed)
            .expect("project diagnostic truncation")
            .diagnostics
            .last()
            .expect("terminal diagnostic")
            .code
            .as_str(),
        "ZRYNA-F2003"
    );

    for signature in ["x: string y: number", "x: string | number", "x: string = 1"] {
        let text = format!(
            "export function rejected({signature}): boolean {{ return true; }}\n\
             export function retained(): i32 {{ return 1; }}"
        );
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        let lexed = lex(&sources).expect("lexical stream");
        assert!(parse_v2_candidate(&sources, &lexed).is_err());
        if let Ok(raw) = parse_v2_recovering_candidate(&sources, &lexed) {
            assert!(!raw.diagnostics.is_empty());
            assert!(raw.files[0].functions.iter().all(|function| function.name.text != "rejected"));
            syntax_v2::verify_snapshot(raw, &sources).expect("verified error snapshot");
        }
    }
}

#[test]
fn rejected_signature_counts_toward_the_exact_file_function_limit() {
    let rejected = "export function rejected(x: string): i32 { return 0; }\n";
    let mut siblings = String::new();
    for index in 0..syntax_v2::MAX_FUNCTIONS_PER_FILE - 1 {
        writeln!(&mut siblings, "export function f{index}(): i32 {{ return 1; }}")
            .expect("string write");
    }
    let exact = format!("{rejected}{siblings}export const ignored = 2;\n");
    let sources = SourceMap::build(vec![source("src/main.zry", &exact)]).expect("bounded source");
    let lexed = lex(&sources).expect("bounded lexical stream");
    let raw = parse_v2_recovering_candidate(&sources, &lexed).expect("exact function limit");
    assert_eq!(raw.files[0].functions.len(), syntax_v2::MAX_FUNCTIONS_PER_FILE - 1);
    assert_eq!(raw.diagnostics.len(), 2);
    syntax_v2::verify_snapshot(raw, &sources).expect("verified error snapshot");

    let first_extra =
        format!("{rejected}{siblings}export function overflow(): i32 {{ return 2; }}\n");
    let sources = SourceMap::build(vec![source("src/main.zry", &first_extra)])
        .expect("bounded first-extra source");
    let lexed = lex(&sources).expect("bounded lexical stream");
    assert_eq!(
        parse_v2_recovering_candidate(&sources, &lexed)
            .expect_err("rejected function consumes a slot")
            .diagnostic()
            .code(),
        "ZRYNA-F1002"
    );
}

#[test]
fn primitive_annotation_mutations_preserve_order_and_later_function() {
    let kinds = ["string", "number", "boolean", "unknown", "never", "symbol"];
    for first in kinds {
        for second in kinds {
            let text = format!(
                "// π\nexport function rejected(a: {first}, b: {second}): i32 {{ return 1; }}\n\
                 export function retained(): i32 {{ return 2; }}"
            );
            let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
            let lexed = lex(&sources).expect("lexical stream");
            let raw = parse_v2_recovering_candidate(&sources, &lexed).expect("bounded recovery");
            assert_eq!(raw.diagnostics.len(), 2);
            assert_eq!(raw.files[0].functions.len(), 1);
            assert_eq!(raw.files[0].functions[0].name.text, "retained");
            syntax_v2::verify_snapshot(raw, &sources).expect("verified error snapshot");
        }
    }
}
