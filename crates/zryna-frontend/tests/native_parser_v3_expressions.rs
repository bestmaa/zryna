//! Complete scalar-expression evidence against the pinned protocol-v3 worker.

use std::fmt::Write as _;

use zryna_frontend::{
    native_lexer::lex,
    native_parser::v3::{parse_v3_import_candidate, parse_v3_straight_line_candidate},
    syntax_v3,
};
use zryna_source::{SourceFileInput, SourceMap};

const MAIN: &str = include_str!("native_parser_v3_expressions/main.zry");
const MATH: &str = include_str!("native_parser_v3_expressions/math.zry");
const SNAPSHOT: &[u8] = include_bytes!("native_parser_v3_expressions/expressions.snapshot.json");
const REJECTED: &str = include_str!("native_parser_v3_expressions/rejected.zry");
const REJECTED_RESPONSE: &str = include_str!("native_parser_v3_expressions/rejected.response.json");
const REJECTED_PREFIX: &str = include_str!("native_parser_v3_expressions/rejected-prefix.zry");
const REJECTED_PREFIX_RESPONSE: &str =
    include_str!("native_parser_v3_expressions/rejected-prefix.response.json");

fn source(path: &str, text: &str) -> SourceFileInput {
    SourceFileInput { path: path.to_owned(), text: text.to_owned() }
}

fn parse(sources: &SourceMap) -> Result<syntax_v3::RawProjectSyntaxSnapshot, String> {
    let lexed = lex(sources).expect("bounded lexical stream");
    parse_v3_straight_line_candidate(sources, &lexed)
        .map_err(|error| error.diagnostic().code().to_owned())
}

fn one(expression: &str) -> Result<syntax_v3::RawProjectSyntaxSnapshot, String> {
    let text = format!("function value(): i32 {{ return {expression}; }}");
    let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
    parse(&sources)
}

#[test]
fn two_file_scalar_expressions_match_pinned_worker_and_verify() {
    let main = MAIN.replace("\r\n", "\n").replace('\n', "\r\n");
    let sources =
        SourceMap::build(vec![source("src/math.zry", MATH), source("src/main.zry", &main)])
            .expect("canonical sources from reversed input");
    let native = parse(&sources).expect("scalar expression candidate");
    let worker = syntax_v3::decode_snapshot(SNAPSHOT).expect("pinned TypeScript 6 snapshot");
    assert_eq!(native, worker);
    syntax_v3::verify_snapshot(native, &sources).expect("existing v3 verifier");
}

#[test]
fn adjacent_decrement_ambiguities_match_frozen_worker_rejections() {
    for (source_text, response) in
        [(REJECTED, REJECTED_RESPONSE), (REJECTED_PREFIX, REJECTED_PREFIX_RESPONSE)]
    {
        let worker: serde_json::Value =
            serde_json::from_str(response).expect("pinned worker response");
        assert_eq!(worker["error"]["code"], "ZRYNA-F2002");
        let sources = SourceMap::build(vec![source("src/main.zry", source_text)]).expect("source");
        assert_eq!(parse(&sources), Err("ZRYNA-F2002".to_owned()));
    }
    for expression in ["1- -1", "- -1", "1+-1"] {
        assert!(one(expression).is_ok(), "{expression}");
    }
}

#[test]
fn unsupported_expression_forms_remain_outside_the_candidate() {
    for expression in ["(1)", "-(-1)", "(helper)()", "helper<i32>(1)", "helper(...x)", "1 == 2"] {
        assert_eq!(one(expression), Err("ZRYNA-F2002".to_owned()), "{expression}");
    }
    for expression in ["helper?.(1)", "1 / 2", "1 && 2"] {
        assert_eq!(one(expression), Err("ZRYNA-F1501".to_owned()), "{expression}");
    }
}

#[test]
fn direct_calls_accept_exact_argument_limit_and_reject_first_extra() {
    for (count, accepted) in [
        (syntax_v3::MAX_PARAMETERS_PER_FUNCTION, true),
        (syntax_v3::MAX_PARAMETERS_PER_FUNCTION + 1, false),
    ] {
        let expression = format!("helper({},)", vec!["1"; count].join(", "));
        let text = format!("function value(): i32 {{ return {expression}; }}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        if accepted {
            let native = parse(&sources).expect("exact call argument limit");
            let syntax_v3::RawExpressionKind::Call { arguments, .. } =
                &native.files[0].functions[0].body.expressions[count].kind
            else {
                panic!("expected direct call");
            };
            assert_eq!(arguments.len(), count);
            syntax_v3::verify_snapshot(native, &sources).expect("exact limit verifies");
        } else {
            assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
        }
    }
}

