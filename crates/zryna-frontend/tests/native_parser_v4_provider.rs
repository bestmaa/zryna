//! Live differential evidence for the native protocol-v4 candidate.

use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    thread,
};

use serde_json::{Value, json};
use zryna_frontend::{native_lexer::lex, native_parser::v4::parse_v4_candidate};
use zryna_source::{SourceFileInput, SourceMap};

#[test]
#[ignore = "requires the exact pinned TypeScript provider"]
fn pinned_provider_and_native_v4_parser_match_m3_corpus_and_rejections() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/m3-fixtures");
    let mut paths = fs::read_dir(root)
        .expect("M3 fixtures")
        .map(|entry| entry.expect("M3 fixture").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "zry"))
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths.len(), 68, "frozen M3 source fixture count");

    let mut inputs = paths
        .iter()
        .map(|path| {
            let name = path.file_name().expect("fixture name").to_string_lossy();
            (format!("src/{name}"), fs::read_to_string(path).expect("UTF-8 fixture"))
        })
        .collect::<Vec<_>>();
    let positive_count = inputs.len();
    inputs.extend(
        [
            "export class Unsupported {}",
            "function f(): i32 { return (1); }",
            "function f(): i32 { return --value; }",
            "function f(x: any): i32 { return 1; }",
            "function f(): i32 { const value = 1; return value; }",
            "function f(): i32 { return 1; } import { f } from \"./f.zry\";",
            "function f(x: FixedArray<i32, 1048577>): i32 { return 1; }",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, text)| (format!("src/rejected-{index}.zry"), text.to_owned())),
    );

    let responses = provider_responses(&inputs);
    assert_eq!(responses.len(), inputs.len(), "one provider response per source");
    for (index, ((path, text), response)) in inputs.iter().zip(&responses).enumerate() {
        assert_eq!(response["id"], json!(index + 1), "{path}: response identity");
        let sources =
            SourceMap::build(vec![SourceFileInput { path: path.clone(), text: text.clone() }])
                .expect("source map");
        let lexed = lex(&sources).expect("bounded native tokens");
        if index < positive_count {
            assert!(response.get("error").is_none(), "{path}: provider error: {response}");
            let native = parse_v4_candidate(&sources, &lexed).expect("native v4 candidate");
            assert_eq!(
                serde_json::to_value(native).expect("native JSON"),
                response["result"],
                "{path}: exact provider candidate"
            );
        } else {
            let native = parse_v4_candidate(&sources, &lexed).expect_err("rejected source");
            let provider = response["error"].as_object().expect("provider rejection");
            assert!(response.get("result").is_none(), "{path}: no provider candidate");
            assert_eq!(native.diagnostic().code(), provider["code"], "{path}: rejection class");
            if index < positive_count + 6 {
                let (start, end) =
                    provider_source_range(provider["message"].as_str().expect("provider message"))
                        .expect("provider source location");
                let span = native.diagnostic().primary_span().expect("native source location");
                assert_eq!(span.file().index(), 0, "{path}: source file");
                assert!(
                    span.start() >= start && span.end() <= end,
                    "{path}: native location within provider rejected construct"
                );
            }
        }
    }
}

fn provider_responses(inputs: &[(String, String)]) -> Vec<Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = Command::new("node")
        .arg("src/worker-v4.mjs")
        .current_dir(root.join("adapters/typescript-6"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pinned v4 worker");
    let requests = inputs
        .iter()
        .enumerate()
        .map(|(index, (path, text))| {
            json!({
                "id": index + 1,
                "method": "analyze",
                "params": { "schema_version": 4, "files": [{ "path": path, "text": text }] },
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
    Some((start.parse().ok()?, end.parse().ok()?))
}
