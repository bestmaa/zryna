//! Complete independent source contexts for identifier roles; no provider is involved.

use serde_json::{Value, json};
use zryna_source::{SourceFileInput, SourceMap};

use super::{RawProjectSyntaxSnapshot, decode_snapshot, validate_declarations, verify_snapshot};

const SCRIPT: &[u8] = include_bytes!("../../../../tests/m7-syntax-fixtures/keyword-script.json");
const SCRIPT_SOURCE: &str = include_str!("../../../../tests/m7-syntax-fixtures/keyword-script.zry");
const MODULE: &[u8] = include_bytes!("../../../../tests/m7-syntax-fixtures/keyword-module.json");
const MODULE_SOURCE: &str = include_str!("../../../../tests/m7-syntax-fixtures/keyword-module.zry");
const VALUES_SOURCE: &str = include_str!("../../../../tests/m7-syntax-fixtures/keyword-values.zry");
const STRICT: &[u8] =
    include_bytes!("../../../../tests/m7-syntax-fixtures/keyword-strict-target.json");
const STRICT_SOURCE: &str =
    include_str!("../../../../tests/m7-syntax-fixtures/keyword-strict-target.zry");
const OPERATIONS: &[u8] = include_bytes!("../../../../tests/m7-syntax-fixtures/operations.json");
const OPERATIONS_SOURCE: &str = include_str!("../../../../tests/m7-syntax-fixtures/operations.zry");

fn value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("independent raw fixture")
}

fn sources(raw: &Value, source: &str) -> SourceMap {
    SourceMap::build(
        raw["files"]
            .as_array()
            .expect("files")
            .iter()
            .enumerate()
            .map(|(index, unit)| SourceFileInput {
                path: unit["path"].as_str().expect("path").into(),
                text: if index == 0 { source } else { VALUES_SOURCE }.into(),
            })
            .collect(),
    )
    .expect("source contexts")
}

fn rejects(raw: Value, source: &str) {
    let map = sources(&raw, source);
    let raw: RawProjectSyntaxSnapshot = serde_json::from_value(raw).expect("typed raw claims");
    let errors = verify_snapshot(raw, &map).expect_err("no syntax authority");
    assert!(errors.iter().all(|error| error.code == "ZRYNA-Y5001"), "{errors:?}");
}

// Keep the whole independently authored source/arena context when changing a single name.
fn shift(raw: &mut Value, start: u64, end: u64, delta: i64) {
    match raw {
        Value::Object(object) => {
            if object.get("file").and_then(Value::as_u64) == Some(0)
                && object.contains_key("start")
                && object.contains_key("end")
            {
                for key in ["start", "end"] {
                    let offset = object[key].as_u64().expect("offset");
                    if offset >= end && !(start == end && key == "start" && offset == start) {
                        object.insert(
                            key.into(),
                            json!(
                                offset.checked_add_signed(delta).expect("shifted fixture offset")
                            ),
                        );
                    }
                }
            }
            for child in object.values_mut() {
                shift(child, start, end, delta);
            }
        }
        Value::Array(array) => {
            for child in array {
                shift(child, start, end, delta);
            }
        }
        _ => {}
    }
}

fn renamed(bytes: &[u8], source: &str, pointer: &str, replacement: &str) -> (Value, String) {
    let mut raw = value(bytes);
    let name = raw.pointer(pointer).expect("selected source name");
    let start = name["span"]["start"].as_u64().expect("start");
    let end = name["span"]["end"].as_u64().expect("end");
    assert_eq!(
        &source[usize::try_from(start).expect("fixture start")
            ..usize::try_from(end).expect("fixture end")],
        name["text"].as_str().expect("name")
    );
    let mut source = source.to_owned();
    source.replace_range(
        usize::try_from(start).expect("fixture start")..usize::try_from(end).expect("fixture end"),
        replacement,
    );
    shift(
        &mut raw,
        start,
        end,
        i64::try_from(replacement.len()).expect("replacement length")
            - i64::try_from(end - start).expect("original length"),
    );
    raw.pointer_mut(pointer).expect("selected source name")["text"] = json!(replacement);
    (raw, source)
}

