//! Closed protocol-v2 candidate evidence against the frozen bootstrap snapshot.

use zryna_frontend::{native_lexer::lex, native_parser::parse_v2_candidate, syntax_v2};
use zryna_source::{SourceFileInput, SourceMap};

const ADD_SOURCE: &str = include_str!("../../../examples/universal/add.zry");
const ADD_BOOTSTRAP: &[u8] =
    include_bytes!("../../../tests/fixtures/typescript-adapter-v2-result.json");

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
        "export function f(): i32 { return (1); }",
        "export function f(): i32 { return 1 * 2; }",
        "export function f(): i32 { return 01; }",
        "export function f(): i32 { return - 1; }",
        "export function f(): i32 { return 1 + ; }",
        "export function f(): i32 { return 1; } const extra = 2;",
    ] {
        let sources = SourceMap::build(vec![source("src/main.zry", input)]).expect("source");
        let lexed = lex(&sources).expect("lexical stream");
        assert!(parse_v2_candidate(&sources, &lexed).is_err(), "{input}");
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
