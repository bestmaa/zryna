use serde_json::{Value, json};
use zryna_source::{SourceFileInput, SourceMap};

use super::{RawProjectSyntaxSnapshot, SyntaxDecodeError, decode_snapshot, validate_declarations};

const REFERENCE: &[u8] = include_bytes!("../../../../tests/m7-syntax-fixtures/reference.json");
const MAIN: &str = include_str!("../../../../tests/m7-syntax-fixtures/main.zry");
const VALUES: &str = include_str!("../../../../tests/m7-syntax-fixtures/values.zry");

fn sources(main: &str, values: &str) -> SourceMap {
    SourceMap::build(vec![
        SourceFileInput { path: "main.zry".into(), text: main.into() },
        SourceFileInput { path: "values.zry".into(), text: values.into() },
    ])
    .expect("fixture sources")
}

fn reference() -> RawProjectSyntaxSnapshot {
    decode_snapshot(REFERENCE).expect("independent wire reference")
}

fn changed(mut update: impl FnMut(&mut Value)) -> RawProjectSyntaxSnapshot {
    let mut value: Value = serde_json::from_slice(REFERENCE).expect("reference JSON");
    update(&mut value);
    decode_snapshot(&serde_json::to_vec(&value).expect("mutated JSON")).expect("structural DTO")
}

#[test]
fn reference_headers_data_types_and_complete_module_inventory() {
    validate_declarations(&sources(MAIN, VALUES), &reference())
        .expect("source-faithful declarations");
    assert!(reference().files[1].functions[0].export_span.is_some());
    assert!(reference().files[1].functions[0].type_parameters.is_some());
    // An exported template remains a raw declaration. There is no executable export inventory.
}

#[test]
fn fixed_hostile_wire_and_source_claim_catalogues() {
    let cases: Value = serde_json::from_slice(include_bytes!(
        "../../../../tests/m7-syntax-fixtures/hostile-wire.json"
    ))
    .expect("hostile wire catalogue");
    for case in cases.as_array().expect("wire cases") {
        let error = decode_snapshot(case["wire"].as_str().expect("wire").as_bytes())
            .expect_err("independent malformed wire");
        assert_eq!(error.code(), case["code"].as_str().expect("code"), "{}", case["id"]);
    }
    let cases: Value = serde_json::from_slice(include_bytes!(
        "../../../../tests/m7-syntax-fixtures/hostile-declarations.json"
    ))
    .expect("hostile source catalogue");
    for case in cases.as_array().expect("source cases") {
        let raw = changed(|value| {
            *value
                .pointer_mut(case["pointer"].as_str().expect("pointer"))
                .expect("existing independent target") = case["value"].clone();
        });
        let error = validate_declarations(&sources(MAIN, VALUES), &raw)
            .expect_err("independent hostile declaration");
        assert_eq!(error.code, case["code"].as_str().expect("code"), "{}", case["id"]);
    }
}

#[test]
fn decoded_import_token_span_extremes_reject_without_panicking() {
    let authority = sources(MAIN, VALUES);
    let token = reference().files[0].imports[0].specifier.token_span;
    for (start, end) in [(u32::MAX, token.end), (token.start, u32::MAX), (u32::MAX, u32::MAX)] {
        let raw = changed(|value| {
            value["files"][0]["imports"][0]["specifier"]["token_span"]["start"] = json!(start);
            value["files"][0]["imports"][0]["specifier"]["token_span"]["end"] = json!(end);
        });
        let outcome = std::panic::catch_unwind(|| validate_declarations(&authority, &raw))
            .expect("decoded hostile import spans must not panic");
        assert_eq!(outcome.expect_err("malformed import token span").code, "ZRYNA-Y5001");
    }
}

#[test]
fn unknown_fields_missing_nullable_fields_duplicate_keys_and_trailing_input_reject() {
    let updates: [fn(&mut Value); 5] = [
        |value: &mut Value| {
            value["files"][0]["unknown"] = json!(true);
        },
        |value: &mut Value| {
            value["files"][0]["functions"][0]
                .as_object_mut()
                .expect("function")
                .remove("type_parameters");
        },
        |value: &mut Value| {
            value["files"][0]["imports"][0]["bindings"][0]
                .as_object_mut()
                .expect("binding")
                .remove("as_span");
        },
        |value: &mut Value| {
            value["schema_version"] = json!(2);
        },
        |value: &mut Value| {
            value["files"][1]["data_declarations"][0]["type_parameters"]["parameters"] = json!([]);
        },
    ];
    for update in updates {
        let mut value: Value = serde_json::from_slice(REFERENCE).expect("reference");
        update(&mut value);
        assert_eq!(
            decode_snapshot(&serde_json::to_vec(&value).expect("mutation")),
            Err(SyntaxDecodeError::InvalidSnapshot)
        );
    }
    for bytes in [
        br#"{"schema_version":5,"schema_version":5,"files":[],"diagnostics":[]}"#.as_slice(),
        br#"{"schema_version":5,"files":[],"diagnostics":[],"files":[]}"#.as_slice(),
        br#"{"schema_version":5,"files":[],"diagnostics":[]} {}"#.as_slice(),
    ] {
        assert_eq!(decode_snapshot(bytes), Err(SyntaxDecodeError::InvalidSnapshot));
    }
}

