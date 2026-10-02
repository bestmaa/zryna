use super::{
    admit, decode_snapshot, environment, json, map, shift_spans, span, verified, verify_snapshot,
};

#[test]
fn independent_call_trivia_matches_the_locked_source_scanner() {
    for separator in [
        " ",
        "\t",
        "\n",
        "\r",
        "\u{000b}",
        "\u{000c}",
        "\u{0085}",
        "\u{00a0}",
        "\u{1680}",
        "\u{2000}",
        "\u{200a}",
        "\u{200b}",
        "\u{2028}",
        "\u{2029}",
        "\u{202f}",
        "\u{205f}",
        "\u{3000}",
        "\u{feff}",
        "/*comment*/",
        "//comment\u{2028}",
    ] {
        let (mut text, mut raw) = environment("MODE");
        let end = text.find("environmentLookup").expect("callee") + 17;
        text.insert_str(end, separator);
        shift_spans(&mut raw, end, separator.len());
        raw["files"][0]["functions"][0]["body"]["expressions"][1]["kind"]["callee"]["span"]["end"] =
            json!(end);
        let sources = map(&text);
        admit(&verified(&raw, &sources), &sources).expect("exact pinned-source trivia");
    }
    for separator in ["\u{001c}", "\u{180e}", "\u{2060}", "//comment\u{0085}"] {
        let (mut text, mut raw) = environment("MODE");
        let end = text.find("environmentLookup").expect("callee") + 17;
        text.insert_str(end, separator);
        shift_spans(&mut raw, end, separator.len());
        raw["files"][0]["functions"][0]["body"]["expressions"][1]["kind"]["callee"]["span"]["end"] =
            json!(end);
        let sources = map(&text);
        assert!(admit(&verified(&raw, &sources), &sources).is_err(), "{separator:?}");
    }
}

#[test]
fn real_outcome_container_annotations_and_source_trivia_are_admitted() {
    for (keyword, kind) in [
        ("Vec", "vec"),
        ("Shared", "shared"),
        ("Weak", "weak"),
        ("Borrow", "borrow"),
        ("BorrowMut", "borrow-mut"),
    ] {
        for trivia in ["", " /* comment */ ", "\u{feff}"] {
            let (mut text, mut raw) = environment("MODE");
            let start = text.find("bool").expect("result");
            let annotation = format!("{keyword}{trivia}<{trivia}EnvLookupV1{trivia}>");
            text.replace_range(start..start + 4, &annotation);
            shift_spans(&mut raw, start + 4, annotation.len() - 4);
            let named = text.find("EnvLookupV1").expect("outcome");
            let less = start + keyword.len() + trivia.len();
            let greater = start + annotation.len() - 1;
            raw["files"][0]["type_syntax"] = json!([
                {"span":span(named,named+11),"kind":{"kind":"named","name":{"text":"EnvLookupV1","span":span(named,named+11)}}},
                {"span":span(start,greater+1),"kind":{"kind":kind,"keyword_span":span(start,start+keyword.len()),"less_than_span":span(less,less+1),"argument":0,"greater_than_span":span(greater,greater+1)}}
            ]);
            raw["files"][0]["functions"][0]["result_type"] = json!(1);
            let sources = map(&text);
            admit(&verified(&raw, &sources), &sources).expect("actual source annotation context");
        }
    }
}

#[test]
fn parameter_outcome_context_retains_an_admitted_trailing_comma() {
    for comma in ["", ","] {
        let (mut text, mut raw) = environment("MODE");
        let start = text.find("main()").expect("entry") + 5;
        let parameter = format!("value: EnvLookupV1{comma}");
        text.insert_str(start, &parameter);
        shift_spans(&mut raw, start, parameter.len());
        let result = text.find("bool").expect("result");
        text.replace_range(result..result + 4, "EnvLookupV1");
        shift_spans(&mut raw, result + 4, 7);
        raw["files"][0]["type_syntax"][0] = json!({
            "span":span(result,result+11),"kind":{"kind":"named","name":{"text":"EnvLookupV1","span":span(result,result+11)}}
        });
        let token = start + 7;
        raw["files"][0]["type_syntax"].as_array_mut().expect("types").push(json!({
            "span":span(token,token+11),"kind":{"kind":"named","name":{"text":"EnvLookupV1","span":span(token,token+11)}}
        }));
        raw["files"][0]["functions"][0]["parameters"] = json!([{
            "span":span(start,token+11),"name":{"text":"value","span":span(start,start+5)},"type_syntax":1
        }]);
        let sources = map(&text);
        admit(&verified(&raw, &sources), &sources).expect("exact parameter context");
    }
}

