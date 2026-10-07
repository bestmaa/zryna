//! Independently authored complete arenas and hostile source claims, without a provider.

use super::{RawProjectSyntaxSnapshot, decode_snapshot, validate_declarations, verify_snapshot};
use serde_json::{Value, json};
use zryna_source::{SourceFileInput, SourceMap, UntrustedSpan};

const REFERENCE: &[u8] = include_bytes!("../../../../tests/m7-syntax-fixtures/reference.json");
const MAIN: &str = include_str!("../../../../tests/m7-syntax-fixtures/main.zry");
const VALUES: &str = include_str!("../../../../tests/m7-syntax-fixtures/values.zry");
const PRECEDENCE: &[u8] = include_bytes!("../../../../tests/m7-syntax-fixtures/precedence.json");
const PRECEDENCE_SOURCE: &str = include_str!("../../../../tests/m7-syntax-fixtures/precedence.zry");
const CONTROL: &[u8] = include_bytes!("../../../../tests/m7-syntax-fixtures/control.json");
const CONTROL_SOURCE: &str = include_str!("../../../../tests/m7-syntax-fixtures/control.zry");
const OPERATIONS: &[u8] = include_bytes!("../../../../tests/m7-syntax-fixtures/operations.json");
const OPERATIONS_SOURCE: &str = include_str!("../../../../tests/m7-syntax-fixtures/operations.zry");

fn sources(main: &str, values: &str) -> SourceMap {
    SourceMap::build(vec![
        SourceFileInput { path: "main.zry".into(), text: main.into() },
        SourceFileInput { path: "values.zry".into(), text: values.into() },
    ])
    .expect("source fixtures")
}

fn single(name: &str, source: &str) -> SourceMap {
    SourceMap::build(vec![SourceFileInput { path: format!("{name}.zry"), text: source.into() }])
        .expect("source fixture")
}

fn value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("independent JSON")
}
fn raw(value: Value) -> RawProjectSyntaxSnapshot {
    serde_json::from_value(value).expect("typed raw DTO")
}

fn rejects(value: Value, map: &SourceMap) {
    let errors = verify_snapshot(raw(value), map).expect_err("no partial syntax seal");
    assert!(errors.iter().all(|error| error.code == "ZRYNA-Y5001"), "{errors:?}");
}

#[test]
fn complete_reference_is_bound_to_exact_immutable_source_map() {
    let map = sources(MAIN, VALUES);
    let verified = verify_snapshot(decode_snapshot(REFERENCE).expect("reference decode"), &map)
        .expect("complete syntax");
    assert!(verified.is_bound_to(&map));
    assert!(verified.is_bound_to(&map.clone()));
    assert!(!verified.is_bound_to(&sources(MAIN, VALUES)));
    assert_eq!(verified.schema_version(), 5);
    assert_eq!(verified.files().len(), 2);
    assert!(verified.advisories().is_empty());
}

#[test]
fn independent_complete_fixtures_cover_every_expression_and_statement_form() {
    let mut expression_tags = std::collections::BTreeSet::new();
    let mut statement_tags = std::collections::BTreeSet::new();
    for (bytes, map) in [
        (REFERENCE, sources(MAIN, VALUES)),
        (PRECEDENCE, single("precedence", PRECEDENCE_SOURCE)),
        (CONTROL, single("control", CONTROL_SOURCE)),
        (OPERATIONS, single("operations", OPERATIONS_SOURCE)),
    ] {
        verify_snapshot(decode_snapshot(bytes).expect("closed fixture"), &map)
            .expect("complete source and arenas");
        for unit in value(bytes)["files"].as_array().expect("files") {
            for function in unit["functions"].as_array().expect("functions") {
                for expression in function["body"]["expressions"].as_array().expect("expressions") {
                    expression_tags
                        .insert(expression["kind"]["kind"].as_str().expect("tag").to_owned());
                }
                for statement in function["body"]["statements"].as_array().expect("statements") {
                    statement_tags
                        .insert(statement["kind"]["kind"].as_str().expect("tag").to_owned());
                }
            }
        }
    }
    assert_eq!(expression_tags.len(), 28, "{expression_tags:?}");
    assert_eq!(statement_tags.len(), 8, "{statement_tags:?}");
    // These fixtures assert syntax only, including uses whose semantics must reject later.
}