#[test]
fn missing_or_reordered_declarations_and_hidden_top_level_input_reject() {
    let authority = sources(MAIN, VALUES);
    let omitted = changed(|value| {
        value["files"][1]["data_declarations"].as_array_mut().expect("declarations").remove(0);
    });
    assert_eq!(
        validate_declarations(&authority, &omitted).expect_err("omitted declaration").code,
        "ZRYNA-Y5001"
    );
    let reordered = changed(|value| {
        value["files"][1]["functions"].as_array_mut().expect("functions").swap(0, 1);
    });
    assert_eq!(
        validate_declarations(&authority, &reordered).expect_err("reordered functions").code,
        "ZRYNA-Y5001"
    );
    for suffix in [" class Hidden<T> {}", " ;", " function extra(): i32 { return 1; }"] {
        assert_eq!(
            validate_declarations(&sources(&format!("{MAIN}{suffix}"), VALUES), &reference())
                .expect_err("unconsumed source")
                .code,
            "ZRYNA-Y5001"
        );
    }
}

#[test]
fn bound_error_requires_authentic_source_spelling_and_span() {
    let raw = changed(|value| {
        value["files"][1]["data_declarations"][0]["type_parameters"]["parameters"][0]["bound"]["text"] =
            json!("OtherValue");
    });
    assert_eq!(
        validate_declarations(&sources(MAIN, VALUES), &raw).expect_err("forged token").code,
        "ZRYNA-Y5001"
    );
    let source = VALUES.replacen("ZrynaValue", "OtherValue", 1);
    let error = validate_declarations(&sources(MAIN, &source), &raw).expect_err("unknown bound");
    assert_eq!(error.code, "ZRYNA-D7001");
    let span = error.span.expect("authenticated bound");
    assert_eq!(&source[span.start() as usize..span.end() as usize], "OtherValue");
}

#[test]
fn hostile_type_edges_source_files_utf8_boundaries_and_tokens_reject() {
    for raw in [
        changed(|value| {
            value["files"][0]["id"] = json!(1);
        }),
        changed(|value| {
            value["files"][0]["path"] = json!("foreign.zry");
        }),
        changed(|value| {
            value["files"][0]["type_syntax"][2]["kind"]["type_arguments"]["arguments"][0] =
                json!(4);
        }),
        changed(|value| {
            value["files"][1]["data_declarations"][0]["span"]["start"] = json!(4);
        }),
        changed(|value| {
            value["files"][1]["data_declarations"][0]["kind"]["interface_span"]["end"] = json!(42);
        }),
    ] {
        assert_eq!(
            validate_declarations(&sources(MAIN, VALUES), &raw).expect_err("hostile claim").code,
            "ZRYNA-Y5001"
        );
    }
}

#[test]
fn duplicate_and_reserved_type_parameter_names_reject_at_authenticated_name() {
    let raw = changed(|value| {
        value["files"][1]["functions"][1]["type_parameters"]["parameters"][1]["name"]["text"] =
            json!("T");
    });
    let source = VALUES.replacen(
        "function select<T extends ZrynaValue, E extends",
        "function select<T extends ZrynaValue, T extends",
        1,
    );
    assert_eq!(
        validate_declarations(&sources(MAIN, &source), &raw).expect_err("duplicate parameter").code,
        "ZRYNA-D7001"
    );
    let raw = changed(|value| {
        value["files"][1]["data_declarations"][0]["type_parameters"]["parameters"][0]["bound"]["span"]
            ["file"] = json!(0);
    });
    assert_eq!(
        validate_declarations(&sources(MAIN, VALUES), &raw).expect_err("foreign bound span").code,
        "ZRYNA-Y5001"
    );
}

#[test]
fn declaration_admission_does_not_authenticate_function_body_arenas() {
    let raw = changed(|value| {
        value["files"][0]["functions"][0]["body"]["expressions"][0]["span"]["start"] = json!(0);
    });
    validate_declarations(&sources(MAIN, VALUES), &raw).expect("only declarations are checked");
    // No syntax seal is produced: body arena provenance must be verified by a later complete gate.
}
