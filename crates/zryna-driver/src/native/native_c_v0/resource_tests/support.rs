//! Retained driver staging for test-only reviewed fixture linkage and independent observations.

use super::capture;
use crate::native::{self, ArtifactOutputRoot, NativeProcessLimits, NativeStage, ProcessPhase};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zryna_backend_native::native_c_v0::resources::ValidatedHandleEntries;

mod sanitize;
pub(super) fn run_sanitized(artifact: &ValidatedHandleEntries, client: &str) {
    sanitize::run_sanitized(artifact, client);
}

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "zryna-c-handles-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".zryna/out")).expect("private output");
        Self(root)
    }
    fn output(&self) -> ArtifactOutputRoot {
        ArtifactOutputRoot::for_workspace(&self.0).expect("retained output capability")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove owned test output");
    }
}

pub(super) fn emit(capture: &capture::Capture, name: &str) -> ValidatedHandleEntries {
    let ir = zryna_native_c_ir::lower(&capture.sources, &capture.authority).expect("genuine IR");
    let mir = zryna_native_mir::native_c_v0::lower(&ir).expect("genuine machine seal");
    let symbol = mir
        .functions()
        .find(|function| function.name() == name)
        .expect("selected original body")
        .entry()
        .symbol
        .clone();
    zryna_backend_native::native_c_v0::resources::emit_handle_entries(
        &mir,
        &[&symbol],
        zryna_backend_native::select_object_target(zryna_backend_native::NATIVE_OBJECT_TARGET)
            .expect("exact target"),
    )
    .expect("independently audited handle object")
}

pub(super) fn entry(artifact: &ValidatedHandleEntries, name: &str) -> String {
    artifact
        .program()
        .functions()
        .find(|function| function.name() == name)
        .expect("original body")
        .entry()
        .symbol
        .clone()
}

pub(super) fn run(artifact: &ValidatedHandleEntries, client: &str, success: bool) {
    let _ = run_fault(artifact, client, "", "", success);
}

pub(super) fn run_fault(
    artifact: &ValidatedHandleEntries,
    client: &str,
    prefix: &str,
    replacement: &str,
    success: bool,
) -> std::process::ExitStatus {
    let fixture = Fixture::new();
    let root = fixture.output();
    let toolchain = native::discover_linux_native_toolchain(NativeProcessLimits::default())
        .expect("retained GNU tools");
    let source = source(artifact, client, prefix, replacement);
    let (bytes, diagnostics) = native::link_and_audit_native_invocation(
        artifact.bytes(),
        source.as_bytes(),
        "zryna_c_v0_i_dispatch",
        &root,
        &toolchain,
        NativeProcessLimits::default(),
    )
    .expect("test-only reviewed C fixture link and executable audit");
    assert!(diagnostics.is_empty());
    let stage = NativeStage::create(&root, "handle-client").expect("retained stage");
    stage.write_input(&stage.executable, &bytes).expect("exact executable bytes");
    let executable = stage.capability_file_path("invocation.elf").expect("retained executable");
    native::prepare_executable_mode(&executable).expect("private execution mode");
    stage.revalidate().expect("retained identity");
    let directory = stage.capability_directory_path();
    let output = native::run_bounded_process(
        &executable,
        &[],
        &directory,
        native::MAX_NATIVE_RUN_TIMEOUT,
        4096,
        native::MAX_NATIVE_RUN_STDERR_BYTES,
        ProcessPhase::Run,
        Some(&directory),
    )
    .expect("bounded process completion");
    assert!(stage.cleanup().is_empty());
    assert_eq!(
        output.status.success(),
        success,
        "status={:?} stderr={:?}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    if success {
        assert!(output.stderr.is_empty());
    }
    assert_eq!(
        fs::read_dir(fixture.0.join(".zryna/out")).expect("complete output inventory").count(),
        0
    );
    output.status
}

fn source(
    artifact: &ValidatedHandleEntries,
    client: &str,
    prefix: &str,
    replacement: &str,
) -> String {
    let fixture_source = include_str!("../../../../../../tests/native-c-prototype/fixture.c")
        .replace(
            "#include \"../native-c-abi-v0/candidate.h\"",
            std::str::from_utf8(capture::HEADER).expect("captured header"),
        )
        .replace(
            "#include \"fixture_control.h\"",
            include_str!("../../../../../../tests/native-c-prototype/fixture_control.h"),
        );
    let fixture_source = if replacement.is_empty() {
        fixture_source
    } else {
        fixture_source
            .replace("int32_t fixture_open(", "int32_t reviewed_open(")
            .replace("void fixture_close(", "void reviewed_close(")
    };
    let fixture_source = if replacement.contains("int32_t fixture_read(") {
        fixture_source.replace("int32_t fixture_read(", "int32_t reviewed_read(")
    } else {
        fixture_source
    };
    format!("{}\n{prefix}\n{fixture_source}\n{replacement}\n{client}", artifact.header())
}
