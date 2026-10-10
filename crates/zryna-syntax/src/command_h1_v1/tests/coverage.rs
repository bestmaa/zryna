use super::{admit, environment, json, map, verified};

#[test]
fn inherited_v4_source_fixtures_retain_complete_leaf_coverage() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/m3-fixtures");
    for name in [
        "syntax-v4-shorthand",
        "exclusive-root-borrow",
        "owned-root-borrow-reads",
        "shared-root-borrow",
        "shared-root-reborrow",
        "loop-root-borrow",
        "pair-score-v4",
        "nonindexed-borrow-nested-call",
        "fixed-array-subobject-return",
    ] {
        let text =
            std::fs::read_to_string(root.join(format!("{name}.zry"))).expect("source fixture");
        let raw: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join(format!("{name}.json"))).expect("raw fixture"),
        )
        .expect("JSON fixture");
        let sources = map(&text);
        let raw = raw.get("result").unwrap_or(&raw);
        let decoded = super::decode_snapshot(&serde_json::to_vec(raw).expect("fixture bytes"))
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let syntax = super::verify_snapshot(decoded, &sources)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        admit(&syntax, &sources).unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

#[test]
fn complete_owned_and_h1_source_contexts_retain_leaf_coverage() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/wasi-command-source-fixtures");
    for name in ["environment-match", "owned-aggregates", "control-flow", "weak-upgrade"] {
        let text =
            std::fs::read_to_string(root.join(format!("{name}.zry"))).expect("source fixture");
        let raw = serde_json::from_slice(
            &std::fs::read(root.join(format!("{name}.json"))).expect("raw fixture"),
        )
        .expect("JSON fixture");
        let sources = map(&text);
        let syntax = verified(&raw, &sources);
        admit(&syntax, &sources).unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}

#[test]
fn omitted_file_edges_and_unrepresented_statements_reject_independent_v4_claims() {
    let (text, raw) = environment("MODE");
    for suffix in [
        "\nfunction unused(): bool { return true; }",
        "\nimport { foo } from './other.zry';",
        "\nconst other: bool = true;",
        "\n;",
        "\n{}",
    ] {
        let sources = map(&format!("{text}{suffix}"));
        let syntax = verified(&raw, &sources);
        assert!(admit(&syntax, &sources).is_err(), "unrepresented source suffix {suffix:?}");
    }
    let mut empty = raw;
    empty["files"][0]["functions"] = json!([]);
    empty["files"][0]["type_syntax"] = json!([]);
    let sources = map("export function main(): bool { return true; }");
    let syntax = verified(&empty, &sources);
    assert!(admit(&syntax, &sources).is_err(), "empty provider cannot hide a pure entry");
}

#[test]
fn wide_parent_spans_cannot_cover_omitted_statement_or_call_argument_tokens() {
    let mut text =
        include_str!("../../../../../tests/wasi-command-source-fixtures/pure-entry.zry").to_owned();
    let mut raw: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../../tests/wasi-command-source-fixtures/pure-entry.json"
    ))
    .expect("independent provider fixture");
    let start = text.find("return").expect("return");
    let omitted = "const hidden: bool = false; ";
    text.insert_str(start, omitted);
    super::shift_spans(&mut raw, start, omitted.len());
    raw["files"][0]["functions"][0]["body"]["statements"][0]["span"]["start"] = json!(start);
    let sources = map(&text);
    let syntax = verified(&raw, &sources);
    assert!(
        admit(&syntax, &sources).is_err(),
        "parent Return span cannot authenticate hidden local tokens"
    );

    let mut text =
        include_str!("../../../../../tests/wasi-command-source-fixtures/control-flow.zry")
            .to_owned();
    let mut raw: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../../tests/wasi-command-source-fixtures/control-flow.json"
    ))
    .expect("independent provider fixture");
    let start = text.find("keep(true,)").expect("call") + "keep(true,".len();
    let omitted = " hidden(),";
    text.insert_str(start, omitted);
    super::shift_spans(&mut raw, start, omitted.len());
    let sources = map(&text);
    let syntax = verified(&raw, &sources);
    assert!(
        admit(&syntax, &sources).is_err(),
        "parent Call span cannot authenticate hidden argument tokens"
    );
}
