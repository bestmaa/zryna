//! Real source-to-executable successor Copy conformance.

use std::process::Command;
use zryna_ir::generic_v1::{copy_v1, wire};
use zryna_layout::StorageTarget;
use zryna_ownership_runtime_abi::generic_v1 as runtime;
use zryna_semantics::bounded_generics_v1::{
    SemanticInput,
    body_types::check_body_types,
    instantiation::{copy_v1::produce_claim, discover, layouts::verify_layouts},
    resolve_declarations,
};
use zryna_source::{SourceFileInput, SourceMap};
use zryna_syntax::v5::{decode_snapshot, verify_snapshot};

#[test]
fn artifact_exact_byte_ceiling_then_first_extra_rejects_atomically() {
    let limit = crate::prelude::MAX_CONTROL_FLOW_JAVASCRIPT_BYTES;
    let mut writer = super::Writer { source: String::new() };
    let chunk = "x".repeat(1024 * 1024);
    for _ in 0..(limit / chunk.len()) {
        writer.text(&chunk).expect("exact artifact byte ceiling");
    }
    assert_eq!(writer.source.len(), limit);
    let error = writer.text("x").expect_err("first extra artifact byte");
    assert_eq!(error.code, "ZRYNA-J2003");
    assert_eq!(writer.source.len(), limit);
    writer.text("").expect("failed append retains exact bounded artifact");
}

#[test]
fn genuine_source_generic_forwarding_option_result_and_scalar_boundaries_execute() {
    let artifact = compile(
        &[("main.zry", include_str!("../../../../tests/m7-generic-copy-fixtures/main.zry"))],
        include_bytes!("../../../../tests/m7-generic-copy-fixtures/reference.json"),
    );
    execute(&artifact);
}

#[test]
fn exact_imported_original_instances_execute_across_modules() {
    let artifact = compile(
        &[
            ("main.zry", include_str!("../../../../tests/m7-generic-copy-fixtures/cross-main.zry")),
            (
                "values.zry",
                include_str!("../../../../tests/m7-generic-copy-fixtures/cross-values.zry"),
            ),
        ],
        include_bytes!("../../../../tests/m7-generic-copy-fixtures/cross-reference.json"),
    );
    execute(&artifact);
}

fn compile(files: &[(&str, &str)], snapshot: &[u8]) -> crate::JavaScriptArtifact {
    let sources = SourceMap::build(
        files
            .iter()
            .map(|(path, text)| SourceFileInput { path: (*path).into(), text: (*text).into() })
            .collect(),
    )
    .expect("genuine fixture sources");
    let syntax =
        verify_snapshot(decode_snapshot(snapshot).expect("genuine fixture invariant"), &sources)
            .expect("genuine fixture invariant");
    let entry = sources.verify_file_id(0).expect("genuine fixture invariant");
    let declarations = resolve_declarations(
        SemanticInput::try_new(&syntax, &sources, entry).expect("genuine fixture invariant"),
    )
    .expect("genuine fixture invariant");
    let bodies = check_body_types(&declarations).expect("genuine fixture invariant");
    let instances = discover(&bodies).expect("genuine fixture invariant");
    assert_eq!(instances.function_keys().len(), 5); // identity<bool/i32/Option<i32>>, forward<i32>, wrap<i32>
    let linear =
        verify_layouts(&instances, StorageTarget::Linear32V1).expect("genuine fixture invariant");
    let linux =
        verify_layouts(&instances, StorageTarget::LinuxX8664V1).expect("genuine fixture invariant");
    let abi = runtime::verify_v1(
        runtime::raw_v1(&linear, &linux).expect("genuine fixture invariant"),
        &linear,
        &linux,
    )
    .expect("genuine fixture invariant");
    let raw = produce_claim(&instances, &linear, &linux).expect("genuine fixture invariant");
    let bytes = wire::encode(&raw).expect("genuine fixture invariant");
    let program = copy_v1::verify(
        wire::decode(&bytes).expect("genuine fixture invariant"),
        &syntax,
        &sources,
        entry,
        &linear,
        &linux,
        &abi,
    )
    .expect("genuine fixture invariant");
    let artifact = super::emit(&program).expect("genuine fixture invariant");
    assert_eq!(artifact, super::emit(&program).expect("genuine fixture invariant"));
    if let Some(directory) = std::env::var_os("ZRYNA_GENERIC_COPY_EVIDENCE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory)
            .expect("create explicitly selected proof output directory");
        let name = format!("{}-modules", files.len());
        std::fs::write(directory.join(format!("{name}.mjs")), &artifact.source)
            .expect("retain exact executable proof bytes");
        std::fs::write(directory.join(format!("{name}.zir")), &bytes)
            .expect("retain exact wire proof bytes");
    }
    artifact
}

fn execute(artifact: &crate::JavaScriptArtifact) {
    let version =
        Command::new("node").arg("--version").output().expect("genuine fixture invariant");
    assert_eq!(
        String::from_utf8(version.stdout).expect("genuine fixture invariant").trim(),
        "v22.22.1"
    );
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("genuine fixture invariant")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("zryna-416-generic-{}-{unique}.mjs", std::process::id()));
    std::fs::write(&path, &artifact.source).expect("genuine fixture invariant");
    let harness = r"
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
const module = await import(pathToFileURL(process.argv[1]).href);
assert.deepEqual(Object.keys(module).sort(), ['add', 'error', 'fallback', 'flag', 'score', 'unwrap']);
for (const value of [-2147483648, -1, 0, 7, 2147483647]) { assert.equal(module.score(value), value); assert.equal(module.unwrap(value), value); }
assert.equal(module.flag(true), true); assert.equal(module.flag(false), false);
assert.equal(module.add(2147483647), -2147483648); assert.equal(module.add(-1), 0);
assert.equal(module.fallback(), 11); assert.equal(module.error(), 23);
for (const value of [-0, 1.5, NaN, Infinity, 2147483648, -2147483649, true, '7', null, {}]) assert.throws(() => module.score(value));
for (const value of [0, 1, 'true', null, {}]) assert.throws(() => module.flag(value));
assert.throws(() => module.score()); assert.throws(() => module.score(7, 8));
assert.throws(() => module.fallback(0)); assert.equal(module.score(7), 7);
console.log('generic-copy-v1: source -> semantics -> layouts -> runtime ABI -> wire -> IR -> JavaScript: PASS');
";
    let output = Command::new("node")
        .args(["--input-type=module", "-e", harness])
        .arg(&path)
        .output()
        .expect("genuine fixture invariant");
    std::fs::remove_file(&path).expect("genuine fixture invariant");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
