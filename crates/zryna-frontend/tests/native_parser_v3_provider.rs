//! Live protocol-v3 comparison over the public M2 source corpus.

use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    thread,
};

use serde_json::{Value, json};
use zryna_frontend::{
    native_lexer::lex, native_parser::v3::parse_v3_straight_line_candidate, syntax_v3,
};
use zryna_source::{SourceFileInput, SourceMap};

#[test]
fn pinned_provider_and_native_v3_parser_match_m2_corpus_and_rejections() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/m2-fixtures");
    let mut directories = vec![root.clone()];
    let mut inputs = Vec::new();
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).expect("M2 fixture directory") {
            let path = entry.expect("M2 fixture entry").path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|extension| extension == "zry") {
                let relative = path.strip_prefix(&root).expect("M2 source relative path");
                let portable = relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                inputs.push((
                    format!("src/{portable}"),
                    fs::read_to_string(path).expect("UTF-8 source"),
                ));
            }
        }
    }
    inputs.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(inputs.len(), 14, "complete M2 source fixture count");
    let corpus_count = inputs.len();
    inputs.extend(
        [
            "function f(): i32 { return --value; }",
            "function f(): i32 { return (1); }",
            "function f(): i32 { return foo.bar(); }",
            "function f(): i32 { return 1; } import { x } from \"./x.zry\";",
            "function f(): i32 { let x: i32 = 1; return x--; }",
            "function f(): i32 { let x: i32 = 1; return x++; }",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, text)| (format!("src/rejected-{index}.zry"), text.to_owned())),
    );

    let responses = provider_responses(&inputs);
    assert_eq!(responses.len(), inputs.len(), "one provider response per source");
    let mut accepted = 0;
    let mut rejected = 0;
    for (index, ((path, text), response)) in inputs.iter().zip(&responses).enumerate() {
        assert_eq!(response["id"], json!(index + 1), "{path}: response identity");
        let sources =
            SourceMap::build(vec![SourceFileInput { path: path.clone(), text: text.clone() }])
                .expect("source map");
        let lexed = lex(&sources).expect("bounded native tokens");
        if response.get("result").is_some() {
            assert!(index < corpus_count, "constructed rejection became accepted");
            let native =
                parse_v3_straight_line_candidate(&sources, &lexed).expect("native v3 candidate");
            assert_eq!(
                serde_json::to_value(&native).expect("native JSON"),
                response["result"],
                "{path}: exact provider candidate"
            );
            syntax_v3::verify_snapshot(native, &sources).expect("v3 verifier");
            accepted += 1;
        } else {
            let native = parse_v3_straight_line_candidate(&sources, &lexed)
                .expect_err("provider-rejected syntax");
            let error = response["error"].as_object().expect("provider rejection");
            assert_eq!(native.diagnostic().code(), error["code"], "{path}: rejection class");
            let (start, end) =
                provider_source_range(error["message"].as_str().expect("provider message"))
                    .expect("provider source location");
            let span = native.diagnostic().primary_span().expect("native source location");
            assert_eq!(span.file().index(), 0, "{path}: source file");
            assert!(
                span.start() >= start && span.end() <= end,
                "{path}: native location within provider rejected construct"
            );
            rejected += 1;
        }
    }
    assert_eq!((accepted, rejected), (13, 7), "M2 accepted and rejected source counts");
}

