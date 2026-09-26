//! Closed protocol-v2 candidate evidence against the frozen bootstrap snapshot.

use zryna_frontend::{
    native_lexer::lex,
    native_parser::{parse_v2_candidate, parse_v2_recovering_candidate},
    syntax_v2,
};
use zryna_source::{SourceFileInput, SourceMap};

const ADD_SOURCE: &str = include_str!("../../../examples/universal/add.zry");
const ADD_BOOTSTRAP: &[u8] =
    include_bytes!("../../../tests/fixtures/typescript-adapter-v2-result.json");
const BOOL_SOURCE: &str = include_str!("../../../tests/m1-fixtures/bool-gated.zry");
const ANY_SOURCE: &str = include_str!("../../../tests/m1-fixtures/invalid-any.zry");
const PUBLIC_M1_BOOTSTRAP: &[u8] =
    include_bytes!("native_parser_v2/native-parser-v2-public-m1.snapshot.json");
const M1_SOURCE: &str = include_str!("native_parser_v2/native-parser-v2-m1.zry");
const M1_BOOTSTRAP: &[u8] = include_bytes!("native_parser_v2/native-parser-v2-m1.snapshot.json");
const RETURN_NEWLINE_SOURCE: &str =
    include_str!("native_parser_v2/native-parser-v2-return-newline.zry");
const RETURN_NEWLINE_BOOTSTRAP: &[u8] =
    include_bytes!("native_parser_v2/native-parser-v2-return-newline.snapshot.json");
const RECOVERY_SOURCE: &str = include_str!("native_parser_v2/native-parser-v2-recovery.zry");
const RECOVERY_BOOTSTRAP: &[u8] =
    include_bytes!("native_parser_v2/native-parser-v2-recovery.snapshot.json");

fn source(path: &str, text: &str) -> SourceFileInput {
    SourceFileInput { path: path.to_owned(), text: text.to_owned() }
}

fn candidate(inputs: Vec<SourceFileInput>) -> (SourceMap, syntax_v2::RawProjectSyntaxSnapshot) {
    let sources = SourceMap::build(inputs).expect("bounded test source");
    let lexed = lex(&sources).expect("bounded lexical stream");
    let raw = parse_v2_candidate(&sources, &lexed).expect("closed native candidate");
    (sources, raw)
}

#[test]
fn frozen_bootstrap_add_snapshot_is_byte_span_equivalent() {
    let (sources, native) = candidate(vec![source("examples/universal/add.zry", ADD_SOURCE)]);
    let bootstrap = syntax_v2::decode_snapshot(ADD_BOOTSTRAP).expect("frozen provider snapshot");
    assert_eq!(native, bootstrap);
    let verified = syntax_v2::verify_snapshot(native, &sources).expect("existing verifier");
    assert!(verified.is_bound_to(&sources));
}

#[test]
fn complete_public_m1_source_corpus_matches_frozen_bootstrap_candidate() {
    let (sources, native) = candidate(vec![
        source("tests/m1-fixtures/invalid-any.zry", ANY_SOURCE),
        source("examples/universal/add.zry", ADD_SOURCE),
        source("tests/m1-fixtures/bool-gated.zry", BOOL_SOURCE),
    ]);
    let bootstrap =
        syntax_v2::decode_snapshot(PUBLIC_M1_BOOTSTRAP).expect("frozen public M1 snapshot");
    assert_eq!(native, bootstrap);
    syntax_v2::verify_snapshot(native, &sources).expect("source-bound public M1 candidate");
}

#[test]
fn frozen_bootstrap_missing_types_trailing_comma_and_asi_match() {
    let (sources, native) = candidate(vec![source(
        "crates/zryna-frontend/tests/native_parser_v2/native-parser-v2-m1.zry",
        M1_SOURCE,
    )]);
    let bootstrap = syntax_v2::decode_snapshot(M1_BOOTSTRAP).expect("frozen provider snapshot");
    assert_eq!(native, bootstrap);
    syntax_v2::verify_snapshot(native, &sources).expect("existing source-bound verifier");
}

#[test]
fn frozen_bootstrap_and_native_reject_return_newline_at_the_same_token() {
    let sources = SourceMap::build(vec![source(
        "crates/zryna-frontend/tests/native_parser_v2/native-parser-v2-return-newline.zry",
        RETURN_NEWLINE_SOURCE,
    )])
    .expect("bounded negative source");
    let lexed = lex(&sources).expect("bounded lexical stream");
    let native = parse_v2_candidate(&sources, &lexed).expect_err("newline ends return");
    let bootstrap =
        syntax_v2::decode_snapshot(RETURN_NEWLINE_BOOTSTRAP).expect("frozen provider result");
    assert!(bootstrap.files[0].functions.is_empty());
    assert_eq!(native.diagnostic().code(), bootstrap.diagnostics[0].code);
    let span = native.diagnostic().primary_span().expect("source-bound native error");
    assert_eq!(&RETURN_NEWLINE_SOURCE[span.start() as usize..span.end() as usize], "return");
    assert!(matches!(
        &bootstrap.diagnostics[0].location,
        syntax_v2::RawDiagnosticLocation::Source { span: provider }
            if provider.file == span.file().index()
                && provider.start == span.start()
                && provider.end == span.end()
    ));
}

