//! Real source execution and independent final-byte corruption/resource evidence.

use super::{audit, bytes::Bytes, layout::Layout};
use std::process::Command;
use wasmparser::{Operator, Parser, Payload};
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

fn with_program(cross: bool, test: impl FnOnce(&copy_v1::VerifiedCopyProgram<'_>)) {
    let mut files = vec![SourceFileInput {
        path: "main.zry".into(),
        text: if cross {
            include_str!("../../../../tests/m7-generic-copy-fixtures/cross-main.zry")
        } else {
            include_str!("../../../../tests/m7-generic-copy-fixtures/main.zry")
        }
        .into(),
    }];
    if cross {
        files.push(SourceFileInput {
            path: "values.zry".into(),
            text: include_str!("../../../../tests/m7-generic-copy-fixtures/cross-values.zry")
                .into(),
        });
    }
    let snapshot: &[u8] = if cross {
        include_bytes!("../../../../tests/m7-generic-copy-fixtures/cross-reference.json")
    } else {
        include_bytes!("../../../../tests/m7-generic-copy-fixtures/reference.json")
    };
    let sources = SourceMap::build(files).expect("genuine source map");
    let syntax = verify_snapshot(decode_snapshot(snapshot).expect("frozen DTO"), &sources)
        .expect("exact full source authentication");
    let entry = sources.verify_file_id(0).expect("entry");
    let declarations = resolve_declarations(
        SemanticInput::try_new(&syntax, &sources, entry).expect("exact semantic input"),
    )
    .expect("declarations");
    let bodies = check_body_types(&declarations).expect("opaque symbolic original bodies");
    let instances = discover(&bodies).expect("bounded complete demand");
    assert_eq!(instances.function_keys().len(), 5);
    let linear = verify_layouts(&instances, StorageTarget::Linear32V1).expect("linear layouts");
    let linux = verify_layouts(&instances, StorageTarget::LinuxX8664V1).expect("linux layouts");
    let runtime = runtime::verify_v1(
        runtime::raw_v1(&linear, &linux).expect("raw contract"),
        &linear,
        &linux,
    )
    .expect("separate runtime issuer");
    let raw = produce_claim(&instances, &linear, &linux).expect("untrusted semantic claim");
    let encoded = wire::encode(&raw).expect("bounded wire");
    let program = copy_v1::verify(
        wire::decode(&encoded).expect("wire decode"),
        &syntax,
        &sources,
        entry,
        &linear,
        &linux,
        &runtime,
    )
    .expect("independent source/body/demand/Copy seal");
    assert_eq!(program.ownership_effects(), (0, 0));
    test(&program);
}

#[test]
fn genuine_generic_option_result_program_executes_core_wasm1() {
    with_program(false, execute);
}

#[test]
fn exact_cross_module_original_instances_execute_core_wasm1() {
    with_program(true, execute);
}

fn execute(program: &copy_v1::VerifiedCopyProgram<'_>) {
    let artifact = super::emit(program).expect("complete audited artifact");
    assert_eq!(artifact, super::emit(program).expect("deterministic replay"));
    let version = Command::new("node").arg("--version").output().expect("pinned Node");
    assert_eq!(String::from_utf8(version.stdout).expect("version").trim(), "v22.22.1");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir()
        .join(format!("zryna-416-generic-wasm-{}-{unique}.wasm", std::process::id()));
    std::fs::write(&path, artifact.bytes()).expect("test executable bytes");
    let harness = r"
import assert from 'node:assert/strict';
import fs from 'node:fs';
const bytes = fs.readFileSync(process.argv[1]);
assert.equal(WebAssembly.validate(bytes), true);
const core = new WebAssembly.Module(bytes);
assert.deepEqual(WebAssembly.Module.imports(core), []);
assert.deepEqual(WebAssembly.Module.exports(core).map(x => [x.name, x.kind]).sort(),
  ['add','error','fallback','flag','score','unwrap'].map(name => [name, 'function']));
const module = new WebAssembly.Instance(core).exports;
for (const value of [-2147483648,-1,0,7,2147483647]) {
  assert.equal(module.score(value), value); assert.equal(module.unwrap(value), value);
}
assert.equal(module.flag(1), 1); assert.equal(module.flag(0), 0);
assert.equal(module.add(2147483647), -2147483648); assert.equal(module.add(-1), 0);
assert.equal(module.fallback(), 11); assert.equal(module.error(), 23);
for (const value of [-2147483648,-1,2,2147483647]) {
  assert.throws(() => module.flag(value), WebAssembly.RuntimeError);
  assert.equal(module.flag(1), 1); assert.equal(module.unwrap(7), 7);
}
assert.equal(new WebAssembly.Instance(core).exports.unwrap(-1), -1);
console.log('generic-copy-v1: authenticated source -> sealed IR -> audited core Wasm1 -> pinned Node: PASS');
";
    let result = Command::new("node")
        .args(["--input-type=module", "-e", harness])
        .arg(&path)
        .output()
        .expect("execute pinned core engine");
    std::fs::remove_file(&path).expect("remove own temporary artifact");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    println!("{}", String::from_utf8_lossy(&result.stdout));
    if let Some(directory) = std::env::var_os("ZRYNA_GENERIC_COPY_EVIDENCE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).expect("explicit evidence destination");
        let name = format!("{}-functions", program.functions().len());
        // Both fixtures have the same function count; source identity differentiates these files.
        let fingerprint = program.layouts(StorageTarget::Linear32V1).fingerprint();
        let suffix = format!(
            "{:016x}",
            u64::from_be_bytes(fingerprint[..8].try_into().expect("eight fingerprint bytes"))
        );
        std::fs::write(directory.join(format!("{name}-{suffix}.wasm")), artifact.bytes())
            .expect("retain proof bytes");
    }
}

#[test]
fn final_byte_audit_rejects_independent_capability_export_local_and_operator_drift() {
    with_program(false, |program| {
        let layout = Layout::new(program).expect("lane layout");
        let bytes = super::emit(program).expect("artifact").bytes().to_vec();
        let mut extra = bytes.clone();
        extra.extend_from_slice(&[0, 2, 1, b'x']); // Valid custom section.
        assert_eq!(audit::seal(extra, &layout).expect_err("custom capability").code, "ZRYNA-W4003");
        assert_eq!(
            audit::seal(with_memory(&bytes), &layout).expect_err("unused memory capability").code,
            "ZRYNA-W4003"
        );
        let mut changed_export = bytes.clone();
        let offset = bytes.windows(5).position(|part| part == b"score").expect("export bytes");
        changed_export[offset] = b'x';
        assert_eq!(
            audit::seal(changed_export, &layout).expect_err("wrong scalar export").code,
            "ZRYNA-W4003"
        );
        let mut operator_offset = None;
        let mut global_offset = None;
        let mut local_offset = None;
        let mut function_offset = None;
        for payload in Parser::new(0).parse_all(&bytes) {
            match payload.expect("parse accepted binary") {
                Payload::GlobalSection(reader) => {
                    global_offset =
                        Some(usize::try_from(reader.range().start).expect("bounded offset") + 3);
                }
                Payload::FunctionSection(reader) => {
                    function_offset =
                        Some(usize::try_from(reader.range().start).expect("bounded offset") + 1);
                }
                Payload::CodeSectionEntry(body) => {
                    local_offset.get_or_insert(
                        usize::try_from(body.range().start).expect("bounded offset") + 1,
                    );
                    let mut ops = body.get_operators_reader().expect("operators");
                    while !ops.eof() {
                        let offset =
                            usize::try_from(ops.original_position()).expect("bounded offset");
                        if matches!(ops.read().expect("operator"), Operator::I32Add) {
                            operator_offset = Some(offset);
                        }
                    }
                }
                _ => {}
            }
        }
        let mut wrong_operator = bytes.clone();
        wrong_operator[operator_offset.expect("fixed wrapping add")] = 0x6b; // Valid i32.sub.
        assert_eq!(
            audit::seal(wrong_operator, &layout).expect_err("unapproved operator").code,
            "ZRYNA-W4003"
        );
        let mut wrong_global = bytes.clone();
        wrong_global[global_offset.expect("global instruction") + 1] = 1; // Valid nonzero initializer.
        assert_eq!(
            audit::seal(wrong_global, &layout).expect_err("return global initialization").code,
            "ZRYNA-W4003"
        );
        let mut extra_local = bytes.clone();
        extra_local[local_offset.expect("private local count")] += 1; // Still valid core locals.
        assert_eq!(
            audit::seal(extra_local, &layout).expect_err("unsealed local inventory").code,
            "ZRYNA-W4003"
        );
        let mut wrong_type = bytes.clone();
        wrong_type[function_offset.expect("function type")] = 1; // Same-arity private void signature.
        let rejected =
            audit::seal(wrong_type, &layout).expect_err("changed function type identity");
        assert_eq!(rejected.code, "ZRYNA-W4003");
        let mut malformed = bytes.clone();
        malformed.truncate(malformed.len() - 1);
        assert_eq!(
            audit::seal(malformed, &layout).expect_err("truncated final bytes").code,
            "ZRYNA-W4004"
        );
        assert_eq!(
            audit::seal(bytes, &layout).expect("recovery retains pristine replay"),
            super::emit(program).expect("pristine")
        );
    });
}

fn with_memory(bytes: &[u8]) -> Vec<u8> {
    let mut memory = wasm_encoder::MemorySection::new();
    memory.memory(wasm_encoder::MemoryType {
        minimum: 0,
        maximum: Some(0),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    let mut module = wasm_encoder::Module::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Some((id, range)) = payload.expect("accepted sections").as_section() {
            if id == 6 {
                module.section(&memory);
            }
            let start = usize::try_from(range.start).expect("bounded section");
            let end = usize::try_from(range.end).expect("bounded section");
            module.section(&wasm_encoder::RawSection { id, data: &bytes[start..end] });
        }
    }
    module.finish()
}

#[test]
fn final_artifact_first_extra_budget_precedes_binary_validation() {
    with_program(false, |program| {
        let layout = Layout::new(program).expect("real sealed lane plan");
        assert_eq!(
            audit::seal(vec![0; super::MAX_BYTES + 1], &layout)
                .expect_err("first extra final byte before decode")
                .code,
            "ZRYNA-W4001"
        );
        assert_eq!(
            audit::seal(vec![0; super::MAX_BYTES], &layout)
                .expect_err("exact-sized malformed binary receives validation")
                .code,
            "ZRYNA-W4004"
        );
    });
}

#[test]
fn incremental_artifact_exact_ceiling_and_first_extra_are_atomic() {
    let mut bytes = Bytes::new();
    let chunk = vec![0u8; 1024 * 1024];
    for _ in 0..32 {
        bytes.extend(&chunk).expect("exact 32 MiB encoding ceiling");
    }
    assert_eq!(bytes.bytes.len(), super::MAX_BYTES);
    assert_eq!(bytes.extend(&[0]).expect_err("first extra byte").code, "ZRYNA-W4001");
    assert_eq!(bytes.bytes.len(), super::MAX_BYTES);
    bytes.extend(&[]).expect("failed append retains bounded writer");
}
