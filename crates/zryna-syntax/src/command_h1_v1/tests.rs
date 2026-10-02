use serde_json::{Value, json};
use zryna_source::{SourceFileInput, SourceMap};

use super::{admit, identifiers};
use crate::v4::{ProjectSyntaxSnapshot, decode_snapshot, verify_snapshot};

mod contexts;
mod coverage;

fn map(text: &str) -> SourceMap {
    SourceMap::build(vec![SourceFileInput {
        path: "src/main.zry".to_owned(),
        text: text.to_owned(),
    }])
    .expect("bounded fixture")
}

fn verified(raw: &Value, sources: &SourceMap) -> ProjectSyntaxSnapshot {
    let bytes = serde_json::to_vec(raw).expect("fixture JSON");
    verify_snapshot(decode_snapshot(&bytes).expect("independent raw fixture"), sources)
        .expect("authenticated v4 fixture")
}

fn span(start: usize, end: usize) -> Value {
    json!({"file":0,"start":start,"end":end})
}

fn environment(key: &str) -> (String, Value) {
    let spelling = format!("\"{key}\"");
    expression(&spelling, json!({"kind":"string-literal","spelling":spelling}))
}

fn expression(argument: &str, mut argument_kind: Value) -> (String, Value) {
    let source =
        format!("export function main(): bool {{ return environmentLookup({argument}); }}");
    let body = source.find('{').expect("body");
    let return_start = source.find("return").expect("return");
    let call = source.find("environmentLookup").expect("callee");
    let literal = call + 18;
    let literal_end = literal + argument.len();
    if argument_kind["kind"] == "reference" {
        argument_kind["name"] = json!({"text":argument,"span":span(literal,literal_end)});
    }
    let close_paren = literal_end;
    let semicolon = close_paren + 1;
    let result = source.find("bool").expect("entry type");
    let end = source.len();
    let raw = json!({
        "schema_version":4,"diagnostics":[],"files":[{
            "id":0,"path":"src/main.zry","imports":[],"data_declarations":[],
            "type_syntax":[{"span":span(result,result+4),"kind":{"kind":"named","name":{"text":"bool","span":span(result,result+4)}}}],
            "functions":[{
                "span":span(0,end),"export_span":span(0,6),"function_span":span(7,15),
                "name":{"text":"main","span":span(16,20)},"parameters":[],"result_type":0,
                "body":{
                    "span":span(body,end),"root_block":0,
                    "blocks":[{"span":span(body,end),"open_brace_span":span(body,body+1),"close_brace_span":span(end-1,end),"statements":[0]}],
                    "statements":[{"span":span(return_start,semicolon+1),"kind":{"kind":"return","keyword_span":span(return_start,return_start+6),"value":1,"semicolon_span":span(semicolon,semicolon+1)}}],
                    "expressions":[
                        {"span":span(literal,literal_end),"kind":argument_kind},
                        {"span":span(call,close_paren+1),"kind":{"kind":"call","callee":{"text":"environmentLookup","span":span(call,call+17)},"open_paren_span":span(call+17,call+18),"arguments":[0],"close_paren_span":span(close_paren,close_paren+1)}}
                    ]
                }
            }]
        }]
    });
    (source, raw)
}

#[test]
fn literal_environment_requirement_retains_exact_source_authority() {
    for key in ["MODE", "é界", &"a".repeat(64)] {
        let (text, raw) = environment(key);
        let sources = map(&text);
        let syntax = verified(&raw, &sources);
        let admitted = admit(&syntax, &sources).expect("one exact literal source requirement");
        let requirement = admitted.environment().expect("nonempty source requirement");
        assert_eq!(requirement.key(), key);
        assert_eq!(requirement.function_index(), 0);
        assert_eq!(requirement.expression_index(), 1);
        assert!(sources.resolve(requirement.call_span()).is_ok());
        assert!(admitted.is_bound_to(&sources));
        assert!(
            !admitted.is_bound_to(&map(&text)),
            "same bytes do not replace source-map authority"
        );
    }
}

#[test]
fn first_extra_key_byte_and_escaped_or_computed_keys_reject() {
    for key in ["", &"a".repeat(65), &"é".repeat(33)] {
        let (text, raw) = environment(key);
        let sources = map(&text);
        let syntax = verified(&raw, &sources);
        assert_eq!(admit(&syntax, &sources).expect_err("key bound").code(), "ZRYNA-Y4100");
    }
    assert!(super::literal_key("\"M\\ODE\"").is_err());
    assert!(super::literal_key("\"A\0B\"").is_err());
    let (text, raw) = expression("MODE", json!({"kind":"reference"}));
    let sources = map(&text);
    let syntax = verified(&raw, &sources);
    assert!(admit(&syntax, &sources).is_err(), "computed key is syntactically valid but forbidden");
}

#[test]
fn extra_omitted_call_and_shadowing_reject_with_an_otherwise_valid_snapshot() {
    let (text, raw) = environment("MODE");
    for suffix in
        ["\nenvironmentLookup(\"MODE\");", "\nfunction environmentLookup(): bool { return true; }"]
    {
        let sources = map(&format!("{text}{suffix}"));
        let syntax = verified(&raw, &sources);
        assert!(
            admit(&syntax, &sources).is_err(),
            "provider cannot omit additional reserved source syntax"
        );
    }
    let sources = map(&text.replace("MODE", "SEEN"));
    let raw =
        decode_snapshot(&serde_json::to_vec(&raw).expect("JSON")).expect("independent candidate");
    assert!(
        verify_snapshot(raw, &sources).is_err(),
        "changed literal bytes cannot retain old authority"
    );
}

