//! Standalone lexical-block evidence against the pinned protocol-v3 worker.

use zryna_frontend::{
    native_lexer::lex,
    native_parser::v3::{parse_v3_import_candidate, parse_v3_straight_line_candidate},
    syntax_v3,
};
use zryna_source::{SourceFileInput, SourceMap};

const REQUEST: &str = include_str!("../../../tests/fixtures/m2-straight-line-request.json");
const SNAPSHOT: &[u8] = include_bytes!("../../../tests/fixtures/m2-straight-line-result.json");
const MAIN: &str = include_str!("native_parser_v3_blocks/main.zry");
const MATH: &str = include_str!("native_parser_v3_blocks/math.zry");
const BLOCK_SNAPSHOT: &[u8] = include_bytes!("native_parser_v3_blocks/blocks.snapshot.json");
const REJECTED: &str = include_str!("native_parser_v3_blocks/rejected.zry");
const REJECTED_RESPONSE: &str = include_str!("native_parser_v3_blocks/rejected.response.json");

fn source(path: &str, text: &str) -> SourceFileInput {
    SourceFileInput { path: path.to_owned(), text: text.to_owned() }
}

fn parse(sources: &SourceMap) -> Result<syntax_v3::RawProjectSyntaxSnapshot, String> {
    let lexed = lex(sources).expect("bounded lexical stream");
    parse_v3_straight_line_candidate(sources, &lexed)
        .map_err(|error| error.diagnostic().code().to_owned())
}

fn one(body: &str) -> Result<syntax_v3::RawProjectSyntaxSnapshot, String> {
    let text = format!("function value(): i32 {{ {body} }}");
    let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
    parse(&sources)
}

#[test]
fn complete_two_file_m2_straight_line_snapshot_matches_pinned_worker() {
    let request: serde_json::Value = serde_json::from_str(REQUEST).expect("pinned request");
    let files = request["params"]["files"].as_array().expect("request files");
    let sources = SourceMap::build(
        files
            .iter()
            .rev()
            .map(|file| {
                source(
                    file["path"].as_str().expect("file path"),
                    file["text"].as_str().expect("file text"),
                )
            })
            .collect(),
    )
    .expect("reversed source-map input");
    let native = parse(&sources).expect("complete straight-line candidate");
    let worker = syntax_v3::decode_snapshot(SNAPSHOT).expect("pinned TypeScript 6 snapshot");
    assert_eq!(native, worker);
    let body = &native.files[0]
        .functions
        .iter()
        .find(|function| function.name.text == "evaluate")
        .expect("evaluate function")
        .body;
    assert_eq!(body.blocks.len(), 2);
    assert_eq!(body.statements.len(), 21);
    assert_eq!(body.expressions.len(), 53);
    syntax_v3::verify_snapshot(native, &sources).expect("existing v3 verifier");
}

#[test]
fn utf8_crlf_nested_and_empty_blocks_match_pinned_worker() {
    let main = MAIN.replace("\r\n", "\n").replace('\n', "\r\n");
    let sources =
        SourceMap::build(vec![source("src/math.zry", MATH), source("src/main.zry", &main)])
            .expect("reversed source-map input");
    let native = parse(&sources).expect("lexical blocks candidate");
    let worker = syntax_v3::decode_snapshot(BLOCK_SNAPSHOT).expect("pinned block snapshot");
    assert_eq!(native, worker);
    let body = &native.files[0].functions[0].body;
    assert_eq!(body.blocks.len(), 4);
    assert_eq!(body.statements.len(), 9);
    assert_eq!(body.expressions.len(), 11);
    syntax_v3::verify_snapshot(native, &sources).expect("existing v3 verifier");
}

#[test]
fn malformed_and_unsupported_child_statements_reject_whole_candidate() {
    let worker: serde_json::Value =
        serde_json::from_str(REJECTED_RESPONSE).expect("pinned worker rejection");
    assert_eq!(worker["error"]["code"], "ZRYNA-F2002");
    let sources = SourceMap::build(vec![source("src/rejected.zry", REJECTED)]).expect("source");
    assert_eq!(parse(&sources), Err("ZRYNA-F2002".to_owned()));
    for body in [
        "if (true) return 1;",
        "while (true) return 1;",
        "if (true) {} else return 1;",
        "{} return (1);",
        "{} break; return 1;",
    ] {
        assert_eq!(one(body), Err("ZRYNA-F2002".to_owned()), "{body}");
    }
    let sources = SourceMap::build(vec![source(
        "src/main.zry",
        "function value(): i32 { { let x: i32 = @; } return 1; }",
    )])
    .expect("source");
    let lexed = lex(&sources).expect("recoverable lexical stream");
    assert!(!lexed.diagnostics().is_empty());
    assert!(parse_v3_straight_line_candidate(&sources, &lexed).is_err());
}

