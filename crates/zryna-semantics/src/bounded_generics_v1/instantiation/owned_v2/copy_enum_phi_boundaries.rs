//! Source-byte authentication and independent exact-byte ASI/Unicode boundaries.
use super::{SourceMap, decode_snapshot, forms, source_inputs, verify_snapshot};
use serde_json::{Value, json};
use zryna_source::SourceFileInput;
use zryna_syntax::v5::RawProjectSyntaxSnapshot;

#[test]
fn frozen_copy_enum_keywords_literals_and_assignment_bytes_are_authenticated() {
    forms(|files, bytes| {
        let dto = decode_snapshot(bytes).expect("frozen DTO");
        let original = source_inputs(files);
        let sources = SourceMap::build(original.clone()).expect("original source");
        verify_snapshot(dto.clone(), &sources).expect("pristine source authority");
        let affected = usize::from(files.len() == 2);
        for (from, to) in [
            ("if\n", "іf\n"),
            ("if\n", "i\nf\n"),
            ("if\n", "let\n"),
            ("else", "elsе"),
            ("一", "二"),
            ("item=Option.some<i32>(11)", "item=Option.some<i32>(12)"),
            ("left=right", "right=left"),
        ] {
            let mut hostile = original.clone();
            hostile[affected].text = hostile[affected].text.replacen(from, to, 1);
            assert_ne!(hostile[affected].text, original[affected].text, "mutation present");
            let map = SourceMap::build(hostile).expect("valid hostile UTF8");
            let errors = verify_snapshot(dto.clone(), &map).expect_err("no forged source seal");
            assert!(errors.iter().all(|error| error.code == "ZRYNA-Y5001"));
            verify_snapshot(dto.clone(), &sources).expect("pristine recovery after each change");
        }
    });
}

fn assignment_reference(raw: &Value) -> (usize, usize, usize) {
    for (fi, file) in raw["files"].as_array().expect("files").iter().enumerate() {
        for (function, value) in file["functions"].as_array().expect("functions").iter().enumerate()
        {
            let body = &value["body"];
            for statement in body["statements"].as_array().expect("statements") {
                if statement["kind"]["kind"] != "assignment" {
                    continue;
                }
                let index = usize::try_from(statement["kind"]["target"].as_u64().expect("target"))
                    .expect("target index");
                if body["expressions"][index]["kind"]["name"]["text"] == "item" {
                    return (fi, function, index);
                }
            }
        }
    }
    panic!("exact assignment target reference");
}

#[test]
fn frozen_copy_enum_assignment_reference_has_authoritative_utf8_byte_spans() {
    forms(|files, bytes| {
        let dto = decode_snapshot(bytes).expect("frozen DTO");
        let sources = SourceMap::build(source_inputs(files)).expect("exact sources");
        verify_snapshot(dto.clone(), &sources).expect("authentic reference roles");
        let raw = serde_json::to_value(&dto).expect("raw claims");
        let (file, function, expression) = assignment_reference(&raw);
        let reference =
            &raw["files"][file]["functions"][function]["body"]["expressions"][expression];
        let span = &reference["kind"]["name"]["span"];
        let start = usize::try_from(span["start"].as_u64().expect("start")).expect("start");
        let end = usize::try_from(span["end"].as_u64().expect("end")).expect("end");
        assert_eq!(reference["span"], *span);
        assert_eq!(&files[file].1[start..end], "item");
        assert!(files[file].1[..start].contains("保持"));
        assert!(start > files[file].1[..start].chars().count(), "offsets are UTF8 bytes");
        let mut hostile = raw.clone();
        hostile["files"][file]["functions"][function]["body"]["expressions"][expression]["kind"]
            ["name"]["span"]["start"] = json!(start + 1);
        let changed: RawProjectSyntaxSnapshot =
            serde_json::from_value(hostile).expect("raw span claim, not a new syntax authority");
        let errors = verify_snapshot(changed, &sources).expect_err("shifted consuming name");
        assert!(errors.iter().all(|error| error.code == "ZRYNA-Y5001"));
        verify_snapshot(dto, &sources).expect("original assignment span recovery");
    });
}

