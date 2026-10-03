//! Exact diagnostic and recovery parity at the untrusted candidate boundary.

use serde_json::{Value, json};
use zryna_frontend::{
    native_lexer::lex,
    native_parser::{
        parse_v2_recovering_candidate, v3::parse_v3_straight_line_candidate, v4::parse_v4_candidate,
    },
    syntax_v2, syntax_v3, syntax_v4,
};
use zryna_source::{SourceFileInput, SourceMap};

#[path = "native_parser_parity/projects.rs"]
mod projects;
#[path = "native_parser_parity/provider.rs"]
mod provider;
#[path = "native_parser_parity/resources.rs"]
mod resources;

fn frozen_cases() -> Vec<Value> {
    let cases = [
        include_str!("native_parser_parity/corpus.json"),
        include_str!("native_parser_parity/depth-corpus.json"),
        include_str!("native_parser_parity/project-corpus.json"),
        include_str!("native_parser_parity/call-depth-corpus.json"),
        include_str!("native_parser_parity/priority-corpus.json"),
        include_str!("native_parser_parity/priority-project-corpus.json"),
        include_str!("native_parser_parity/count-order-corpus.json"),
        include_str!("native_parser_parity/review-corpus.json"),
    ]
    .into_iter()
    .flat_map(|text| serde_json::from_str::<Vec<Value>>(text).expect("frozen corpus"))
    .collect::<Vec<_>>();
    let mut names = std::collections::BTreeSet::new();
    for case in &cases {
        assert!(
            names.insert(case["name"].as_str().expect("stable case identity")),
            "duplicate corpus identity"
        );
    }
    assert_eq!(cases.len(), 428, "complete combined frozen inventory");
    cases
}

#[test]
fn frozen_project_resources_reject_first_extra_atomically() {
    let receipts: Vec<Value> =
        serde_json::from_str(include_str!("native_parser_parity/project-rejections.json"))
            .expect("frozen project receipts");
    assert_eq!(receipts.len(), 12, "complete project first-extra inventory");
    for receipt in &receipts {
        let version =
            u32::try_from(receipt["version"].as_u64().expect("version")).expect("version range");
        let (_, files) = projects::cases(version)
            .into_iter()
            .find(|(name, _)| receipt["name"] == *name)
            .expect("project boundary source");
        let sources = SourceMap::build(
            files.into_iter().map(|(path, text)| SourceFileInput { path, text }).collect(),
        )
        .expect("bounded project authority");
        let lexed = lex(&sources).expect("reachable project lexical budget");
        assert_eq!(
            candidate(version, &sources, &lexed),
            receipt["response"],
            "v{version}-{}: project first extra",
            receipt["name"]
        );
    }
}