#[test]
fn operators_calls_and_prefixes_obey_exact_and_first_extra_depth() {
    let max = syntax_v3::MAX_NESTING_DEPTH as usize;
    for (count, accepted) in [(max, true), (max + 1, false)] {
        let text = format!("function value(): i32 {{ return {}; }}", vec!["1"; count].join(" - "));
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        if accepted {
            syntax_v3::verify_snapshot(parse(&sources).expect("exact binary depth"), &sources)
                .expect("binary depth verifies");
        } else {
            assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
        }
    }
    for (calls, accepted) in [(max - 1, true), (max, false)] {
        let expression = format!("{}1{}", "helper(".repeat(calls), ")".repeat(calls));
        let text = format!("function value(): i32 {{ return {expression}; }}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        if accepted {
            syntax_v3::verify_snapshot(parse(&sources).expect("exact call depth"), &sources)
                .expect("call depth verifies");
        } else {
            assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
        }
    }
    for (minuses, accepted) in [(max - 1, true), (max, false)] {
        let expression = format!("{}1", "- ".repeat(minuses));
        let text = format!("function value(): i32 {{ return {expression}; }}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        if accepted {
            syntax_v3::verify_snapshot(parse(&sources).expect("exact prefix depth"), &sources)
                .expect("prefix depth verifies");
        } else {
            assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
        }
    }
    for (minuses, accepted) in [(max, true), (max + 1, false)] {
        let expression = format!("{}-1", "- ".repeat(minuses - 1));
        let text = format!("function value(): i32 {{ return {expression}; }}");
        let sources = SourceMap::build(vec![source("src/main.zry", &text)]).expect("source");
        if accepted {
            syntax_v3::verify_snapshot(
                parse(&sources).expect("exact signed-prefix depth"),
                &sources,
            )
            .expect("signed-prefix depth verifies");
        } else {
            assert_eq!(parse(&sources), Err("ZRYNA-F1002".to_owned()));
        }
    }
}

#[test]
fn scalar_expressions_obey_exact_and_first_extra_inventory() {
    let expression = "-helper(1, 2) * helper(3, 4)";
    for (last, accepted) in [(expression, true), ("-helper(1, 2) * -helper(3, 4)", false)] {
        let mut text = String::from("function many(): i32 {\n");
        for index in 0..2_047 {
            writeln!(&mut text, "const value{index}: i32 = {expression};").expect("write source");
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
fn verifier_rejects_forged_binary_and_call_edges() {
    let sources = SourceMap::build(vec![source(
        "src/main.zry",
        "function f(): i32 { return helper(1, 2) * 3; }",
    )])
    .expect("source");
    let native = parse(&sources).expect("candidate");
    syntax_v3::verify_snapshot(native.clone(), &sources).expect("baseline verifies");

    let mut forged_operator = native.clone();
    let expressions = &mut forged_operator.files[0].functions[0].body.expressions;
    let operand_span = expressions[3].span;
    let syntax_v3::RawExpressionKind::Multiplication { operator_span, .. } =
        &mut expressions[4].kind
    else {
        panic!("expected multiplication");
    };
    *operator_span = operand_span;
    assert!(syntax_v3::verify_snapshot(forged_operator, &sources).is_err());

    let mut forged_arguments = native;
    let syntax_v3::RawExpressionKind::Call { arguments, .. } =
        &mut forged_arguments.files[0].functions[0].body.expressions[2].kind
    else {
        panic!("expected direct call");
    };
    arguments[1] = arguments[0];
    assert!(syntax_v3::verify_snapshot(forged_arguments, &sources).is_err());
}

#[test]
fn later_malformed_input_cannot_expose_earlier_expression_arenas() {
    let sources = SourceMap::build(vec![
        source("a.zry", "function good(): i32 { return helper(1, 2) * 3; }"),
        source("z.zry", "function bad(): i32 { return 1--1; }"),
    ])
    .expect("two sources");
    assert_eq!(parse(&sources), Err("ZRYNA-F2002".to_owned()));
    let text =
        "function good(): i32 { return 1 * 2; } function bad(): i32 { return helper(1,,2); }";
    let sources = SourceMap::build(vec![source("src/main.zry", text)]).expect("source");
    assert_eq!(parse(&sources), Err("ZRYNA-F2002".to_owned()));
}

#[test]
fn lexical_failure_foreign_source_map_and_import_only_entry_remain_closed() {
    let sources = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("source");
    let lexed = lex(&sources).expect("tokens");
    let foreign = SourceMap::build(vec![source("src/main.zry", MAIN)]).expect("foreign map");
    assert!(parse_v3_straight_line_candidate(&foreign, &lexed).is_err());
    assert!(parse_v3_import_candidate(&sources, &lexed).is_err());

    let malformed =
        SourceMap::build(vec![source("src/main.zry", "function f(): i32 { return 1 * @; }")])
            .expect("malformed source");
    let lexed = lex(&malformed).expect("recoverable lexical stream");
    assert!(!lexed.diagnostics().is_empty());
    assert!(parse_v3_straight_line_candidate(&malformed, &lexed).is_err());
}