#[test]
fn object_property_cannot_be_laundered_as_an_owned_local_type_annotation() {
    let (mut text, mut raw) = environment("MODE");
    let start = text.find("return").expect("statement");
    let inserted = "const obj = { value: EnvLookupV1 }; ";
    text.insert_str(start, inserted);
    shift_spans(&mut raw, start, inserted.len());
    let name = text.find("value:").expect("property");
    let ty = text.find("EnvLookupV1").expect("property value");
    let equals = text[start..].find('=').expect("object initializer") + start;
    let semicolon = text[start..].find(';').expect("object semicolon") + start;
    raw["files"][0]["type_syntax"].as_array_mut().expect("types").push(json!({
        "span":span(ty,ty+11),"kind":{"kind":"named","name":{"text":"EnvLookupV1","span":span(ty,ty+11)}}
    }));
    let body = &mut raw["files"][0]["functions"][0]["body"];
    body["expressions"].as_array_mut().expect("expressions").push(json!({
        "span":span(ty,ty+11),"kind":{"kind":"reference","name":{"text":"EnvLookupV1","span":span(ty,ty+11)}}
    }));
    body["statements"].as_array_mut().expect("statements").push(json!({
        "span":span(start,semicolon+1),"kind":{"kind":"local-declaration","keyword_span":span(start,start+5),"mutable":false,
        "name":{"text":"value","span":span(name,name+5)},"type_syntax":1,"equals_span":span(equals,equals+1),"initializer":2,"semicolon_span":span(semicolon,semicolon+1)}
    }));
    canonical_locals(body);
    let sources = map(&text);
    let candidate =
        decode_snapshot(&serde_json::to_vec(&raw).expect("raw fixture")).expect("candidate DTO");
    assert!(
        verify_snapshot(candidate, &sources).is_err(),
        "the real '=' precedes the claimed local name/type and violates authenticated child ordering"
    );
}

#[test]
fn assignment_property_cannot_launder_its_value_as_a_local_type() {
    for (inserted, accepted) in [
        ("const obj = { value: EnvLookupV1 = 1 }; ", false),
        ("const value: EnvLookupV1 = 1; ", true),
    ] {
        let (mut text, mut raw) = environment("MODE");
        let start = text.find("return").expect("statement");
        text.insert_str(start, inserted);
        shift_spans(&mut raw, start, inserted.len());
        let name = text.find("value:").expect("property");
        let ty = text.find("EnvLookupV1").expect("property value");
        let equals = ty + 12;
        let literal = equals + 2;
        let semicolon = text[start..].find(';').expect("object semicolon") + start;
        raw["files"][0]["type_syntax"].as_array_mut().expect("types").push(json!({
        "span":span(ty,ty+11),"kind":{"kind":"named","name":{"text":"EnvLookupV1","span":span(ty,ty+11)}}
    }));
        let body = &mut raw["files"][0]["functions"][0]["body"];
        body["expressions"].as_array_mut().expect("expressions").push(json!({
            "span":span(literal,literal+1),"kind":{"kind":"i32-literal","spelling":"1"}
        }));
        body["statements"].as_array_mut().expect("statements").push(json!({
        "span":span(start,semicolon+1),"kind":{"kind":"local-declaration","keyword_span":span(start,start+5),"mutable":false,
        "name":{"text":"value","span":span(name,name+5)},"type_syntax":1,"equals_span":span(equals,equals+1),"initializer":2,"semicolon_span":span(semicolon,semicolon+1)}
    }));
        canonical_locals(body);
        let sources = map(&text);
        let syntax = verified(&raw, &sources);
        assert_eq!(
            admit(&syntax, &sources).is_ok(),
            accepted,
            "only the real local declaration has exact keyword-to-name source context"
        );
    }
}

fn canonical_locals(body: &mut serde_json::Value) {
    body["statements"].as_array_mut().expect("statements").swap(0, 1);
    let expressions = body["expressions"].as_array_mut().expect("expressions");
    expressions.swap(0, 2);
    expressions.swap(1, 2);
    expressions[2]["kind"]["arguments"] = json!([1]);
    body["statements"][0]["kind"]["initializer"] = json!(0);
    body["statements"][1]["kind"]["value"] = json!(2);
    body["blocks"][0]["statements"] = json!([0, 1]);
}

#[test]
fn quoted_or_commented_outcome_annotation_cannot_acquire_source_authority() {
    let (text, mut raw) = environment("MODE");
    let text = text.replace("bool", "EnvLookupV1").replace("environmentLookup(\"MODE\")", "true");
    let result = text.find("EnvLookupV1").expect("result");
    let literal = text.find("true").expect("literal");
    let body_start = text.find('{').expect("body");
    let return_start = text.find("return").expect("statement");
    let end = text.len();
    raw["files"][0]["type_syntax"] = json!([{
        "span":span(result,result+11),"kind":{"kind":"named","name":{"text":"EnvLookupV1","span":span(result,result+11)}}
    }]);
    let function = &mut raw["files"][0]["functions"][0];
    function["span"] = span(0, end);
    function["body"] = json!({
        "span":span(body_start,end),"root_block":0,
        "blocks":[{"span":span(body_start,end),"open_brace_span":span(body_start,body_start+1),"close_brace_span":span(end-1,end),"statements":[0]}],
        "statements":[{"span":span(return_start,literal+5),"kind":{"kind":"return","keyword_span":span(return_start,return_start+6),"value":0,"semicolon_span":span(literal+4,literal+5)}}],
        "expressions":[{"span":span(literal,literal+4),"kind":{"kind":"bool-literal","value":true}}]
    });
    let sources = map(&text);
    admit(&verified(&raw, &sources), &sources).expect("real source outcome type");
    for (prefix, suffix) in [("/*", "*/"), ("'", "'")] {
        let mut candidate = raw.clone();
        shift_spans(&mut candidate, 0, prefix.len());
        let sources = map(&format!("{prefix}{text}{suffix}"));
        assert!(
            admit(&verified(&candidate, &sources), &sources).is_err(),
            "lexically absent outcome type"
        );
    }
}