#[test]
fn fixed_independent_hostile_body_claims_never_seal() {
    let cases = value(include_bytes!("../../../../tests/m7-syntax-fixtures/hostile-bodies.json"));
    for case in cases.as_array().expect("cases") {
        let mut snapshot = value(REFERENCE);
        *snapshot.pointer_mut(case["pointer"].as_str().expect("pointer")).expect("target") =
            case["value"].clone();
        rejects(snapshot, &sources(MAIN, VALUES));
    }
}

#[test]
fn whole_project_raw_barrier_precedes_real_earlier_declaration_error() {
    let source = MAIN.replacen("function score", "function Weak ", 1);
    let map = sources(&source, VALUES);
    let mut authentic = value(REFERENCE);
    let name = &mut authentic["files"][0]["functions"][0]["name"];
    name["text"] = json!("Weak");
    name["span"]["end"] = json!(name["span"]["end"].as_u64().expect("name end") - 1);
    let start = u32::try_from(source.find("Weak").expect("reserved name")).expect("fixture offset");
    let expected = map
        .verify_span(UntrustedSpan { file: 0, start, end: start + 4 })
        .expect("authentic reserved identifier");
    let declaration = validate_declarations(&map, &raw(authentic.clone()))
        .expect_err("real source-spelled reserved declaration");
    assert_eq!(declaration.code, "ZRYNA-D7001");
    assert_eq!(declaration.span, Some(expected));
    let full = verify_snapshot(raw(authentic.clone()), &map)
        .expect_err("complete source/arena baseline reaches declaration admission");
    assert_eq!(full, vec![declaration]);
    for orphan in [false, true] {
        let mut snapshot = authentic.clone();
        let body = &mut snapshot["files"][1]["functions"][0]["body"];
        if orphan {
            let expression = body["expressions"][0].clone();
            body["expressions"].as_array_mut().expect("expressions").push(expression);
        } else {
            body["expressions"][0]["span"]["file"] = json!(0);
        }
        assert_eq!(
            validate_declarations(&map, &raw(snapshot.clone()))
                .expect_err("real reserved declaration")
                .code,
            "ZRYNA-D7001"
        );
        rejects(snapshot, &map);
    }
}

#[test]
fn faithful_invalid_bound_is_deferred_until_complete_project_barrier() {
    let mut snapshot = value(REFERENCE);
    let bound = &mut snapshot["files"][1]["data_declarations"][0]["type_parameters"]["parameters"]
        [0]["bound"];
    let start =
        bound["span"]["start"].as_u64().and_then(|n| usize::try_from(n).ok()).expect("start");
    let end = bound["span"]["end"].as_u64().and_then(|n| usize::try_from(n).ok()).expect("end");
    bound["text"] = json!("OpaqueCopy");
    let mut source = VALUES.to_owned();
    source.replace_range(start..end, "OpaqueCopy");
    let errors = verify_snapshot(raw(snapshot), &sources(MAIN, &source))
        .expect_err("invalid source-spelled bound");
    assert_eq!(errors[0].code, "ZRYNA-D7001");
    assert!(errors[0].span.is_some());
}

#[test]
fn omitted_body_source_rejects_even_with_clean_owned_arena() {
    let mut snapshot = value(REFERENCE);
    let body = &mut snapshot["files"][0]["functions"][0]["body"];
    body["blocks"][0]["statements"].as_array_mut().expect("block statements").pop();
    body["statements"].as_array_mut().expect("statements").pop();
    body["expressions"].as_array_mut().expect("expressions").truncate(14);
    let map = sources(MAIN, VALUES);
    validate_declarations(&map, &raw(snapshot.clone())).expect("header-only boundary");
    rejects(snapshot, &map);
}