#[test]
fn bounded_recovery_retains_the_same_following_function_as_bootstrap() {
    let sources = SourceMap::build(vec![source(
        "crates/zryna-frontend/tests/native_parser_v2/native-parser-v2-recovery.zry",
        RECOVERY_SOURCE,
    )])
    .expect("bounded recovery fixture");
    let lexed = lex(&sources).expect("bounded lexical stream");
    assert!(parse_v2_candidate(&sources, &lexed).is_err());
    let native = parse_v2_recovering_candidate(&sources, &lexed).expect("bounded recovery");
    let bootstrap = syntax_v2::decode_snapshot(RECOVERY_BOOTSTRAP).expect("frozen provider result");
    assert_eq!(native.files[0].functions, bootstrap.files[0].functions);
    assert_eq!(native.diagnostics.len(), 1);
    assert_eq!(native.diagnostics[0].code, bootstrap.diagnostics[0].code);
    let verified = syntax_v2::verify_snapshot(native, &sources).expect("existing verifier");
    assert_eq!(verified.diagnostics()[0].code(), "ZRYNA-F2002");
}

#[test]
fn recovery_never_promotes_nested_exports_to_top_level_functions() {
    for (open, close) in [("[", "]"), ("(", ")")] {
        let text = format!(
            "export const x = {open}export function phantom(): i32 {{ return 1; }}{close}; \
             export function retained(): i32 {{ return 2; }}"
        );
        let sources =
            SourceMap::build(vec![source("src/main.zry", &text)]).expect("bounded source");
        let lexed = lex(&sources).expect("bounded lexical stream");
        let raw = parse_v2_recovering_candidate(&sources, &lexed).expect("bounded recovery");
        assert_eq!(raw.diagnostics.len(), 1);
        assert_eq!(raw.files[0].functions.len(), 1);
        assert_eq!(raw.files[0].functions[0].name.text, "retained");
        syntax_v2::verify_snapshot(raw, &sources).expect("error snapshot remains verifiable");
    }
}

#[test]
fn mismatched_recovery_delimiters_cannot_promote_later_exports() {
    let text = "export const x = [); export function retained(): i32 { return 2; }";
    let sources = SourceMap::build(vec![source("src/main.zry", text)]).expect("bounded source");
    let lexed = lex(&sources).expect("bounded lexical stream");
    let raw = parse_v2_recovering_candidate(&sources, &lexed).expect("bounded recovery");
    assert_eq!(raw.diagnostics.len(), 1);
    assert!(raw.files[0].functions.is_empty());
    syntax_v2::verify_snapshot(raw, &sources).expect("error snapshot remains verifiable");
}

#[test]
fn first_extra_recovery_diagnostic_fails_atomically() {
    let rejected = "export function rejected(): i32 { return (1); }\n";
    let text = rejected.repeat(syntax_v2::MAX_PROVIDER_DIAGNOSTICS + 1);
    let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("bounded source");
    let lexed = lex(&sources).expect("bounded lexical stream");
    let error = parse_v2_recovering_candidate(&sources, &lexed)
        .expect_err("first-extra recovery diagnostic");
    assert_eq!(error.diagnostic().code(), "ZRYNA-F2003");
}

#[test]
fn canonical_postorder_and_file_order_survive_trivia_and_multiple_returns() {
    let (sources, native) = candidate(vec![
        source("z.zry", "export function z(): i32 { return -1; }"),
        source(
            "a.zry",
            "// heading\nexport function a(x: i32): i32 { return x + 1 + 2; return 0; }",
        ),
    ]);
    assert_eq!(
        native.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(),
        ["a.zry", "z.zry"]
    );
    let body = &native.files[0].functions[0].body;
    assert_eq!(body.statements.len(), 2);
    assert_eq!(body.expressions.len(), 6);
    syntax_v2::verify_snapshot(native, &sources).expect("source-bound postorder candidate");
}