#[test]
fn complete_script_and_module_contexts_preserve_permitted_identifier_roles() {
    for (bytes, source) in [(SCRIPT, SCRIPT_SOURCE), (MODULE, MODULE_SOURCE)] {
        let map = sources(&value(bytes), source);
        verify_snapshot(decode_snapshot(bytes).expect("closed fixture"), &map)
            .expect("syntax only; unresolved names and semantic validity remain later checks");
    }
}

#[test]
fn source_exact_keywords_cannot_claim_function_or_strict_binding_roles() {
    for (bytes, source, pointer, name) in [
        (SCRIPT, SCRIPT_SOURCE, "/files/0/functions/0/name", "true"),
        (MODULE, MODULE_SOURCE, "/files/0/functions/0/name", "await"),
        (MODULE, MODULE_SOURCE, "/files/0/functions/0/parameters/0/name", "yield"),
        (MODULE, MODULE_SOURCE, "/files/0/functions/0/body/statements/0/kind/name", "eval"),
        (SCRIPT, SCRIPT_SOURCE, "/files/0/functions/0/body/statements/0/kind/name", "let"),
    ] {
        let (raw, source) = renamed(bytes, source, pointer, name);
        rejects(raw, &source);
    }
}

#[test]
fn type_heads_follow_their_actual_function_or_data_owner() {
    let (raw, source) = renamed(SCRIPT, SCRIPT_SOURCE, "/files/0/type_syntax/0/kind/name", "true");
    rejects(raw, &source);
    let (raw, source) = renamed(MODULE, MODULE_SOURCE, "/files/0/type_syntax/0/kind/name", "await");
    rejects(raw, &source);
    let (raw, source) = renamed(
        MODULE,
        MODULE_SOURCE,
        "/files/0/functions/1/type_parameters/parameters/0/name",
        "yield",
    );
    rejects(raw, &source);
}

#[test]
fn bare_function_type_reference_survives_function_and_exported_data_contexts() {
    for (bytes, source, pointer) in [
        (SCRIPT, SCRIPT_SOURCE, "/files/0/type_syntax/0/kind/name"),
        (MODULE, MODULE_SOURCE, "/files/0/type_syntax/0/kind/name"),
    ] {
        let (raw, source) = renamed(bytes, source, pointer, "function");
        let map = sources(&raw, &source);
        verify_snapshot(serde_json::from_value(raw).expect("authentic Named claims"), &map)
            .expect("syntax preserves Named function; type resolution remains later");
    }
}

#[test]
fn function_type_head_cannot_claim_a_complete_generic_application() {
    let source = "function f(value: function<i32>): i32 { return 1; }";
    let span = |start: usize, end: usize| json!({"file": 0, "start": start, "end": end});
    let name =
        |text: &str, start: usize| json!({"text": text, "span": span(start, start + text.len())});
    let head = source.find("function<").expect("application head");
    let less = head + "function".len();
    let argument = less + 1;
    let greater = argument + 3;
    let result = source.find("): i32").expect("result") + 3;
    let parameter = source.find("value").expect("parameter");
    let open = source.find('{').expect("body");
    let close = source.find('}').expect("close");
    let ret = source.find("return").expect("return");
    let literal = source.find('1').expect("literal");
    let semi = source.find(';').expect("semicolon");
    let raw = json!({"schema_version": 5, "diagnostics": [], "files": [{
        "id": 0, "path": "function-application.zry", "imports": [], "data_declarations": [],
        "type_syntax": [
            {"span": span(argument, greater), "kind": {"kind": "named", "name": name("i32", argument)}},
            {"span": span(head, greater + 1), "kind": {"kind": "application", "name": name("function", head),
                "type_arguments": {"span": span(less, greater + 1), "less_than_span": span(less, less + 1),
                    "arguments": [0], "comma_spans": [], "greater_than_span": span(greater, greater + 1)}}},
            {"span": span(result, result + 3), "kind": {"kind": "named", "name": name("i32", result)}}
        ], "functions": [{"span": span(0, source.len()), "export_span": null,
            "function_span": span(0, 8), "name": name("f", 9), "type_parameters": null,
            "parameters": [{"span": span(parameter, greater + 1), "name": name("value", parameter), "type_syntax": 1}],
            "result_type": 2, "body": {"span": span(open, close + 1), "root_block": 0,
                "blocks": [{"span": span(open, close + 1), "open_brace_span": span(open, open + 1),
                    "statements": [0], "close_brace_span": span(close, close + 1)}],
                "statements": [{"span": span(ret, semi + 1), "kind": {"kind": "return",
                    "keyword_span": span(ret, ret + 6), "value": 0, "semicolon_span": span(semi, semi + 1)}}],
                "expressions": [{"span": span(literal, literal + 1), "kind": {"kind": "i32-literal", "spelling": "1"}}]
            }}]
    }]});
    let map = sources(&raw, source);
    validate_declarations(&map, &serde_json::from_value(raw.clone()).expect("raw application"))
        .expect("declaration-only checks do not establish a TypeReference application AST");
    rejects(raw, source);
}