#[test]
fn faithful_operator_tokens_cannot_forge_precedence_or_associativity() {
    let map = single("precedence", PRECEDENCE_SOURCE);
    let mut precedence = value(PRECEDENCE);
    let original = precedence["files"][0]["functions"][0]["body"]["expressions"].clone();
    let mut plus = original[4].clone();
    plus["span"]["end"] = original[1]["span"]["end"].clone();
    plus["kind"]["rhs"] = json!(1);
    let mut times = original[3].clone();
    times["span"]["start"] = original[0]["span"]["start"].clone();
    times["kind"]["lhs"] = json!(2);
    times["kind"]["rhs"] = json!(3);
    precedence["files"][0]["functions"][0]["body"]["expressions"] =
        json!([original[0], original[1], plus, original[2], times]);
    rejects(precedence, &map);

    let mut associativity = value(PRECEDENCE);
    let original = associativity["files"][0]["functions"][1]["body"]["expressions"].clone();
    let mut right = original[4].clone();
    right["span"]["start"] = original[1]["span"]["start"].clone();
    right["kind"]["lhs"] = json!(1);
    right["kind"]["rhs"] = json!(2);
    let mut outer = original[2].clone();
    outer["span"]["end"] = original[3]["span"]["end"].clone();
    outer["kind"]["rhs"] = json!(3);
    associativity["files"][0]["functions"][1]["body"]["expressions"] =
        json!([original[0], original[1], original[3], right, outer]);
    rejects(associativity, &map);
}

#[test]
fn full_match_key_and_inferred_delimiters_are_authenticated() {
    for source in [
        MAIN.replace("\"Option.none\"", "'Option.none'"),
        MAIN.replace("match(found, {", "match(found; {"),
    ] {
        validate_declarations(&sources(&source, VALUES), &raw(value(REFERENCE)))
            .expect("deferred balanced body");
        rejects(value(REFERENCE), &sources(&source, VALUES));
    }
}

#[test]
fn reordered_statement_arena_and_weak_binding_claim_reject() {
    let mut snapshot = value(CONTROL);
    snapshot["files"][0]["functions"][0]["body"]["blocks"][0]["statements"]
        .as_array_mut()
        .expect("root")
        .swap(0, 1);
    rejects(snapshot, &single("control", CONTROL_SOURCE));
    let mut snapshot = value(OPERATIONS);
    let statements = snapshot["files"][0]["functions"][0]["body"]["statements"]
        .as_array_mut()
        .expect("statements");
    let upgrade = statements
        .iter_mut()
        .find(|statement| statement["kind"]["kind"] == "weak-upgrade")
        .expect("upgrade");
    upgrade["kind"]["binding"]["span"]["start"] = json!(0);
    rejects(snapshot, &single("operations", OPERATIONS_SOURCE));
}

#[test]
fn boolean_token_cannot_be_relabelled_as_identifier_reference() {
    let mut snapshot = value(OPERATIONS);
    let expressions = snapshot["files"][0]["functions"][0]["body"]["expressions"]
        .as_array_mut()
        .expect("expressions");
    let boolean = expressions
        .iter_mut()
        .find(|expression| {
            expression["kind"]["kind"] == "bool-literal" && expression["kind"]["value"] == true
        })
        .expect("true");
    boolean["kind"] =
        json!({ "kind": "reference", "name": { "text": "true", "span": boolean["span"] } });
    rejects(snapshot, &single("operations", OPERATIONS_SOURCE));
}

#[test]
fn typed_caller_cannot_bypass_per_expression_budget() {
    let mut snapshot = raw(value(CONTROL));
    let first = snapshot.files[0].functions[0].body.expressions[0].clone();
    snapshot.files[0].functions[0].body.expressions =
        vec![first; crate::v4::MAX_EXPRESSIONS_PER_FUNCTION + 1];
    let errors =
        verify_snapshot(snapshot, &single("control", CONTROL_SOURCE)).expect_err("first extra");
    assert_eq!(errors[0].code, "ZRYNA-Y5201");
}