#[test]
fn unsupported_and_ambiguous_constructs_fail_before_verification() {
    for input in [
        "function hidden(): i32 { return 1; }",
        "export default function f(): i32 { return 1; }",
        "export async function f(): i32 { return 1; }",
        "export function* f(): i32 { return 1; }",
        "export class Value {}",
        "export const value = 1;",
        "export function f({ x }: any): i32 { return 1; }",
        "export function f(...x: any): i32 { return 1; }",
        "export function f(x?: i32): i32 { return 1; }",
        "export function f(x: i32 = 1): i32 { return x; }",
        "export function f<T>(x: T): T { return x; }",
        "export function f(x: i32 | bool): i32 { return 1; }",
        "export function f(): i32 { return; }",
        "export function f(): i32 { if (true) return 1; return 2; }",
        "export function f(): i32 { return f(); }",
        "export function f(): i32 { return (1); }",
        "export function f(): i32 { return 1 * 2; }",
        "export function f(a: any): any { return a.value; }",
        "export function \\u0076alue(): i32 { return 1; }",
        "export const f = () => 1;",
        "export function f(): i32 { return 01; }",
        "export function f(): i32 { return - 1; }",
        "export function f(): i32 { return 1 + ; }",
        "export function f(x: unknown): i32 { return 1; }",
        "export function f(x: string): i32 { return 1; }",
        "export function f(this: i32): i32 { return 1; }",
        "export function f(): i32 { return null; }",
        "export function f(): i32 { return this; }",
        "export function f(): i32 { return 1 return 2; }",
        "export function f(): i32 { return\n1; }",
        "export function f(): i32 { return 1; } const extra = 2;",
    ] {
        let sources = SourceMap::build(vec![source("src/main.zry", input)]).expect("source");
        let lexed = lex(&sources).expect("lexical stream");
        assert!(parse_v2_candidate(&sources, &lexed).is_err(), "{input}");
    }
}

#[test]
fn exact_expression_depth_accepts_128_and_rejects_first_extra_129() {
    for (terms, expected_ok) in
        [(syntax_v2::MAX_EXPRESSION_DEPTH, true), (syntax_v2::MAX_EXPRESSION_DEPTH + 1, false)]
    {
        let expression = vec!["1"; terms as usize].join(" + ");
        let text = format!("export function f(): i32 {{ return {expression}; }}");
        let sources =
            SourceMap::build(vec![source("src/main.zry", &text)]).expect("bounded source");
        let lexed = lex(&sources).expect("bounded lexical stream");
        let result = parse_v2_candidate(&sources, &lexed);
        assert_eq!(result.is_ok(), expected_ok, "{terms} terms");
        if let Ok(raw) = result {
            syntax_v2::verify_snapshot(raw, &sources).expect("depth-bound candidate verifies");
        }
    }
}

#[test]
fn unsupported_token_reports_its_authoritative_span() {
    let text = "export function f(): i32 { return 1 * 2; }";
    let sources = SourceMap::build(vec![source("src/main.zry", text)]).expect("source");
    let lexed = lex(&sources).expect("lexical stream");
    let error = parse_v2_candidate(&sources, &lexed).expect_err("multiplication is outside v2");
    let span = error.diagnostic().primary_span().expect("bound diagnostic location");
    assert_eq!(&text[span.start() as usize..span.end() as usize], "*");
}

#[test]
fn lexical_errors_and_wrong_source_authority_fail_closed() {
    let left =
        SourceMap::build(vec![source("src/main.zry", "export function f(): i32 { return 1; }")])
            .expect("source");
    let right = left.clone();
    let unrelated =
        SourceMap::build(vec![source("src/main.zry", "export function f(): i32 { return 1; }")])
            .expect("independent source authority");
    let lexed = lex(&left).expect("lexical stream");
    assert!(parse_v2_candidate(&right, &lexed).is_ok());
    assert!(parse_v2_candidate(&unrelated, &lexed).is_err());

    let malformed =
        SourceMap::build(vec![source("src/main.zry", "export function f(): i32 { return @; }")])
            .expect("malformed source bytes remain valid UTF-8");
    let lexed = lex(&malformed).expect("recoverable lexical diagnostic");
    assert!(!lexed.diagnostics().is_empty());
    assert!(parse_v2_candidate(&malformed, &lexed).is_err());
}

#[test]
fn first_extra_function_exceeds_the_frozen_function_limit() {
    let function = "export function f(): i32 { return 1; }\n";
    let text = function.repeat(syntax_v2::MAX_FUNCTIONS_PER_FILE + 1);
    let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("bounded source");
    let lexed = lex(&sources).expect("bounded lexical inventory");
    let error = parse_v2_candidate(&sources, &lexed).expect_err("first-extra function");
    assert_eq!(error.diagnostic().code(), "ZRYNA-F2003");
}

#[test]
fn first_extra_parameter_exceeds_the_frozen_parameter_limit() {
    for (count, expected_ok) in [
        (syntax_v2::MAX_PARAMETERS_PER_FUNCTION, true),
        (syntax_v2::MAX_PARAMETERS_PER_FUNCTION + 1, false),
    ] {
        let parameters = (0..count).map(|index| format!("p{index}: i32")).collect::<Vec<_>>();
        let text = format!("export function f({}): i32 {{ return 1; }}", parameters.join(", "));
        let sources =
            SourceMap::build(vec![source("src/main.zry", &text)]).expect("bounded source");
        let lexed = lex(&sources).expect("bounded lexical stream");
        let result = parse_v2_candidate(&sources, &lexed);
        assert_eq!(result.is_ok(), expected_ok, "{count} parameters");
        if let Ok(raw) = result {
            syntax_v2::verify_snapshot(raw, &sources).expect("parameter-bound candidate verifies");
        }
    }
}
