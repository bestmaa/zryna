//! Nominal data-syntax evidence against the frozen protocol-v4 worker.

use zryna_frontend::{native_lexer::lex, native_parser::v4::parse_v4_candidate, syntax_v4};
use zryna_source::{SourceFileInput, SourceMap};

const DATA: &str = include_str!("native_parser_v4_data/data.zry");
const SNAPSHOT: &[u8] = include_bytes!("native_parser_v4_data/data.snapshot.json");
const IMPORTS: &str = include_str!("native_parser_v4_data/imports.zry");
const IMPORT_SNAPSHOT: &[u8] = include_bytes!("native_parser_v4_data/imports.snapshot.json");
const FUNCTION: &str = include_str!("native_parser_v4_data/function.zry");
const FUNCTION_SNAPSHOT: &[u8] = include_bytes!("native_parser_v4_data/function.snapshot.json");
const ARITHMETIC: &str = include_str!("native_parser_v4_data/arithmetic.zry");
const ARITHMETIC_SNAPSHOT: &[u8] = include_bytes!("native_parser_v4_data/arithmetic.snapshot.json");
const MISSING: &str = include_str!("native_parser_v4_data/missing.zry");
const MISSING_SNAPSHOT: &[u8] = include_bytes!("native_parser_v4_data/missing.snapshot.json");
const CONTROL: &str = include_str!("native_parser_v4_data/control.zry");
const CONTROL_SNAPSHOT: &[u8] = include_bytes!("native_parser_v4_data/control.snapshot.json");
const OWNERSHIP: &str = include_str!("native_parser_v4_data/ownership.zry");
const OWNERSHIP_SNAPSHOT: &[u8] = include_bytes!("native_parser_v4_data/ownership.snapshot.json");
const CONSTRUCT: &str = include_str!("native_parser_v4_data/construct.zry");
const CONSTRUCT_SNAPSHOT: &[u8] = include_bytes!("native_parser_v4_data/construct.snapshot.json");
const ARRAYS: &str = include_str!("native_parser_v4_data/arrays.zry");
const ARRAYS_SNAPSHOT: &[u8] = include_bytes!("native_parser_v4_data/arrays.snapshot.json");
const UPGRADE: &str = include_str!("native_parser_v4_data/upgrade.zry");
const UPGRADE_SNAPSHOT: &[u8] = include_bytes!("native_parser_v4_data/upgrade.snapshot.json");
const MATCH: &str = include_str!("native_parser_v4_data/match.zry");
const MATCH_SNAPSHOT: &[u8] = include_bytes!("native_parser_v4_data/match.snapshot.json");
const RESERVED_REFERENCE: &str = include_str!("native_parser_v4_data/reserved-reference.zry");
const RESERVED_REFERENCE_SNAPSHOT: &[u8] =
    include_bytes!("native_parser_v4_data/reserved-reference.snapshot.json");
const ORDER_A: &str =
    include_str!("../../../tests/provider-conformance-v4/fixtures/ordering-a.zry");
const ORDER_Z: &str =
    include_str!("../../../tests/provider-conformance-v4/fixtures/ordering-z.zry");
const ORDER_SNAPSHOT: &[u8] =
    include_bytes!("../../../tests/provider-conformance-v4/fixtures/ordering.snapshot.json");

fn assert_exact(path: &str, text: &str, snapshot: &[u8]) -> syntax_v4::RawProjectSyntaxSnapshot {
    let sources =
        SourceMap::build(vec![SourceFileInput { path: path.to_owned(), text: text.to_owned() }])
            .expect("source map");
    let lexed = lex(&sources).expect("native tokens");
    let native = parse_v4_candidate(&sources, &lexed).expect("native data syntax");
    let worker = syntax_v4::decode_snapshot(snapshot).expect("frozen worker syntax");
    assert_eq!(native, worker);
    syntax_v4::verify_snapshot(native.clone(), &sources).expect("v4 verifier");
    native
}