#[test]
fn provider_advisory_cannot_hide_incomplete_source() {
    let mut snapshot = value(REFERENCE);
    snapshot["diagnostics"] = json!([{ "code": "ZRYNA-M7001", "severity": "error", "location": { "kind": "global" }, "message": "missing arguments", "guidance": "provide arguments" }]);
    snapshot["files"][0]["functions"][0]["body"]["blocks"][0]["statements"]
        .as_array_mut()
        .expect("root")
        .pop();
    rejects(snapshot, &sources(MAIN, VALUES));
}

#[test]
fn reserved_terminal_slot_and_order_are_deterministic() {
    let map = single("control", &"x".repeat(300));
    let run = || {
        let mut errors = super::diagnostic_order::Errors::default();
        for start in 0..300 {
            let span = map
                .verify_span(UntrustedSpan { file: 0, start, end: start + 1 })
                .expect("authenticated location");
            errors.push(super::DeclarationError::malformed(Some(span)));
        }
        assert!(errors.terminal());
        errors.finish()
    };
    let errors = run();
    assert_eq!(errors, run());
    assert_eq!(errors.len(), 256);
    assert_eq!(errors.last().expect("terminal").code, "ZRYNA-Y5201");
}

#[test]
fn raw_span_extremes_never_panic_full_verifier() {
    for field in ["start", "end"] {
        let mut snapshot = value(REFERENCE);
        snapshot["files"][0]["functions"][0]["body"]["expressions"][0]["span"][field] =
            json!(u32::MAX);
        let map = sources(MAIN, VALUES);
        let result = std::panic::catch_unwind(|| verify_snapshot(raw(snapshot), &map))
            .expect("no untrusted arithmetic panic");
        assert_eq!(result.expect_err("no seal")[0].code, "ZRYNA-Y5001");
    }
}

#[test]
fn missing_annotation_node_cannot_forge_a_concrete_container_operand() {
    let source = "function f(value: Vec<>): i32 { return 1; }";
    let span = |start: usize, end: usize| json!({ "file": 0, "start": start, "end": end });
    let ident =
        |text: &str, start: usize| json!({ "text": text, "span": span(start, start + text.len()) });
    let vector = source.find("Vec").expect("vector");
    let result = source.find("i32").expect("result");
    let parameter = source.find("value").expect("parameter");
    let open = source.find('{').expect("body");
    let close = source.find('}').expect("close");
    let ret = source.find("return").expect("return");
    let literal = source.find('1').expect("literal");
    let semi = source.find(';').expect("semicolon");
    let body_span = span(open, close + 1);
    let snapshot = json!({ "schema_version": 5, "diagnostics": [], "files": [{
        "id": 0, "path": "control.zry", "imports": [], "data_declarations": [],
        "type_syntax": [
            { "span": span(vector + 4, vector + 4), "kind": { "kind": "missing" } },
            { "span": span(vector, vector + 5), "kind": { "kind": "vec", "keyword_span": span(vector, vector + 3), "less_than_span": span(vector + 3, vector + 4), "argument": 0, "greater_than_span": span(vector + 4, vector + 5) } },
            { "span": span(result, result + 3), "kind": { "kind": "named", "name": ident("i32", result) } }
        ], "functions": [{ "span": span(0, source.len()), "export_span": null, "function_span": span(0, 8), "name": ident("f", 9), "type_parameters": null,
            "parameters": [{ "span": span(parameter, vector + 5), "name": ident("value", parameter), "type_syntax": 1 }], "result_type": 2,
            "body": { "span": body_span, "root_block": 0, "blocks": [{ "span": body_span, "open_brace_span": span(open, open + 1), "statements": [0], "close_brace_span": span(close, close + 1) }],
                "statements": [{ "span": span(ret, semi + 1), "kind": { "kind": "return", "keyword_span": span(ret, ret + 6), "value": 0, "semicolon_span": span(semi, semi + 1) } }],
                "expressions": [{ "span": span(literal, literal + 1), "kind": { "kind": "i32-literal", "spelling": "1" } }]
            }
        }]
    }] });
    let map = single("control", source);
    validate_declarations(&map, &raw(snapshot.clone()))
        .expect("declaration-only omission boundary");
    rejects(snapshot, &map);
}