#[test]
fn alias_local_role_does_not_inherit_the_imported_member_role() {
    let (raw, source) =
        renamed(MODULE, MODULE_SOURCE, "/files/0/imports/0/bindings/0/local", "true");
    rejects(raw, &source);
}

#[test]
fn expression_and_match_bindings_require_their_own_source_roles() {
    let original = value(MODULE);
    let functions = original["files"][0]["functions"].as_array().expect("functions");
    let expressions = functions[0]["body"]["expressions"].as_array().expect("expressions");
    let call = expressions
        .iter()
        .position(|expression| expression["kind"]["kind"] == "call")
        .expect("read-only eval call");
    let pointer = format!("/files/0/functions/0/body/expressions/{call}/kind/callee");
    let (raw, source) = renamed(MODULE, MODULE_SOURCE, &pointer, "true");
    rejects(raw, &source);
    let arms = functions[3]["body"]["expressions"].as_array().expect("expressions");
    let match_id =
        arms.iter().position(|expression| expression["kind"]["kind"] == "match").expect("match");
    let pointer = format!("/files/0/functions/3/body/expressions/{match_id}/kind/arms/0/binding");
    let (raw, source) = renamed(MODULE, MODULE_SOURCE, &pointer, "arguments");
    rejects(raw, &source);
}

#[test]
fn strict_bare_assignment_rejects_while_read_only_and_member_uses_are_portable() {
    rejects(value(STRICT), STRICT_SOURCE);
    let mut raw = value(STRICT);
    raw["files"][0]["functions"][0]["export_span"] = Value::Null;
    raw["files"][0]["functions"][0]["span"]["start"] = json!(7);
    shift(&mut raw, 0, 7, -7);
    let source = &STRICT_SOURCE[7..];
    let map = sources(&raw, source);
    verify_snapshot(serde_json::from_value(raw).expect("script raw"), &map)
        .expect("ordinary script permits bare eval assignment at the syntax boundary");
}

#[test]
fn hidden_export_cannot_downgrade_the_original_module_to_script() {
    let mut raw = value(STRICT);
    raw["files"][0]["functions"][0]["export_span"] = Value::Null;
    raw["files"][0]["functions"][0]["span"]["start"] = json!(7);
    rejects(raw, STRICT_SOURCE);
}

#[test]
fn source_exact_literal_cannot_claim_an_assignment_place() {
    let (mut raw, source) =
        renamed(STRICT, STRICT_SOURCE, "/files/0/functions/0/body/expressions/0/kind/name", "1234");
    raw["files"][0]["functions"][0]["body"]["expressions"][0]["kind"] =
        json!({"kind": "i32-literal", "spelling": "1234"});
    rejects(raw, &source);
}