#[test]
fn nominal_data_and_nested_type_arena_match_worker() {
    let native = assert_exact("src/data.zry", DATA, SNAPSHOT);
    assert_eq!(native.files[0].data_declarations.len(), 2);
    assert_eq!(native.files[0].type_syntax.len(), 5);
}

#[test]
fn named_import_prefix_and_data_declaration_match_worker() {
    let native = assert_exact("src/imports.zry", IMPORTS, IMPORT_SNAPSHOT);
    assert_eq!(native.files[0].imports[0].bindings.len(), 2);
}

#[test]
fn exported_function_and_result_type_match_worker() {
    let native = assert_exact("src/function.zry", FUNCTION, FUNCTION_SNAPSHOT);
    assert_eq!(native.files[0].functions.len(), 1);
}

#[test]
fn arithmetic_and_call_arenas_match_worker() {
    let native = assert_exact("src/arithmetic.zry", ARITHMETIC, ARITHMETIC_SNAPSHOT);
    assert_eq!(native.files[0].functions[0].body.expressions.len(), 7);
}

#[test]
fn missing_function_annotations_match_worker() {
    let native = assert_exact("src/missing.zry", MISSING, MISSING_SNAPSHOT);
    assert_eq!(native.files[0].type_syntax.len(), 3);
}

#[test]
fn control_flow_and_assignment_arenas_match_worker() {
    let native = assert_exact("src/control.zry", CONTROL, CONTROL_SNAPSHOT);
    assert_eq!(native.files[0].functions[0].body.blocks.len(), 5);
}

#[test]
fn ownership_and_data_access_match_worker() {
    let native = assert_exact("src/ownership.zry", OWNERSHIP, OWNERSHIP_SNAPSHOT);
    assert_eq!(native.files[0].data_declarations.len(), 2);
}

#[test]
fn struct_construction_and_shorthand_match_worker() {
    let native = assert_exact("src/construct.zry", CONSTRUCT, CONSTRUCT_SNAPSHOT);
    assert_eq!(native.files[0].functions[0].body.expressions.len(), 4);
}

#[test]
fn nested_array_constructions_preserve_type_order() {
    let native = assert_exact("src/arrays.zry", ARRAYS, ARRAYS_SNAPSHOT);
    assert_eq!(native.files[0].functions[0].body.expressions.len(), 10);
}

#[test]
fn weak_upgrade_callbacks_match_worker() {
    let native = assert_exact("src/upgrade.zry", UPGRADE, UPGRADE_SNAPSHOT);
    assert_eq!(native.files[0].functions[0].body.blocks.len(), 3);
}

#[test]
fn match_arms_match_worker() {
    let native = assert_exact("src/match.zry", MATCH, MATCH_SNAPSHOT);
    assert_eq!(native.files[0].functions[0].body.expressions.len(), 4);
}

#[test]
fn reserved_form_name_without_call_remains_a_reference() {
    assert_exact("src/reserved-reference.zry", RESERVED_REFERENCE, RESERVED_REFERENCE_SNAPSHOT);
}

#[test]
fn canonical_multi_file_order_matches_provider_corpus() {
    let sources = SourceMap::build(vec![
        SourceFileInput { path: "src/z.zry".to_owned(), text: ORDER_Z.to_owned() },
        SourceFileInput { path: "src/a.zry".to_owned(), text: ORDER_A.to_owned() },
    ])
    .expect("source map");
    let lexed = lex(&sources).expect("native tokens");
    let native = parse_v4_candidate(&sources, &lexed).expect("native v4 syntax");
    let worker = syntax_v4::decode_snapshot(ORDER_SNAPSHOT).expect("frozen worker syntax");
    assert_eq!(native, worker);
    syntax_v4::verify_snapshot(native, &sources).expect("v4 verifier");
}

