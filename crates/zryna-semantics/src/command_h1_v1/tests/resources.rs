use serde_json::{Value, json};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v4;

fn range(start: usize, end: usize) -> Value {
    json!({"file":0,"start":start,"end":end})
}

fn shift(value: &mut Value, amount: usize) {
    match value {
        Value::Object(object) => {
            if object.contains_key("file")
                && object.contains_key("start")
                && object.contains_key("end")
            {
                for key in ["start", "end"] {
                    object[key] = json!(
                        object[key].as_u64().expect("span")
                            + u64::try_from(amount).expect("bounded prefix")
                    );
                }
            } else {
                for child in object.values_mut() {
                    shift(child, amount);
                }
            }
        }
        Value::Array(children) => {
            for child in children {
                shift(child, amount);
            }
        }
        _ => {}
    }
}

fn input(count: usize, environment: bool) -> (Value, SourceMap) {
    let (body, bytes) = if environment {
        (
            include_str!("../../../../../tests/wasi-command-source-fixtures/environment-match.zry"),
            include_bytes!(
                "../../../../../tests/wasi-command-source-fixtures/environment-match.json"
            )
            .as_slice(),
        )
    } else {
        (
            include_str!("../../../../../tests/wasi-command-source-fixtures/pure-entry.zry"),
            include_bytes!("../../../../../tests/wasi-command-source-fixtures/pure-entry.json")
                .as_slice(),
        )
    };
    let mut raw: Value =
        serde_json::from_slice(bytes).expect("independent complete provider fixture");
    let mut prefix = String::new();
    let mut declarations = Vec::new();
    let mut types = Vec::new();
    let base_types = raw["files"][0]["type_syntax"].as_array().expect("types").len();
    for index in 0..count {
        let name = format!("Item{index}");
        let line = format!("interface {name} extends ZrynaStruct {{ field: bool; }}\n");
        let at = |token: &str| prefix.len() + line.find(token).expect("source token");
        let token = |text: &str| range(at(text), at(text) + text.len());
        declarations.push(json!({"span":range(prefix.len(), prefix.len()+line.len()-1), "export_span":null,
            "kind":{"kind":"struct","interface_span":token("interface"),
                "name":{"text":name,"span":token(&name)},"extends_span":token("extends"),
                "marker_span":token("ZrynaStruct"),"open_brace_span":token("{"),
                "close_brace_span":token("}"),"fields":[{
                    "span":range(at("field"),at(";")+1),"name":{"text":"field","span":token("field")},
                    "colon_span":token(":"),"type_syntax":base_types+index,"semicolon_span":token(";")
                }]}}));
        types.push(json!({"span":token("bool"),"kind":{"kind":"named","name":{"text":"bool","span":token("bool")}}}));
        prefix.push_str(&line);
    }
    shift(&mut raw, prefix.len());
    raw["files"][0]["data_declarations"] = json!(declarations);
    raw["files"][0]["type_syntax"].as_array_mut().expect("types").extend(types);
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "src/main.zry".into(),
        text: prefix + body,
    }])
    .expect("bounded complete source");
    (raw, sources)
}

#[test]
fn exact_source_nominal_budget_preserves_m3_and_reserves_one_command_builtin_slot() {
    let limit = v4::MAX_DATA_DECLARATIONS_PER_MODULE;
    for environment in [false, true] {
        let (raw, sources) = input(limit, environment);
        let syntax = v4::verify_snapshot(
            v4::decode_snapshot(&serde_json::to_vec(&raw).expect("JSON"))
                .expect("exact source count"),
            &sources,
        )
        .expect("independent authenticated nominal inventory");
        let source =
            zryna_syntax::command_h1_v1::admit(&syntax, &sources).expect("complete source");
        let program = super::lower(&source, &sources).expect("exact nominal command budget");
        assert_eq!(
            program.verified_ir().modules().next().expect("module").data_declarations() as usize,
            limit + usize::from(environment)
        );
        if !environment {
            let input = crate::data_ownership_v1::SemanticInput::try_new(
                &syntax,
                &sources,
                sources.verify_file_id(0).expect("file"),
            )
            .expect("M3 input");
            let ordinary =
                crate::data_ownership_v1::lower(input).expect("unchanged exact ordinary M3 budget");
            assert_eq!(
                ordinary.modules().next().expect("module").data_declarations() as usize,
                limit
            );
        }
    }
    let (raw, _) = input(limit + 1, true);
    assert!(
        v4::decode_snapshot(&serde_json::to_vec(&raw).expect("JSON")).is_err(),
        "first extra source nominal remains rejected"
    );
}
