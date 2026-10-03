use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::fmt::Write as _;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use zryna_abi::ScalarOutcome;
use zryna_abi::{Invocation, ScalarValue};
use zryna_semantics::data_ownership_v1::{SemanticInput, lower};
use zryna_source::{NormalizedSourcePath, SourceFileInput, SourceMap};
use zryna_syntax::v4::{decode_snapshot, verify_snapshot};

use super::*;
use crate::discover_linux_native_toolchain;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use crate::run_native_invocation;

const SOURCE: &str = include_str!("../../../../../tests/m3-fixtures/pair-score-v4.zry");
const SNAPSHOT: &str = include_str!("../../../../../tests/m3-fixtures/pair-score-v4.json");
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const PAIR_ORACLE: &str = include_str!("../../../../../tests/m3-fixtures/pair-oracle-v1.json");
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const OWNED_TYPES_SOURCE: &str = "interface OwnedBox extends ZrynaStruct { value: String; }\ninterface Node extends ZrynaStruct { children: Vec<Node>; }\nfunction inspect(a: Vec<String>, b: Vec<String>, box: OwnedBox, node: Node): i32 { const xs: Vec<String> = Vec<String>([\"x\"]); return 0; }";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const OWNED_CONTRACTS: &str =
    include_str!("../../../../zryna-semantics/src/data_ownership_v1/tests/aggregate_contracts.rs");
static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct TestRoot {
    workspace: PathBuf,
    output: ArtifactOutputRoot,
}

impl TestRoot {
    fn new() -> Self {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let workspace = std::env::temp_dir()
            .join(format!("zryna-ownership-native-{}-{sequence}", std::process::id()));
        fs::create_dir(&workspace).expect("test workspace");
        let output = ArtifactOutputRoot::prepare_for_workspace(&workspace).expect("output root");
        Self { workspace, output }
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.workspace).expect("test workspace cleanup");
    }
}

fn verified() -> zryna_semantics::data_ownership_v1::VerifiedProgram {
    verified_export(SOURCE, SNAPSHOT, 0, 62)
}

fn verified_export(
    original_source: &str,
    original_snapshot: &str,
    function_index: usize,
    function_start: u64,
) -> zryna_semantics::data_ownership_v1::VerifiedProgram {
    const EXPORT_BYTES: u64 = 7;
    let source = format!(
        "{}export {}",
        &original_source[..usize::try_from(function_start).expect("function offset")],
        &original_source[usize::try_from(function_start).expect("function offset")..]
    );
    let mut snapshot: serde_json::Value =
        serde_json::from_str(original_snapshot).expect("JSON snapshot");
    shift_spans(&mut snapshot, function_start, EXPORT_BYTES);
    snapshot["files"][0]["functions"][function_index]["export_span"] = serde_json::json!({
        "file": 0,
        "start": function_start,
        "end": function_start + 6,
    });
    snapshot["files"][0]["functions"][function_index]["span"]["start"] =
        serde_json::Value::from(function_start);
    let snapshot = serde_json::to_vec(&snapshot).expect("shifted snapshot");
    let sources =
        SourceMap::build(vec![SourceFileInput { path: "src/main.zry".to_owned(), text: source }])
            .expect("source map");
    let syntax = verify_snapshot(decode_snapshot(&snapshot).expect("snapshot"), &sources)
        .expect("verified syntax");
    let entry =
        sources.file_id(&NormalizedSourcePath::new("src/main.zry").expect("path")).expect("entry");
    lower(SemanticInput::try_new(&syntax, &sources, entry).expect("semantic input"))
        .expect("verified program")
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn verified_private(
    source: &str,
    snapshot: &str,
) -> zryna_semantics::data_ownership_v1::VerifiedProgram {
    let sources = SourceMap::build(vec![SourceFileInput {
        path: "src/main.zry".to_owned(),
        text: source.to_owned(),
    }])
    .expect("source map");
    let syntax = verify_snapshot(decode_snapshot(snapshot.as_bytes()).expect("snapshot"), &sources)
        .expect("verified syntax");
    let entry =
        sources.file_id(&NormalizedSourcePath::new("src/main.zry").expect("path")).expect("entry");
    lower(SemanticInput::try_new(&syntax, &sources, entry).expect("semantic input"))
        .expect("verified program")
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn owned_contract_program() -> zryna_semantics::data_ownership_v1::VerifiedProgram {
    const PREFIX: &str = "const OWNED_TYPES_RESPONSE: &str = r#\"";
    let response = OWNED_CONTRACTS
        .split_once(PREFIX)
        .expect("owned response prefix")
        .1
        .split_once("\"#;")
        .expect("owned response suffix")
        .0
        .replacen(
            "\"end\":192}}},{\"span\":{\"file\":0,\"start\":195",
            "\"end\":192}}}},{\"span\":{\"file\":0,\"start\":195",
            1,
        )
        .replacen(
            "\"end\":198}}},{\"span\":{\"file\":0,\"start\":215",
            "\"end\":198}}}},{\"span\":{\"file\":0,\"start\":215",
            1,
        );
    let response: serde_json::Value = serde_json::from_str(&response).expect("owned response");
    let snapshot = serde_json::to_string(&response["result"]).expect("owned snapshot");
    verified_private(OWNED_TYPES_SOURCE, &snapshot)
}

fn shift_spans(value: &mut serde_json::Value, threshold: u64, shift: u64) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                shift_spans(value, threshold, shift);
            }
        }
        serde_json::Value::Object(fields) => {
            if fields.contains_key("file")
                && fields.contains_key("start")
                && fields.contains_key("end")
            {
                for key in ["start", "end"] {
                    if let Some(offset) = fields.get_mut(key)
                        && offset.as_u64().is_some_and(|offset| offset >= threshold)
                    {
                        *offset = serde_json::Value::from(offset.as_u64().expect("offset") + shift);
                    }
                }
            } else {
                for value in fields.values_mut() {
                    shift_spans(value, threshold, shift);
                }
            }
        }
        _ => {}
    }
}

