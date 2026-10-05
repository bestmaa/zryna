//! Finite pinned live raw differential; this never refreshes the frozen corpus or selects a provider.

use serde_json::{Value, json};
use std::{
    fmt::Write as FmtWrite,
    io::Write,
    process::{Command, Stdio},
    thread,
};
use zryna_frontend::{native_lexer::lex, native_parser::v5::parse_v5_candidate};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v5;

#[path = "native_parser_v4_corpus/fixtures.rs"]
mod m3;
#[path = "native_parser_v5/support.rs"]
mod support;

fn worker(projects: &[Vec<SourceFileInput>]) -> Vec<Value> {
    let requests = projects
        .iter()
        .map(|files| {
            files
                .iter()
                .rev()
                .map(|file| json!({ "path": file.path, "text": file.text }))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut child = Command::new("node")
        .arg(support::root().join("crates/zryna-frontend/tests/native_parser_v5/worker.mjs"))
        .arg(support::root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("bounded pinned worker differential");
    let bytes = serde_json::to_vec(&requests).expect("request JSON");
    let mut stdin = child.stdin.take().expect("stdin");
    let writer = thread::spawn(move || stdin.write_all(&bytes));
    let output = child.wait_with_output().expect("worker output");
    writer.join().expect("writer").expect("requests");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).expect("bounded raw responses")
}

fn input(text: String) -> Vec<SourceFileInput> {
    vec![SourceFileInput { path: "main.zry".into(), text }]
}

fn compare(files: Vec<SourceFileInput>, response: &Value, verify: bool) {
    let sources = SourceMap::build(files).expect("source map");
    let raw = parse_v5_candidate(&sources, &lex(&sources).expect("lexical bound"));
    if response.get("result").is_some() {
        let native = match raw {
            Ok(native) => native,
            Err(error) => {
                // TypeScript may emit syntactically structured raw DTOs whose contextual roles
                // fail the mandatory source barrier. Native may reject them before producing a DTO.
                let worker_raw =
                    v5::decode_snapshot(&serde_json::to_vec(&response["result"]).expect("JSON"))
                        .expect("closed raw");
                let errors = v5::verify_snapshot(worker_raw, &sources)
                    .expect_err("both paths reject excluded source");
                assert_eq!(error.diagnostic().code(), "ZRYNA-F2002");
                assert_eq!(errors[0].code, "ZRYNA-Y5001");
                return;
            }
        };
        assert_eq!(serde_json::to_value(&native).expect("raw JSON"), response["result"]);
        let closed =
            v5::decode_snapshot(&serde_json::to_vec(&response["result"]).expect("raw JSON"))
                .expect("closed bounded v5 DTO");
        assert_eq!(native, closed);
        if verify {
            v5::verify_snapshot(native, &sources).expect("independent source verifier");
        }
    } else {
        let error = raw.expect_err("atomic native rejection");
        assert_eq!(
            error.diagnostic().code(),
            response["error"]["code"].as_str().expect("worker error")
        );
    }
}

#[test]
fn pinned_live_v5_matches_all_frozen_raw_dtos_in_fresh_processes() {
    let corpus = support::corpus();
    let cases = corpus["cases"].as_array().expect("cases");
    let projects = cases.iter().map(support::inputs).collect::<Vec<_>>();
    for _ in 0..2 {
        let replies = worker(&projects);
        assert_eq!(replies.len(), 12);
        for (index, case) in cases.iter().enumerate() {
            let expected: Value = serde_json::from_slice(
                &std::fs::read(
                    support::root().join(case["reference"].as_str().expect("reference")),
                )
                .expect("original DTO"),
            )
            .expect("JSON");
            assert_eq!(replies[index]["result"], expected, "{}", case["id"]);
            compare(projects[index].clone(), &replies[index], true);
        }
    }
}

#[test]
fn retained_ninety_five_m3_sources_match_live_raw_v5() {
    let fixtures = m3::sources();
    let projects = fixtures
        .iter()
        .map(|(path, text)| vec![SourceFileInput { path: path.clone(), text: text.clone() }])
        .collect::<Vec<_>>();
    let replies = worker(&projects);
    assert_eq!(replies.len(), 95);
    for (index, _) in fixtures.iter().enumerate() {
        compare(projects[index].clone(), &replies[index], true);
    }
}

#[test]
fn generic_punctuation_keywords_utf8_and_four_line_endings_match_live() {
    let mut projects = Vec::new();
    for newline in ["\n", "\r\n", "\u{2028}", "\u{2029}"] {
        for prefix in ["// π😀\n", "/* π😀 */ "] {
            let text = format!("{prefix}function identity<T extends ZrynaValue,>(x: T): T {{ return/*x*/x; }}\nfunction run(): i32 {{ const v: Option<FixedArray<Result<i32,String>,2>>=Option.none<FixedArray<Result<i32,String>,2>>(); return identity<i32,>(7); }}").replace('\n', newline);
            projects.push(input(text));
            projects.push(input(format!(
                "function f<T extends ZrynaValue>(x: T): T {{ return/*{newline}*/x; }}"
            )));
        }
    }
    for text in [
        "function from(value: type, eval: i32, arguments: i32, let: i32): i32 { return arguments; }",
        "export\nfunction score(await: i32): i32 { eval(await); arguments(await); return await; }",
        "export function f(): i32 { const eval: i32 = 1; return eval; }",
        "export function f(): i32 { eval = 1; return 1; }",
        "export function eval(): i32 { return 1; }",
        "function return(): i32 { return 1; }",
        "export interface Choice<await extends ZrynaValue> extends ZrynaEnum { none: ZrynaNone; }",
        "export function f<T extends ZrynaValue>(value: T): T { return value; } export class C {}",
        "function f(): i32 { \"use strict\"; return 1; }",
    ] {
        projects.push(input(text.into()));
    }
    let replies = worker(&projects);
    for (files, reply) in projects.into_iter().zip(replies) {
        compare(files, &reply, true);
    }
}

#[test]
fn production_parameter_member_array_function_and_declaration_boundaries_match_live() {
    let mut projects = Vec::new();
    for count in [256, 257] {
        let args = (0..count).map(|i| format!("p{i}: i32")).collect::<Vec<_>>().join(",");
        projects
            .push(input(format!("function f<T extends ZrynaValue>({args}): i32 {{ return 1; }}")));
    }
    for count in [1024, 1025] {
        let fields = (0..count).fold(String::new(), |mut text, i| {
            write!(text, "p{i}: T;").expect("String writer");
            text
        });
        projects.push(input(format!(
            "interface Box<T extends ZrynaValue> extends ZrynaStruct {{{fields}}}"
        )));
    }
    for count in [4096, 4097] {
        projects.push(input(format!(
            "function f(): Vec<i32> {{ return Vec<i32>([{}]); }}",
            vec!["1"; count].join(",")
        )));
        projects.push(input((0..count).fold(String::new(), |mut text, i| {
            writeln!(text, "function f{i}(): i32 {{ return 1; }}").expect("String writer");
            text
        })));
        projects.push(input((0..count).fold(String::new(), |mut text, i| {
            writeln!(text, "interface E{i} extends ZrynaEnum {{ none: ZrynaNone; }}")
                .expect("String writer");
            text
        })));
    }
    for count in [127, 128] {
        let annotation = format!("{}i32{}", "Option<".repeat(count), ">".repeat(count));
        projects.push(input(format!("function f(value: {annotation}): i32 {{ return 1; }}")));
    }
    let replies = worker(&projects);
    for (files, reply) in projects.into_iter().zip(replies) {
        compare(files, &reply, true);
    }
}
