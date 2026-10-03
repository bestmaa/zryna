//! Genuine total scalar objects through the retained driver link and process boundary.

#[path = "../../../../zryna-native-mir/tests/native_c_v0/capture.rs"]
mod capture;

use super::*;
use crate::native::{
    self, NativeStage, ProcessPhase, prepare_executable_mode, run_bounded_process,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zryna_native_c_ir::VerifiedNativeCProgram;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "zryna-native-c-exports-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".zryna/out")).expect("private fixture output");
        Self(root)
    }
    fn output(&self) -> ArtifactOutputRoot {
        ArtifactOutputRoot::for_workspace(&self.0).expect("retained output capability")
    }
    fn assert_empty(&self) {
        assert_eq!(
            fs::read_dir(self.0.join(".zryna/out")).expect("complete output inventory").count(),
            0
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove this private test fixture");
    }
}
fn emit(source: &VerifiedNativeCProgram) -> ValidatedScalarExports {
    let machine =
        zryna_native_mir::native_c_v0::lower(source).expect("independent machine admission");
    zryna_backend_native::native_c_v0::emit_scalar_exports(
        &machine,
        zryna_backend_native::select_object_target(zryna_backend_native::NATIVE_OBJECT_TARGET)
            .expect("exact target"),
    )
    .expect("real generated and audited object")
}
fn toolchain() -> LinuxX8664LinkToolchain {
    native::discover_linux_native_toolchain(NativeProcessLimits::default())
        .expect("retained GNU toolchain")
}
fn linked_client(
    artifact: &ValidatedScalarExports,
    client: &str,
    root: &ArtifactOutputRoot,
    toolchain: &LinuxX8664LinkToolchain,
) -> Result<Vec<u8>, Vec<Diagnostic>> {
    let (bytes, cleanup) = native::link_and_audit_native_invocation(
        artifact.bytes(),
        client.as_bytes(),
        "zryna_c_v0_e_add",
        root,
        toolchain,
        NativeProcessLimits::default(),
    )?;
    assert!(cleanup.is_empty());
    Ok(bytes.into())
}
fn observe(bytes: &[u8], root: &ArtifactOutputRoot) -> native::BoundedProcessOutput {
    let stage = NativeStage::create(root, "c-client").expect("retained execution stage");
    stage.write_input(&stage.executable, bytes).expect("exact executable snapshot");
    let executable = stage.capability_file_path("invocation.elf").expect("capability executable");
    prepare_executable_mode(&executable).expect("private execution mode");
    stage.revalidate().expect("retained identity");
    let directory = stage.capability_directory_path();
    let output = run_bounded_process(
        &executable,
        &[],
        &directory,
        native::MAX_NATIVE_RUN_TIMEOUT,
        4096,
        native::MAX_NATIVE_RUN_STDERR_BYTES,
        ProcessPhase::Run,
        Some(&directory),
    )
    .expect("bounded completed process, including deliberate signal cases");
    assert!(stage.cleanup().is_empty());
    output
}

#[test]
fn typed_scalar_export_driver_links_runs_wrapping_edges_and_reuses_sealed_bytes() {
    let fixture = Fixture::new();
    let root = fixture.output();
    let toolchain = toolchain();
    let (_, source) = capture::reference();
    let artifact = emit(&source);
    for (left, right, expected) in
        [(20, 22, 42), (i32::MAX, 1, i32::MIN), (i32::MIN, -1, i32::MAX), (i32::MIN, i32::MIN, 0)]
    {
        let arguments = [ScalarValue::I32(left), ScalarValue::I32(right)];
        let first = prepare_scalar_export(
            &artifact,
            "add",
            &arguments,
            &root,
            &toolchain,
            NativeProcessLimits::default(),
        )
        .expect("checked invocation");
        let repeated = prepare_scalar_export(
            &artifact,
            "add",
            &arguments,
            &root,
            &toolchain,
            NativeProcessLimits::default(),
        )
        .expect("repeat exact invocation");
        assert_eq!(first.bytes(), repeated.bytes());
        assert_eq!(
            first.object().program().source().source_map_identity(),
            source.source_map_identity()
        );
        assert_eq!(
            first.run(&root, NativeProcessLimits::default()).expect("typed framed result"),
            ScalarOutcome::Returned { value: ScalarValue::I32(expected) }
        );
        fixture.assert_empty();
    }
}

#[test]
fn unchanged_reverse_c_client_reads_full_width_results_from_real_zryna_export() {
    let fixture = Fixture::new();
    let root = fixture.output();
    let artifact = emit(&capture::reference().1);
    let client = include_str!("../../../../../tests/native-c-prototype/reverse_client.c")
        .replace("#include \"prototype_exports.h\"", artifact.header());
    let bytes = linked_client(&artifact, &client, &root, &toolchain())
        .expect("exact generated header and object link");
    let output = observe(&bytes, &root);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"native-c-prototype: reverse scalar observations passed\n");
    assert!(output.stderr.is_empty());
    fixture.assert_empty();
}