#[test]
fn omitted_source_effect_and_reserved_name_are_rejected_independently_of_provider() {
    let (text, mut raw) = environment("MODE");
    raw["files"][0]["functions"] = json!([]);
    raw["files"][0]["type_syntax"] = json!([]);
    let sources = map(&text);
    let syntax = verified(&raw, &sources);
    assert!(admit(&syntax, &sources).is_err(), "an omitted call cannot become a pure command");
    let shadow = "const EnvLookupV1 = 1;";
    let sources = map(shadow);
    assert!(admit(&verified(&raw, &sources), &sources).is_err());
    let comment = "// environmentLookup\n/* EnvLookupV1 */";
    let sources = map(comment);
    assert!(
        admit(&verified(&raw, &sources), &sources)
            .expect("trivia has no effects")
            .environment()
            .is_none()
    );
}

#[test]
fn lexical_inventory_handles_quotes_unicode_comments_and_marker_substrings() {
    for text in [
        "'environmentLookup'",
        "\"EnvLookupV1\"",
        "/* environmentLookup */",
        "// EnvLookupV1\r\n",
        "myenvironmentLookup",
        "environmentLookupSuffix",
    ] {
        assert!(identifiers::inventory(text).expect("lexical fixture").is_empty());
    }
    for separator in ["\n", "\r", "\u{2028}", "\u{2029}"] {
        let text = format!("// comment{separator}environmentLookup");
        assert_eq!(identifiers::inventory(&text).expect("line comment").len(), 1);
    }
    for text in ["/*", "'unterminated", "`environmentLookup`", "\\u0065nvironmentLookup"] {
        assert!(identifiers::inventory(text).is_err());
    }
    for text in [
        "object.environmentLookup(\"MODE\")",
        "object. /*comment*/ environmentLookup(\"MODE\")",
        "object.\u{2003}environmentLookup(\"MODE\")",
    ] {
        assert!(identifiers::inventory(text).is_err());
    }
}

#[test]
fn reserved_suffix_cannot_be_authenticated_as_a_standalone_callee() {
    for prefix in ["$", "1", "é", "_", "my"] {
        let (mut text, mut raw) = environment("MODE");
        let call = text.find("environmentLookup").expect("callee");
        text.insert_str(call, prefix);
        shift_spans(&mut raw, call, prefix.len());
        let sources = map(&text);
        let syntax = verified(&raw, &sources);
        assert!(admit(&syntax, &sources).is_err(), "provider cannot authenticate a token suffix");
        assert!(
            identifiers::source_call(
                &text,
                u32::try_from(call + prefix.len()).expect("small fixture"),
                u32::try_from(call + prefix.len() + 17).expect("small fixture")
            )
            .is_err()
        );
    }
}

fn shift_spans(value: &mut Value, at: usize, length: usize) {
    match value {
        Value::Array(items) => {
            for item in items {
                shift_spans(item, at, length);
            }
        }
        Value::Object(fields) => {
            for (name, item) in fields {
                if matches!(name.as_str(), "start" | "end") {
                    let position = item.as_u64().expect("fixture span");
                    if position >= u64::try_from(at).expect("small fixture") {
                        *item = json!(position + u64::try_from(length).expect("small prefix"));
                    }
                } else {
                    shift_spans(item, at, length);
                }
            }
        }
        _ => {}
    }
}

#[test]
fn reserved_declaration_cannot_be_laundered_as_a_used_named_result_type() {
    for inserted in ["; const EnvLookupV1 = 1;", "; object.EnvLookupV1;", "; EnvLookupV1;"] {
        let (mut text, mut raw) = environment("MODE");
        let after = text.find("bool").expect("result") + 4;
        text.insert_str(after, inserted);
        shift_spans(&mut raw, after, inserted.len());
        let token = text.find("EnvLookupV1").expect("reserved source name");
        raw["files"][0]["type_syntax"][0] = json!({
            "span":span(token,token+11),
            "kind":{"kind":"named","name":{"text":"EnvLookupV1","span":span(token,token+11)}}
        });
        let sources = map(&text);
        let syntax = verified(&raw, &sources);
        assert!(
            admit(&syntax, &sources).is_err(),
            "a used type node cannot launder omitted syntax"
        );
    }
    let (mut text, mut raw) = environment("MODE");
    let token = text.find("bool").expect("result");
    text.replace_range(token..token + 4, "EnvLookupV1");
    shift_spans(&mut raw, token + 4, 7);
    raw["files"][0]["type_syntax"][0] = json!({
        "span":span(token,token+11),
        "kind":{"kind":"named","name":{"text":"EnvLookupV1","span":span(token,token+11)}}
    });
    let sources = map(&text);
    admit(&verified(&raw, &sources), &sources).expect("real annotation passes the source boundary");
    // This authenticates syntax context only: the later command entry verifier rejects
    // this owned result type because the sole public entry must return bool.
}