#[test]
fn unsupported_syntax_rejects_atomically() {
    for text in [
        "function f(): i32 { return --value; }",
        "function f(): i32 { return (1); }",
        "interface Pair extends ZrynaStruct { x: i32; x: i32; }",
        "function f(): i32 { return 1; } import { f } from \"./f.zry\";",
        "function f(): i32 { const value = 1; return value; }",
        "function f(): i32 { return match(1, { 'Pair.one': () => 1 }); }",
        "function f(x: any): i32 { return 1; }",
        "function f(x: string): i32 { return 1; }",
        "function f(): i32 { return this; }",
        "function f(): i32 { return clone({}); }",
    ] {
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/rejected.zry".to_owned(),
            text: text.to_owned(),
        }])
        .expect("source map");
        let lexed = lex(&sources).expect("native tokens");
        let error = parse_v4_candidate(&sources, &lexed).expect_err(text);
        assert_eq!(error.diagnostic().code(), "ZRYNA-F2002", "{text}");
    }
}

#[test]
fn first_extra_source_nesting_and_array_length_reject() {
    let nested =
        format!("function f(): i32 {{ {}return 1; {} }}", "{".repeat(128), "}".repeat(128));
    for text in
        [nested, "function f(value: FixedArray<i32, 1048577>): i32 { return 1; }".to_owned()]
    {
        let sources = SourceMap::build(vec![SourceFileInput {
            path: "src/resource.zry".to_owned(),
            text: text.clone(),
        }])
        .expect("source map");
        let lexed = lex(&sources).expect("native tokens");
        let error = parse_v4_candidate(&sources, &lexed).expect_err(&text);
        assert_eq!(error.diagnostic().code(), "ZRYNA-F1002");
    }
}

#[test]
fn bounded_expression_grammar_mutations_verify() {
    let atoms = [
        "x",
        "1",
        "true",
        "'ok'",
        "clone(x)",
        "x[0]",
        "Maybe.some(x)",
        "Pair({ value: x })",
        "Vec<i32>([1])",
    ];
    let operators = ["+", "-", "*", "===", "!==", "<", "<=", ">", ">="];
    let mut seed = 0x412_u32;
    for case in 0..128 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let left = atoms[(seed as usize) % atoms.len()];
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let right = atoms[(seed as usize) % atoms.len()];
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let operator = operators[(seed as usize) % operators.len()];
        let text = format!(
            "// π case {case}\nfunction f(x: i32): i32 {{ return {left} {operator} {right}; }}"
        );
        let sources =
            SourceMap::build(vec![SourceFileInput { path: "src/mutated.zry".to_owned(), text }])
                .expect("source map");
        let lexed = lex(&sources).expect("native tokens");
        let native = parse_v4_candidate(&sources, &lexed).expect("generated syntax");
        syntax_v4::verify_snapshot(native, &sources).expect("source-bound generated candidate");
    }
}

#[test]
fn first_extra_v4_block_statement_and_member_inventories_reject() {
    fn fields(count: usize) -> String {
        use std::fmt::Write as _;
        let mut text = String::new();
        for index in 0..count {
            write!(text, "f{index}: i32;").expect("string write");
        }
        text
    }
    for (accepted, rejected) in [
        (
            format!("function f(): i32 {{ {} }}", "{}".repeat(4_095)),
            format!("function f(): i32 {{ {} }}", "{}".repeat(4_096)),
        ),
        (
            format!("function f(): i32 {{ {} }}", "return 1;".repeat(4_096)),
            format!("function f(): i32 {{ {} }}", "return 1;".repeat(4_097)),
        ),
        (
            format!("interface Pair extends ZrynaStruct {{ {} }}", fields(1_024)),
            format!("interface Pair extends ZrynaStruct {{ {} }}", fields(1_025)),
        ),
    ] {
        let parse = |text: String| {
            let sources =
                SourceMap::build(vec![SourceFileInput { path: "src/limit.zry".to_owned(), text }])
                    .expect("source map");
            let lexed = lex(&sources).expect("native tokens");
            parse_v4_candidate(&sources, &lexed)
        };
        parse(accepted).expect("exact limit accepted");
        let first_extra = parse(rejected);
        assert_eq!(
            first_extra.expect_err("first extra rejects").diagnostic().code(),
            "ZRYNA-F1002"
        );
    }
}