fn return_claim(gap: &str) -> (Value, String) {
    let source = format!("function f(): bool {{ return{gap}true; }}");
    let span = |start: usize, end: usize| json!({"file": 0, "start": start, "end": end});
    let annotation = source.find("bool").expect("annotation");
    let open = source.find('{').expect("body");
    let close = source.find('}').expect("close");
    let ret = source.find("return").expect("return");
    let literal = source.rfind("true").expect("literal");
    let semi = source.find(';').expect("semicolon");
    let snapshot = json!({"schema_version": 5, "diagnostics": [], "files": [{
        "id": 0, "path": "return-lines.zry", "imports": [], "data_declarations": [],
        "type_syntax": [{"span": span(annotation, annotation + 4), "kind": {"kind": "named",
            "name": {"text": "bool", "span": span(annotation, annotation + 4)}}}],
        "functions": [{"span": span(0, source.len()), "export_span": null,
            "function_span": span(0, 8), "name": {"text": "f", "span": span(9, 10)},
            "type_parameters": null, "parameters": [], "result_type": 0,
            "body": {"span": span(open, close + 1), "root_block": 0,
                "blocks": [{"span": span(open, close + 1), "open_brace_span": span(open, open + 1),
                    "statements": [0], "close_brace_span": span(close, close + 1)}],
                "statements": [{"span": span(ret, semi + 1), "kind": {"kind": "return",
                    "keyword_span": span(ret, ret + 6), "value": 0, "semicolon_span": span(semi, semi + 1)}}],
                "expressions": [{"span": span(literal, literal + 4), "kind": {"kind": "bool-literal", "value": true}}]
            }}]
    }]});
    (snapshot, source)
}

#[test]
fn faithful_return_claim_cannot_join_a_value_across_any_line_terminator() {
    for terminator in ["\n", "\r", "\u{2028}", "\u{2029}"] {
        for gap in
            [terminator.to_owned(), format!("/*{terminator}*/"), format!("//comment{terminator}")]
        {
            let (snapshot, source) = return_claim(&gap);
            let map = single("return-lines", &source);
            validate_declarations(&map, &raw(snapshot.clone()))
                .expect("balanced whole declaration does not authenticate a Return AST");
            rejects(snapshot, &map);
        }
    }
    for gap in [" ", "\t", "/*ordinary comment*/"] {
        let (snapshot, source) = return_claim(gap);
        verify_snapshot(raw(snapshot), &single("return-lines", &source))
            .expect("same complete arena permits same-line trivia");
    }
}

fn offset_claims(value: &mut Value, offset: usize) {
    match value {
        Value::Object(object) => {
            if object.get("file") == Some(&json!(0))
                && object.contains_key("start")
                && object.contains_key("end")
            {
                for key in ["start", "end"] {
                    let original = object[key].as_u64().expect("span offset");
                    object.insert(key.into(), json!(original + offset as u64));
                }
            }
            for child in object.values_mut() {
                offset_claims(child, offset);
            }
        }
        Value::Array(array) => {
            for child in array {
                offset_claims(child, offset);
            }
        }
        _ => {}
    }
}

#[test]
fn line_comments_cannot_hide_a_declaration_after_unicode_or_ascii_terminators() {
    for terminator in ["\n", "\r", "\u{2028}", "\u{2029}"] {
        let prefix = format!("//comment{terminator}");
        let (mut complete, function) = return_claim(" ");
        let source = format!("{prefix}{function}");
        offset_claims(&mut complete, prefix.len());
        let map = single("return-lines", &source);
        verify_snapshot(raw(complete.clone()), &map)
            .expect("complete source inventory after a correctly terminated line comment");
        complete["files"][0]["functions"] = json!([]);
        complete["files"][0]["type_syntax"] = json!([]);
        rejects(complete, &map);
    }
}