#[test]
#[ignore = "requires pinned worker and complete multi-file production inventories"]
fn pinned_provider_matches_project_resource_boundaries() {
    let mut failures = Vec::new();
    let mut count = 0;
    for version in 2..=4 {
        for (name, files) in projects::cases(version) {
            count += 1;
            eprintln!("checking v{version}-{name}");
            let worker = provider::project_response(version, &files);
            if worker.get("error").is_some() {
                eprintln!("receipt v{version}-{name}: {worker}");
            }
            let sources = SourceMap::build(
                files.into_iter().map(|(path, text)| SourceFileInput { path, text }).collect(),
            )
            .expect("bounded project");
            let lexed = lex(&sources).expect("reachable project lexical budget");
            let native = candidate(version, &sources, &lexed);
            if native != worker {
                failures.push(format!(
                    "v{version}-{name}: native {} worker {}",
                    native.get("error").unwrap_or(&Value::Null),
                    worker.get("error").unwrap_or(&Value::Null)
                ));
            }
        }
    }
    assert_eq!(count, 24, "complete reachable project boundary inventory");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn frozen_production_resources_reject_first_extra_atomically() {
    let receipts: Value =
        serde_json::from_str(include_str!("native_parser_parity/resource-rejections.json"))
            .expect("frozen production resource receipts");
    let receipts = receipts.as_array().expect("resource receipts");
    assert_eq!(receipts.len(), 29, "complete first-extra inventory");
    for receipt in receipts {
        let version =
            u32::try_from(receipt["version"].as_u64().expect("version")).expect("bounded version");
        let (_, text) = resources::cases(version)
            .into_iter()
            .find(|(name, _)| receipt["name"] == *name)
            .expect("resource source");
        let sources =
            SourceMap::build(vec![SourceFileInput { path: "src/main.zry".to_owned(), text }])
                .expect("bounded source");
        let lexed = lex(&sources).expect("production lexical budget");
        assert_eq!(
            candidate(version, &sources, &lexed),
            receipt["response"],
            "v{version}-{}: exact fatal rejection",
            receipt["name"]
        );
    }
}

#[test]
fn frozen_locals_preserve_statement_limit_priority() {
    let receipts: Vec<Value> =
        serde_json::from_str(include_str!("native_parser_parity/resource-rejections.json"))
            .expect("pinned resource receipts");
    let mut count = 0;
    for receipt in receipts.iter().filter(|receipt| receipt["name"] == "locals-4097") {
        count += 1;
        let version =
            u32::try_from(receipt["version"].as_u64().expect("version")).expect("version range");
        let (_, text) = resources::cases(version)
            .into_iter()
            .find(|(name, _)| name == "locals-4097")
            .expect("local boundary source");
        let sources =
            SourceMap::build(vec![SourceFileInput { path: "src/main.zry".to_owned(), text }])
                .expect("source authority");
        let lexed = lex(&sources).expect("lexical budget");
        assert_eq!(
            candidate(version, &sources, &lexed),
            receipt["response"],
            "v{version}: statement slot precedes local allocation"
        );
    }
    assert_eq!(count, 2, "both structured protocols");
}

#[test]
#[ignore = "requires pinned worker and production-limit snapshots"]
fn pinned_provider_matches_production_resource_boundaries() {
    let mut failures = Vec::new();
    for version in 2..=4 {
        for (name, text) in resources::cases(version) {
            eprintln!("checking v{version}-{name}");
            let worker = provider::responses(version, std::slice::from_ref(&text)).remove(0);
            let sources =
                SourceMap::build(vec![SourceFileInput { path: "src/main.zry".to_owned(), text }])
                    .expect("bounded source");
            let lexed = lex(&sources).expect("production lexical budget");
            let native = candidate(version, &sources, &lexed);
            if native != worker {
                let describe = |response: &Value| {
                    response.get("error").cloned().unwrap_or_else(|| json!({"candidate":true}))
                };
                failures.push(format!(
                    "v{version}-{name}: native {} worker {}",
                    describe(&native),
                    describe(&worker)
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "requires the exact pinned TypeScript provider"]
fn pinned_provider_matches_frozen_diagnostic_corpus() {
    let cases = frozen_cases();
    assert_eq!(cases.len(), 428, "complete frozen diagnostic inventory");
    for version in 2..=4 {
        let selected = cases
            .iter()
            .filter(|case| case["version"] == version && case.get("text").is_some())
            .collect::<Vec<_>>();
        let texts = selected
            .iter()
            .map(|case| case["text"].as_str().expect("source").to_owned())
            .collect::<Vec<_>>();
        let responses = provider::responses(version, &texts);
        for (case, response) in selected.into_iter().zip(responses) {
            assert_eq!(response, case["response"], "{}: exact pinned receipt", case["name"]);
        }
    }
    for case in cases.iter().filter(|case| case.get("files").is_some()) {
        let version =
            u32::try_from(case["version"].as_u64().expect("version")).expect("version range");
        let files =
            case_sources(case).into_iter().map(|file| (file.path, file.text)).collect::<Vec<_>>();
        assert_eq!(
            provider::project_response(version, &files),
            case["response"],
            "{}: pinned multi-file receipt",
            case["name"]
        );
    }
    frozen_recovery_and_rejections_match_exact_provider_diagnostics();
}

#[test]
fn frozen_recovery_and_rejections_match_exact_provider_diagnostics() {
    let cases = frozen_cases();
    let mut failures = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().expect("case identity");
        let sources = SourceMap::build(case_sources(case)).expect("source authority");
        let lexed = lex(&sources).expect("bounded tokens");
        let version =
            u32::try_from(case["version"].as_u64().expect("protocol")).expect("version range");
        let native = candidate(version, &sources, &lexed);
        if native != case["response"] {
            failures.push(format!("{name}:\nnative {native}\nworker {}", case["response"]));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn case_sources(case: &Value) -> Vec<SourceFileInput> {
    if let Some(files) = case.get("files") {
        files
            .as_array()
            .expect("case file inventory")
            .iter()
            .map(|file| SourceFileInput {
                path: file["path"].as_str().expect("file path").to_owned(),
                text: file["text"].as_str().expect("file source").to_owned(),
            })
            .collect()
    } else {
        vec![SourceFileInput {
            path: "src/main.zry".to_owned(),
            text: case["text"].as_str().expect("case source").to_owned(),
        }]
    }
}

fn rejection(diagnostic: &zryna_diagnostics::Diagnostic) -> Value {
    let span = diagnostic.primary_span();
    json!({"error": {
        "code": diagnostic.code(),
        "message": diagnostic.message(),
        "span": span.map(|span| json!({
            "file": span.file().index(), "start": span.start(), "end": span.end(),
        })),
    }})
}

fn candidate(
    version: u32,
    sources: &SourceMap,
    lexed: &zryna_frontend::native_lexer::LexedProject,
) -> Value {
    match version {
        2 => match parse_v2_recovering_candidate(sources, lexed) {
            Ok(raw) => {
                let value = serde_json::to_value(&raw).expect("v2 JSON");
                syntax_v2::verify_snapshot(raw, sources).expect("v2 verifier");
                json!({"result": value})
            }
            Err(error) => rejection(error.diagnostic()),
        },
        3 => match parse_v3_straight_line_candidate(sources, lexed) {
            Ok(raw) => {
                let value = serde_json::to_value(&raw).expect("v3 JSON");
                syntax_v3::verify_snapshot(raw, sources).expect("v3 verifier");
                json!({"result": value})
            }
            Err(error) => rejection(error.diagnostic()),
        },
        4 => match parse_v4_candidate(sources, lexed) {
            Ok(raw) => {
                let value = serde_json::to_value(&raw).expect("v4 JSON");
                syntax_v4::verify_snapshot(raw, sources).expect("v4 verifier");
                json!({"result": value})
            }
            Err(error) => rejection(error.diagnostic()),
        },
        _ => unreachable!(),
    }
}

#[test]
fn arbitrary_bounded_token_grammar_is_deterministic_and_never_forges_a_snapshot() {
    let vocabulary = [
        "export",
        "function",
        "(",
        ")",
        "{",
        "}",
        "interface",
        "extends",
        "ZrynaStruct",
        "return",
        "const",
        "let",
        "if",
        "else",
        "while",
        "i32",
        "x",
        "1",
        "true",
        "+",
        "-",
        "*",
        "=",
        "<",
        ">",
        ":",
        ";",
        "[",
        "]",
        ",",
        "\"x\"",
    ];
    let mut seed = 0x412_2026_u32;
    for index in 0..2048 {
        let mut text = "// π\r\n".to_owned();
        for _ in 0..4 + index % 47 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            text.push_str(vocabulary[seed as usize % vocabulary.len()]);
            text.push(' ');
        }
        let sources =
            SourceMap::build(vec![SourceFileInput { path: "src/main.zry".to_owned(), text }])
                .expect("bounded mutated source");
        let lexed = lex(&sources).expect("bounded token grammar");
        for version in 2..=4 {
            let mut results = Vec::new();
            for _ in 0..2 {
                let result = match version {
                    2 => match parse_v2_recovering_candidate(&sources, &lexed) {
                        Ok(raw) => {
                            let value = serde_json::to_value(&raw).expect("v2 JSON");
                            syntax_v2::verify_snapshot(raw, &sources)
                                .expect("generated v2 graph verifies");
                            json!({"result": value})
                        }
                        Err(error) => rejection(error.diagnostic()),
                    },
                    3 => match parse_v3_straight_line_candidate(&sources, &lexed) {
                        Ok(raw) => {
                            let value = serde_json::to_value(&raw).expect("v3 JSON");
                            syntax_v3::verify_snapshot(raw, &sources)
                                .expect("generated v3 graph verifies");
                            json!({"result": value})
                        }
                        Err(error) => rejection(error.diagnostic()),
                    },
                    4 => match parse_v4_candidate(&sources, &lexed) {
                        Ok(raw) => {
                            let value = serde_json::to_value(&raw).expect("v4 JSON");
                            syntax_v4::verify_snapshot(raw, &sources)
                                .expect("generated v4 graph verifies");
                            json!({"result": value})
                        }
                        Err(error) => rejection(error.diagnostic()),
                    },
                    _ => unreachable!(),
                };
                results.push(result);
            }
            assert_eq!(results[0], results[1], "v{version} deterministic grammar case {index}");
        }
    }
}