#[test]
fn wrong_arity_type_import_and_private_names_fail_before_staging() {
    let fixture = Fixture::new();
    let root = fixture.output();
    let toolchain = toolchain();
    let artifact = emit(&capture::boolean_import());
    let cases = [
        ("add", vec![]),
        ("add", vec![ScalarValue::I32(1)]),
        ("add", vec![ScalarValue::I32(1), ScalarValue::I32(2), ScalarValue::I32(3)]),
        ("add", vec![ScalarValue::Bool(true), ScalarValue::I32(2)]),
        ("fixture_boolean_shim", vec![ScalarValue::Bool(true)]),
        ("zryna_c_v0_e_add", vec![ScalarValue::I32(1), ScalarValue::I32(2)]),
        ("zryna_c_v0_i_dispatch", vec![]),
        ("ADD", vec![ScalarValue::I32(1), ScalarValue::I32(2)]),
    ];
    for (logical, arguments) in cases {
        let error = prepare_scalar_export(
            &artifact,
            logical,
            &arguments,
            &root,
            &toolchain,
            NativeProcessLimits::default(),
        )
        .expect_err("no invalid invocation seal");
        assert_eq!(error[0].code(), "ZRYNA-C4104");
        fixture.assert_empty();
    }
}

#[test]
fn generated_header_rejects_wrong_arity_type_and_missing_symbol_without_publication() {
    let fixture = Fixture::new();
    let root = fixture.output();
    let toolchain = toolchain();
    let artifact = emit(&capture::reference().1);
    for client in [
        "_Static_assert(_Generic(&zryna_c_v0_e_add, int32_t (*)(int32_t): 1, default: 0), \"wrong arity\"); int main(void) { return 0; }",
        "_Static_assert(_Generic(&zryna_c_v0_e_add, uint32_t (*)(uint32_t, uint32_t): 1, default: 0), \"wrong type\"); int main(void) { return 0; }",
        "extern int32_t missing_export(int32_t, int32_t); int main(void) { return missing_export(20,22); }",
    ] {
        let source = format!("{}\n{client}\n", artifact.header());
        let error = linked_client(&artifact, &source, &root, &toolchain)
            .expect_err("closed header or missing symbol failure");
        assert_eq!(error[0].code(), "ZRYNA-N4017");
        fixture.assert_empty();
    }
}

#[test]
fn stack_scalar_arguments_execute_exact_system_v_lanes_and_c_int_spelling() {
    let fixture = Fixture::new();
    let root = fixture.output();
    let toolchain = toolchain();
    for count in [7, 8, 9, 16] {
        let artifact = emit(&capture::stack_export(count));
        let arguments = (1..=count)
            .map(|index| ScalarValue::I32(i32::try_from(index).expect("bounded inputs")))
            .collect::<Vec<_>>();
        let invocation = prepare_scalar_export(
            &artifact,
            "add",
            &arguments,
            &root,
            &toolchain,
            NativeProcessLimits::default(),
        )
        .expect("stack invocation");
        assert_eq!(
            invocation.run(&root, NativeProcessLimits::default()).expect("full stack result"),
            ScalarOutcome::Returned {
                value: ScalarValue::I32(i32::try_from(count * (count + 1) / 2).expect("small sum"))
            }
        );
        fixture.assert_empty();
    }
    let artifact = emit(&capture::c_int_export());
    assert!(artifact.header().contains("int zryna_c_v0_e_add(int arg0, int arg1);"));
    let invocation = prepare_scalar_export(
        &artifact,
        "add",
        &[ScalarValue::I32(20), ScalarValue::I32(22)],
        &root,
        &toolchain,
        NativeProcessLimits::default(),
    )
    .expect("exact C-int invocation");
    assert_eq!(
        invocation.run(&root, NativeProcessLimits::default()).expect("C-int result"),
        ScalarOutcome::Returned { value: ScalarValue::I32(42) }
    );
    fixture.assert_empty();
    let constant = emit(&capture::constant_export());
    let invocation = prepare_scalar_export(
        &constant,
        "add",
        &[],
        &root,
        &toolchain,
        NativeProcessLimits::default(),
    )
    .expect("zero-arity constant export");
    assert_eq!(
        invocation.run(&root, NativeProcessLimits::default()).expect("literal full-width result"),
        ScalarOutcome::Returned { value: ScalarValue::I32(i32::MIN) }
    );
    fixture.assert_empty();
}

#[test]
fn bool32_c_client_invalid_low_width_carrier_terminates_without_scalar_result() {
    let fixture = Fixture::new();
    let root = fixture.output();
    let toolchain = toolchain();
    let artifact = emit(&capture::boolean_export());
    for value in [false, true] {
        let invocation = prepare_scalar_export(
            &artifact,
            "add",
            &[ScalarValue::Bool(value)],
            &root,
            &toolchain,
            NativeProcessLimits::default(),
        )
        .expect("canonical Boolean invocation");
        assert_eq!(
            invocation.run(&root, NativeProcessLimits::default()).expect("Boolean result"),
            ScalarOutcome::Returned { value: ScalarValue::Bool(value) }
        );
    }
    for invalid in [2_u32, u32::MAX] {
        let client = format!(
            "{}\n#include <stdio.h>\nint main(void) {{ printf(\"%u\", zryna_c_v0_e_add(UINT32_C({invalid}))); return 0; }}\n",
            artifact.header()
        );
        let bytes = linked_client(&artifact, &client, &root, &toolchain)
            .expect("hostile raw client is independently compiled");
        let output = observe(&bytes, &root);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        fixture.assert_empty();
    }
}