#[test]
fn weak_binding_and_nominal_head_cannot_use_keyword_member_rules() {
    let original = value(OPERATIONS);
    let body = &original["files"][0]["functions"][0]["body"];
    let statement = body["statements"]
        .as_array()
        .expect("statements")
        .iter()
        .position(|statement| statement["kind"]["kind"] == "weak-upgrade")
        .expect("weak binding");
    let pointer = format!("/files/0/functions/0/body/statements/{statement}/kind/binding");
    let (raw, source) = renamed(OPERATIONS, OPERATIONS_SOURCE, &pointer, "true");
    rejects(raw, &source);
    let expression = body["expressions"]
        .as_array()
        .expect("expressions")
        .iter()
        .position(|expression| expression["kind"]["kind"] == "struct-construction")
        .expect("nominal construction");
    let pointer = format!("/files/0/functions/0/body/expressions/{expression}/kind/type_name");
    let (raw, source) = renamed(OPERATIONS, OPERATIONS_SOURCE, &pointer, "true");
    rejects(raw, &source);
}

#[test]
fn shorthand_authenticates_both_the_member_and_runtime_occurrence() {
    let original = value(OPERATIONS);
    let expressions = original["files"][0]["functions"][0]["body"]["expressions"]
        .as_array()
        .expect("expressions");
    let constructor = expressions
        .iter()
        .position(|expression| expression["kind"]["kind"] == "struct-construction")
        .expect("struct");
    let field = &expressions[constructor]["kind"]["fields"][0]["kind"];
    let alias = field["value"].as_u64().expect("reference alias");
    let pointer =
        format!("/files/0/functions/0/body/expressions/{constructor}/kind/fields/0/kind/name");
    let (mut raw, source) = renamed(OPERATIONS, OPERATIONS_SOURCE, &pointer, "true");
    raw["files"][0]["functions"][0]["body"]["expressions"]
        [usize::try_from(alias).expect("fixture alias")]["kind"]["name"]["text"] = json!("true");
    rejects(raw, &source);
}

#[test]
fn strict_directive_cannot_bypass_the_frozen_source_grammar() {
    let source = "function f(): i32 { \"use strict\"; return 1; }";
    let span = |start, end| json!({"file": 0, "start": start, "end": end});
    let raw = json!({"schema_version": 5, "diagnostics": [], "files": [{
        "id": 0, "path": "directive.zry", "imports": [], "data_declarations": [],
        "type_syntax": [{"span": span(14, 17), "kind": {"kind": "named",
            "name": {"text": "i32", "span": span(14, 17)}}}],
        "functions": [{"span": span(0, 45), "export_span": null,
            "function_span": span(0, 8), "name": {"text": "f", "span": span(9, 10)},
            "type_parameters": null, "parameters": [], "result_type": 0,
            "body": {"span": span(18, 45), "root_block": 0,
                "blocks": [{"span": span(18, 45), "open_brace_span": span(18, 19),
                    "statements": [0, 1], "close_brace_span": span(44, 45)}],
                "statements": [
                    {"span": span(20, 33), "kind": {"kind": "expression-statement",
                        "expression": 0, "semicolon_span": span(32, 33)}},
                    {"span": span(34, 43), "kind": {"kind": "return",
                        "keyword_span": span(34, 40), "value": 1, "semicolon_span": span(42, 43)}}],
                "expressions": [
                    {"span": span(20, 32), "kind": {"kind": "string-literal", "spelling": "\"use strict\""}},
                    {"span": span(41, 42), "kind": {"kind": "i32-literal", "spelling": "1"}}]
            }}]
    }]});
    let map = sources(&raw, source);
    validate_declarations(&map, &serde_json::from_value(raw.clone()).expect("raw directive"))
        .expect("balanced declaration alone cannot authenticate body grammar");
    rejects(raw.clone(), source);
    let ordinary = source.replace("use strict", "use string");
    let mut raw = raw;
    raw["files"][0]["functions"][0]["body"]["expressions"][0]["kind"]["spelling"] =
        json!("\"use string\"");
    let map = sources(&raw, &ordinary);
    verify_snapshot(serde_json::from_value(raw).expect("ordinary raw"), &map)
        .expect("same whole arena with ordinary string prologue");
}