#[test]
fn brace_only_nesting_accepts_worker_128_and_rejects_129() {
    for (depth, accepted) in [(128, true), (129, false)] {
        let children = depth - 1;
        let body = format!("{}{} return 1;", "{".repeat(children), "}".repeat(children));
        let text = format!("function value(): i32 {{ {body} }}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        if accepted {
            let native = parse(&sources).expect("root-inclusive exact block depth");
            assert_eq!(native.files[0].functions[0].body.blocks.len(), depth);
            syntax_v3::verify_snapshot(native, &sources).expect("exact depth verifies");
        } else {
            assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
        }
    }
}

#[test]
fn sibling_blocks_and_statements_share_exact_and_first_extra_function_limit() {
    for (children, accepted) in [(4_095, true), (4_096, false)] {
        let text = format!("function value(): i32 {{ {} return 1; }}", "{}".repeat(children));
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        if accepted {
            let native = parse(&sources).expect("exact block and statement inventories");
            let body = &native.files[0].functions[0].body;
            assert_eq!(body.blocks.len(), syntax_v3::MAX_BLOCKS_PER_FUNCTION);
            assert_eq!(body.statements.len(), syntax_v3::MAX_STATEMENTS_PER_FUNCTION);
            syntax_v3::verify_snapshot(native, &sources).expect("exact limits verify");
        } else {
            assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
        }
    }
}

#[test]
fn blocks_and_statements_share_exact_and_first_extra_project_limit() {
    let body = format!("{} return 1;", "{}".repeat(4_095));
    let mut files = (0..16)
        .map(|index| {
            source(
                &format!("src/f{index:02}.zry"),
                &format!("function value{index}(): i32 {{ {body} }}"),
            )
        })
        .collect::<Vec<_>>();
    let sources = SourceMap::build(files.clone()).expect("source");
    let native = parse(&sources).expect("exact project inventories");
    assert_eq!(
        native
            .files
            .iter()
            .flat_map(|file| &file.functions)
            .map(|function| function.body.blocks.len())
            .sum::<usize>(),
        syntax_v3::MAX_BLOCKS_PER_PROJECT
    );
    assert_eq!(
        native
            .files
            .iter()
            .flat_map(|file| &file.functions)
            .map(|function| function.body.statements.len())
            .sum::<usize>(),
        syntax_v3::MAX_STATEMENTS_PER_PROJECT
    );
    files.push(source("src/overflow.zry", "function extra(): i32 { return 1; }"));
    let sources = SourceMap::build(files).expect("source");
    assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
}

#[test]
fn malformed_later_function_or_file_cannot_expose_earlier_blocks() {
    let sources = SourceMap::build(vec![
        source("a.zry", "function good(): i32 { {} return 1; }"),
        source("z.zry", REJECTED),
    ])
    .expect("two sources");
    assert_eq!(parse(&sources), Err("ZRYNA-F2002".to_owned()));
    let sources = SourceMap::build(vec![source(
        "src/main.zry",
        "function good(): i32 { {} return 1; } function bad(): i32 { { return (2); } return 3; }",
    )])
    .expect("source");
    assert_eq!(parse(&sources), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn block_ownership_and_source_authority_remain_verifier_bound() {
    let sources = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("source");
    let lexed = lex(&sources).expect("tokens");
    let foreign = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("foreign map");
    assert!(parse_v3_straight_line_candidate(&foreign, &lexed).is_err());
    assert!(parse_v3_import_candidate(&sources, &lexed).is_err());

    let mut native = parse(&sources).expect("block candidate");
    syntax_v3::verify_snapshot(native.clone(), &sources).expect("baseline verifies");
    let block_statement = native.files[0].functions[0]
        .body
        .statements
        .iter_mut()
        .find(|statement| matches!(statement.kind, syntax_v3::RawStatementKind::Block { .. }))
        .expect("nested block statement");
    block_statement.kind = syntax_v3::RawStatementKind::Block { block: 0 };
    assert!(syntax_v3::verify_snapshot(native, &sources).is_err());
}