#[test]
fn deterministic_v3_grammar_mutations_verify_or_reject_atomically() {
    let atoms = ["x", "1", "true", "-x", "-1", "helper(x)"];
    let operators = ["+", "-", "*", "<", "<=", "===", "!=="];
    let mut seed = 0x412_u32;
    let mut accepted = 0;
    let mut rejected = 0;
    for case in 0..128 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let left = atoms[(seed as usize) % atoms.len()];
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let right = atoms[(seed as usize) % atoms.len()];
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let operator = operators[(seed as usize) % operators.len()];
        let suffix = if case % 5 == 0 { " return (x);" } else { "" };
        let trailing =
            if case % 5 != 0 && case % 7 == 0 { " export class Unsupported {}" } else { "" };
        let text = format!(
            "// π case {case}\nfunction helper(x: i32): i32 {{ return x; }}\n\
             export function f(x: i32): i32 {{ let y: i32 = {left} {operator} {right}; \
             if (true) {{ return y; }} return 0;{suffix} }}{trailing}"
        );
        let sources =
            SourceMap::build(vec![SourceFileInput { path: "src/mutated.zry".to_owned(), text }])
                .expect("source map");
        let lexed = lex(&sources).expect("native tokens");
        match parse_v3_straight_line_candidate(&sources, &lexed) {
            Ok(native) => {
                assert!(case % 5 != 0 && case % 7 != 0, "unsupported mutation was admitted");
                syntax_v3::verify_snapshot(native, &sources)
                    .expect("source-bound generated candidate");
                accepted += 1;
            }
            Err(error) => {
                assert!(case % 5 == 0 || case % 7 == 0, "valid mutation was rejected");
                assert_eq!(error.diagnostic().code(), "ZRYNA-F2002");
                rejected += 1;
            }
        }
    }
    assert_eq!(accepted + rejected, 128);
    assert!(accepted > 75 && rejected > 25, "both grammar outcomes exercised");
}

#[test]
fn frozen_v3_rejections_have_source_bound_locations() {
    let names = [
        "native_parser_v3_assignments/rejected",
        "native_parser_v3_blocks/rejected",
        "native_parser_v3_calls/rejected",
        "native_parser_v3_expressions/rejected",
        "native_parser_v3_expressions/rejected-prefix",
        "native_parser_v3_functions/rejected",
        "native_parser_v3_locals/rejected",
        "native_parser_v3_negation/rejected",
        "native_parser_v3_numeric_negation/rejected",
    ];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for name in names {
        let text = fs::read_to_string(root.join(format!("{name}.zry"))).expect("frozen source");
        let response: Value = serde_json::from_slice(
            &fs::read(root.join(format!("{name}.response.json"))).expect("frozen response"),
        )
        .expect("closed worker response");
        let sources =
            SourceMap::build(vec![SourceFileInput { path: "src/main.zry".to_owned(), text }])
                .expect("source map");
        let lexed = lex(&sources).expect("native tokens");
        let native =
            parse_v3_straight_line_candidate(&sources, &lexed).expect_err("rejected source");
        assert_eq!(
            native.diagnostic().code(),
            response["error"]["code"],
            "{name}: rejection class"
        );
        let message = response["error"]["message"].as_str().expect("worker message");
        let (start, end) = provider_source_range(message).expect("worker source range");
        let span = native.diagnostic().primary_span().expect("native source location");
        assert_eq!(span.file().index(), 0, "{name}: source file");
        assert!(
            span.start() >= start && span.end() <= end,
            "{name}: native location within worker rejected construct"
        );
    }
}

fn provider_responses(inputs: &[(String, String)]) -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = Command::new("node")
        .arg("src/worker-v3.mjs")
        .current_dir(root.join("adapters/typescript-6"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pinned v3 worker");
    let requests = inputs
        .iter()
        .enumerate()
        .map(|(index, (path, text))| {
            json!({
                "id": index + 1,
                "method": "analyze",
                "params": { "schema_version": 3, "files": [{ "path": path, "text": text }] },
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut stdin = child.stdin.take().expect("worker stdin");
    let writer = thread::spawn(move || stdin.write_all(requests.as_bytes()));
    let output = child.wait_with_output().expect("worker output");
    writer.join().expect("worker writer").expect("write worker requests");
    assert!(output.status.success(), "worker failed: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "worker stderr: {}", String::from_utf8_lossy(&output.stderr));
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).expect("closed worker response"))
        .collect()
}

fn provider_source_range(message: &str) -> Option<(u32, u32)> {
    let range = message.rsplit_once(" at file 0 bytes ")?.1;
    let (start, end) = range.split_once("..")?;
    let end = end.chars().take_while(char::is_ascii_digit).collect::<String>();
    Some((start.parse().ok()?, end.parse().ok()?))
}