fn invocation() -> Invocation {
    Invocation::new("pairScore".to_owned(), vec![ScalarValue::I32(2), ScalarValue::I32(3)])
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn ownership_objects_link_publish_create_only_and_execute() {
    use std::os::unix::fs::PermissionsExt as _;

    let root = TestRoot::new();
    let program = verified();
    let limits = NativeProcessLimits::default();
    let toolchain = discover_linux_native_toolchain(limits).expect("toolchain");
    let first = prepare_data_ownership_executable(
        &program,
        invocation(),
        &root.output,
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        limits,
    )
    .expect("first executable");
    let second = prepare_data_ownership_executable(
        &program,
        invocation(),
        &root.output,
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        limits,
    )
    .expect("deterministic executable");
    assert_eq!(first.object_bytes(), second.object_bytes());
    assert_eq!(first.executable_bytes(), second.executable_bytes());
    assert_eq!(first.identity(), second.identity());
    assert_eq!(first.identity().target(), zryna_backend_native::NATIVE_OBJECT_TARGET);
    assert_eq!(first.identity().logical_export(), "pairScore");

    let object = publish_data_ownership_object(&first, &root.output, "pair-score")
        .expect("published object");
    let executable = publish_data_ownership_executable(&first, &root.output, "pair-score")
        .expect("published executable");
    assert_eq!(fs::read(object.path()).expect("object bytes"), first.object_bytes());
    assert_eq!(fs::read(executable.path()).expect("executable bytes"), first.executable_bytes());
    assert_eq!(fs::metadata(executable.path()).expect("mode").permissions().mode() & 0o777, 0o755);
    assert_eq!(executable.data_ownership_identity(), Some(first.identity()));
    assert_eq!(
        run_native_invocation(&executable, limits).expect("native outcome"),
        ScalarOutcome::Returned { value: ScalarValue::I32(65) }
    );

    let object_before = fs::read(object.path()).expect("original object");
    let executable_before = fs::read(executable.path()).expect("original executable");
    assert!(publish_data_ownership_object(&first, &root.output, "pair-score").is_err());
    assert!(publish_data_ownership_executable(&first, &root.output, "pair-score").is_err());
    assert_eq!(fs::read(object.path()).expect("retained object"), object_before);
    assert_eq!(fs::read(executable.path()).expect("retained executable"), executable_before);
    assert!(fs::read_dir(root.output.path()).expect("output entries").all(|entry| {
        !entry.expect("entry").file_name().to_string_lossy().starts_with(".zryna-link")
    }));
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn published_executables_match_the_fixed_pair_oracle() {
    let root = TestRoot::new();
    let program = verified();
    let limits = NativeProcessLimits::default();
    let toolchain = discover_linux_native_toolchain(limits).expect("toolchain");
    let oracle: serde_json::Value = serde_json::from_str(PAIR_ORACLE).expect("pair oracle");
    for case in oracle["cases"].as_array().expect("oracle cases") {
        let arguments = case["arguments"]
            .as_array()
            .expect("arguments")
            .iter()
            .map(|argument| {
                ScalarValue::I32(
                    i32::try_from(argument["value"].as_i64().expect("i32 value"))
                        .expect("bounded i32"),
                )
            })
            .collect();
        let expected = ScalarValue::I32(
            i32::try_from(case["expected"]["value"].as_i64().expect("expected value"))
                .expect("bounded expected i32"),
        );
        let prepared = prepare_data_ownership_executable(
            &program,
            Invocation::new("pairScore".to_owned(), arguments),
            &root.output,
            zryna_backend_native::NATIVE_OBJECT_TARGET,
            &toolchain,
            limits,
        )
        .expect("oracle executable");
        let stem = case["id"].as_str().expect("case id");
        let executable = publish_data_ownership_executable(&prepared, &root.output, stem)
            .expect("published oracle executable");
        assert_eq!(
            run_native_invocation(&executable, limits).expect("oracle observation"),
            ScalarOutcome::Returned { value: expected },
            "oracle case {stem}"
        );
    }
}

#[test]
fn unsupported_target_fails_before_staging_or_publication() {
    let root = TestRoot::new();
    let program = verified();
    let toolchain = discover_linux_native_toolchain(NativeProcessLimits::default());
    if let Ok(toolchain) = toolchain {
        let failure = prepare_data_ownership_executable(
            &program,
            invocation(),
            &root.output,
            "x86_64-linux-gnu",
            &toolchain,
            NativeProcessLimits::default(),
        )
        .expect_err("target alias must fail");
        assert_eq!(failure[0].code(), "ZRYNA-N3001");
    }
    assert_eq!(fs::read_dir(root.output.path()).expect("output entries").count(), 0);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn hostile_invocation_and_executable_publish_nothing() {
    let root = TestRoot::new();
    let program = verified();
    let limits = NativeProcessLimits::default();
    let toolchain = discover_linux_native_toolchain(limits).expect("toolchain");
    let wrong_type = prepare_data_ownership_executable(
        &program,
        Invocation::new("pairScore".to_owned(), vec![ScalarValue::Bool(true), ScalarValue::I32(3)]),
        &root.output,
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        limits,
    )
    .expect_err("wrong typed invocation");
    assert_eq!(wrong_type[0].code(), "ZRYNA-B2103");

    let mut tampered = prepare_data_ownership_executable(
        &program,
        invocation(),
        &root.output,
        zryna_backend_native::NATIVE_OBJECT_TARGET,
        &toolchain,
        limits,
    )
    .expect("prepared executable");
    tampered.executable.bytes = Arc::from(b"not an ELF".as_slice());
    let rejected = publish_data_ownership_executable(&tampered, &root.output, "tampered")
        .expect_err("tampered executable");
    assert_eq!(rejected[0].code(), "ZRYNA-N4018");
    assert_eq!(fs::read_dir(root.output.path()).expect("output entries").count(), 0);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn generated_owned_cleanup_executes_before_scalar_return() {
    let program = owned_contract_program();
    let mir =
        zryna_native_mir::data_ownership_v1::lower(program.verified_ir(), program.runtime_abi())
            .expect("native MIR");
    let target = select_object_target(zryna_backend_native::NATIVE_OBJECT_TARGET).expect("target");
    let object =
        zryna_backend_native::data_ownership_v1::emit_object(&mir, target).expect("program object");
    let function = mir.functions().next().expect("inspect function");
    let mut declarations = String::new();
    let mut arguments = String::new();
    for (index, parameter) in function.parameters().enumerate() {
        let layout = mir.types().find(|ty| ty.id() == parameter.ty()).expect("parameter layout");
        writeln!(
            declarations,
            "  uintptr_t p{index} = 0; if (zryna_m3_allocate_record({}, {}, &p{index}) != 0) return {}; memset((void *)p{index}, 0, {});",
            layout.size(),
            layout.alignment(),
            index + 10,
            layout.size(),
        )
        .expect("in-memory declaration rendering");
        if index != 0 {
            arguments.push_str(", ");
        }
        write!(arguments, "p{index}").expect("in-memory argument rendering");
    }
    let mut runtime = crate::ownership_runtime_v1::render_source(&mir);
    runtime.extend_from_slice(
        format!(
            r"
#include <stdio.h>
extern int32_t zryna_m3_m0_f0(uintptr_t, uintptr_t, uintptr_t, uintptr_t);
uint32_t zryna_m3_observe(uint32_t command) {{ (void)command; return 0; }}
int main(void) {{
{declarations}  if (zryna_m3_m0_f0({arguments}) != 0) return 1;
  if (zryna_m3_finish_invocation() != 0 || allocation_head != NULL) return 2;
  return 0;
}}
"
        )
        .as_bytes(),
    );

    let root = TestRoot::new();
    let source = root.workspace.join("cleanup.c");
    let object_path = root.workspace.join("program.o");
    let executable = root.workspace.join("cleanup.elf");
    fs::write(&source, runtime).expect("runtime harness");
    fs::write(&object_path, object.bytes()).expect("object bytes");
    let output = crate::process_spawn::output(
        std::process::Command::new("/usr/bin/gcc")
            .args([
                "-std=c11",
                "-pedantic",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-O2",
                "-fno-stack-protector",
                "-fno-pie",
                "-no-pie",
            ])
            .arg(&source)
            .arg(&object_path)
            .arg("-o")
            .arg(&executable),
    )
    .expect("cleanup verifier compile");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        crate::process_spawn::status(&mut std::process::Command::new(&executable))
            .expect("cleanup verifier execute")
            .success()
    );
}