#[test]
fn frozen_copy_enum_literal_span_cannot_split_a_unicode_codepoint() {
    forms(|files, bytes| {
        let dto = decode_snapshot(bytes).expect("frozen DTO");
        let sources = SourceMap::build(source_inputs(files)).expect("exact sources");
        verify_snapshot(dto.clone(), &sources).expect("authentic literal bytes");
        let mut raw = serde_json::to_value(&dto).expect("raw claims");
        let mut changed = false;
        for file in raw["files"].as_array_mut().expect("files") {
            let id = usize::try_from(file["id"].as_u64().expect("file id")).expect("file");
            for function in file["functions"].as_array_mut().expect("functions") {
                for expression in
                    function["body"]["expressions"].as_array_mut().expect("expressions")
                {
                    if expression["kind"]["spelling"] != "\"保持\"" {
                        continue;
                    }
                    let start = expression["span"]["start"].as_u64().expect("start");
                    let byte = usize::try_from(start).expect("byte offset");
                    assert_eq!(&files[id].1[byte..byte + "\"保持\"".len()], "\"保持\"");
                    assert!(!files[id].1.is_char_boundary(byte + 2));
                    // One ASCII quote then the three-byte 保: offset+2 is inside 保.
                    expression["span"]["start"] = json!(start + 2);
                    changed = true;
                    break;
                }
                if changed {
                    break;
                }
            }
            if changed {
                break;
            }
        }
        assert!(changed, "real Unicode literal selected");
        let hostile: RawProjectSyntaxSnapshot = serde_json::from_value(raw).expect("raw spans");
        let errors = verify_snapshot(hostile, &sources).expect_err("no interior-byte authority");
        assert!(errors.iter().all(|error| error.code == "ZRYNA-Y5001"));
        verify_snapshot(dto, &sources).expect("pristine Unicode recovery");
    });
}

// Hand-authored complete tiny DTO. Byte offsets come only from the literal source,
// never the provider, shared Reader, source producer or emitted target.
fn tiny_return(prefix: &str, trivia: &str) -> (RawProjectSyntaxSnapshot, SourceMap) {
    let source = format!("{prefix}function root():i32 {{ return{trivia}7; }}");
    let function = source.find("function").expect("function");
    let name = function + "function ".len();
    let result = source.find("i32").expect("result annotation");
    let open = source.find('{').expect("open brace");
    let close = source.rfind('}').expect("close brace");
    let ret = source.find("return").expect("return keyword");
    let literal = source.rfind('7').expect("literal");
    let semi = source.rfind(';').expect("semicolon");
    let span = |start, end| json!({"file": 0, "start": start, "end": end});
    let identifier =
        |text: &str, start: usize| json!({"text": text, "span": span(start, start + text.len())});
    let raw = json!({"schema_version": 5, "diagnostics": [], "files": [{
        "id": 0, "path": "tiny.zry", "imports": [], "data_declarations": [],
        "type_syntax": [{"span": span(result, result + 3), "kind": {
            "kind": "named", "name": identifier("i32", result)
        }}],
        "functions": [{"span": span(function, source.len()), "export_span": null,
            "function_span": span(function, function + 8), "name": identifier("root", name),
            "type_parameters": null, "parameters": [], "result_type": 0,
            "body": {"span": span(open, close + 1), "root_block": 0,
                "blocks": [{"span": span(open, close + 1),
                    "open_brace_span": span(open, open + 1), "statements": [0],
                    "close_brace_span": span(close, close + 1)}],
                "statements": [{"span": span(ret, semi + 1), "kind": {
                    "kind": "return", "keyword_span": span(ret, ret + 6),
                    "value": 0, "semicolon_span": span(semi, semi + 1)}}],
                "expressions": [{"span": span(literal, literal + 1), "kind": {
                    "kind": "i32-literal", "spelling": "7"}}]
            }}]
    }]});
    assert_eq!(&source[ret..ret + 6], "return");
    assert_eq!(&source[literal..=literal], "7");
    assert!(source.is_char_boundary(ret) && source.is_char_boundary(literal));
    let dto = serde_json::from_value(raw).expect("independent complete raw claims");
    let sources = SourceMap::build(vec![SourceFileInput { path: "tiny.zry".into(), text: source }])
        .expect("authoritative exact UTF8");
    (dto, sources)
}

#[test]
fn independent_all_four_terminators_end_comments_and_forbid_return_value_asi() {
    let pristine = || {
        let (dto, sources) = tiny_return("", "/* same line */");
        verify_snapshot(dto, &sources).expect("same-line explicit consuming return");
    };
    pristine();
    for terminator in ["\n", "\r", "\u{2028}", "\u{2029}"] {
        let (dto, sources) = tiny_return(&format!("// prefix{terminator}"), " ");
        verify_snapshot(dto, &sources).expect("function after real line-comment terminator");
        for trivia in [terminator.to_owned(), format!("/*{terminator}*/")] {
            let (dto, sources) = tiny_return("", &trivia);
            let errors = verify_snapshot(dto, &sources)
                .expect_err("expression-bearing return cannot cross ASI terminator");
            assert!(errors.iter().all(|error| error.code == "ZRYNA-Y5001"));
            pristine();
        }
    }
}
