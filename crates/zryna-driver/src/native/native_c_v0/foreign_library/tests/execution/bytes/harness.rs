//! Three separately compiled inputs; sanitizer observations carry no production ELF seal.

use super::*;
use crate::native::{self, NativeStage, ProcessPhase};
use std::{ffi::OsString, path::PathBuf};
mod runtime;

const SANITIZERS: [&str; 3] =
    ["-fsanitize=address,undefined", "-fno-sanitize-recover=all", "-fno-omit-frame-pointer"];

pub(super) fn observe(
    fixture: &Fixture,
    requirements: &HandleLinkRequirements,
    captured: &CapturedForeignLibrary,
    library: &str,
    client: &str,
    sanitized: bool,
) -> native::BoundedProcessOutput {
    observe_with_timeout(
        fixture,
        requirements,
        captured,
        library,
        client,
        sanitized,
        native::MAX_NATIVE_RUN_TIMEOUT,
    )
    .expect("actual separately linked byte execution")
}

pub(super) fn observe_with_timeout(
    fixture: &Fixture,
    requirements: &HandleLinkRequirements,
    captured: &CapturedForeignLibrary,
    library: &str,
    client: &str,
    sanitized: bool,
    timeout: std::time::Duration,
) -> Result<native::BoundedProcessOutput, Diagnostic> {
    captured.check_binding(requirements).expect("genuine original byte issuer");
    // A separately rebuilt ordinary object must match the captured artifact before this source
    // may produce an instrumented observation object. This is provenance, never approval.
    assert_eq!(sha(&fixture.compile(library)), *captured.object_sha256());
    let foreign = if sanitized {
        let bytes = fixture.compile_variant(library, &SANITIZERS);
        reject(requirements, &bytes, &["free", "malloc"]);
        bytes
    } else {
        captured.bytes().to_vec()
    };
    let (private_stage, runtime_bytes) = runtime::compile(fixture, requirements, sanitized);
    let runtime_object =
        private_stage.capability_file_path("runtime.o").expect("private object capability");
    let stage = fixture.stage();
    stage
        .write_input(&stage.object, requirements.object().bytes())
        .expect("independent byte fixture prerequisite");
    stage
        .write_input(&stage.directory.join("runtime.o"), &foreign)
        .expect("independent byte fixture prerequisite");
    let private_header =
        requirements.object().program().source().runtime_abi().native_linux_x86_64_header();
    assert_eq!(
        &sha(private_header),
        requirements
            .private_runtime_header_sha256()
            .expect("independent byte fixture prerequisite")
    );
    let source = format!(
        "{}\n{}\n#include <assert.h>\n#include <stdlib.h>\n#include <string.h>\n{}\n{client}",
        std::str::from_utf8(private_header).expect("independent byte fixture prerequisite"),
        requirements.object().header(),
        include_str!("oracle.c")
    );
    stage
        .write_input(&stage.harness, source.as_bytes())
        .expect("independent byte fixture prerequisite");
    let executable = stage
        .capability_file_path("invocation.elf")
        .expect("independent byte fixture prerequisite");
    let args = link_arguments(&stage, runtime_object, sanitized);
    private_stage.revalidate().expect("independent byte fixture prerequisite");
    let linked = super::super::harness::compile(&stage, &args);
    assert!(linked.status.success(), "{}", String::from_utf8_lossy(&linked.stderr));
    assert!(linked.stdout.is_empty() && linked.stderr.is_empty());
    if !sanitized {
        native::audit_staged_executable(&executable, "zryna_c_v0_i_dispatch")
            .expect("independent byte fixture prerequisite");
    }
    // The instrumented foreign/runtime/client objects are independent observations, outside
    // the production capture and limited executable seal. The generated machine code is not ASan.
    eprintln!(
        "417-byte-evidence {}",
        serde_json::json!({
        "sanitized":sanitized,"library_source_sha256":hex(&sha(library.as_bytes())),
        "captured_foreign_object_sha256":hex(captured.object_sha256()),
        "observed_foreign_object_sha256":hex(&sha(&foreign)),
        "private_source_sha256":hex(&sha(requirements.private_runtime_source().expect("private source identity"))),"private_object_sha256":hex(&sha(&runtime_bytes)),
        "client_source_sha256":hex(&sha(source.as_bytes())),
        "program_object_sha256":hex(requirements.object_sha256()),
        "private_header_sha256":hex(requirements.private_runtime_header_sha256().expect("private ABI identity")),
        "declaration_sha256":hex(requirements.declaration_sha256()),
        "library_header_sha256":hex(requirements.libraries()[0].header_sha256()),
        "library_policy_sha256":hex(requirements.libraries()[0].policy_sha256())})
    );
    native::prepare_executable_mode(&executable).expect("independent byte fixture prerequisite");
    stage.revalidate().expect("independent byte fixture prerequisite");
    private_stage.revalidate().expect("independent byte fixture prerequisite");
    let directory = stage.capability_directory_path();
    let result = native::run_bounded_process(
        &executable,
        &[],
        &directory,
        timeout,
        4096,
        native::MAX_NATIVE_RUN_STDERR_BYTES,
        ProcessPhase::Run,
        Some(&directory),
    );
    assert!(stage.cleanup().is_empty());
    assert!(private_stage.cleanup().is_empty());
    fixture.empty();
    if let Ok(output) = &result {
        assert!(output.stdout.is_empty());
        if output.status.success() {
            assert!(output.stderr.is_empty());
        }
    }
    result
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().fold(String::with_capacity(64), |mut text, byte| {
        write!(text, "{byte:02x}").expect("hex digest write");
        text
    })
}

fn link_arguments(stage: &NativeStage, runtime_object: PathBuf, sanitized: bool) -> Vec<OsString> {
    let executable = stage.capability_file_path("invocation.elf").expect("executable capability");
    let mut args: Vec<OsString> = [
        "-std=c11",
        "-O0",
        "-g0",
        "-fno-ident",
        "-fno-pie",
        "-no-pie",
        "-Wl,--build-id=none",
        "-Wl,--fatal-warnings",
        "-Wl,--no-undefined",
        "-Wl,-z,noexecstack,-z,relro,-z,now",
        "-Wl,--wrap=malloc",
        "-Wl,--wrap=free",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    if sanitized {
        args.extend(SANITIZERS.map(Into::into));
    }
    args.extend([
        "-o".into(),
        executable.clone().into_os_string(),
        stage
            .capability_file_path("invocation.c")
            .expect("independent byte fixture prerequisite")
            .into_os_string(),
        stage
            .capability_file_path("program.o")
            .expect("independent byte fixture prerequisite")
            .into_os_string(),
        stage
            .capability_file_path("runtime.o")
            .expect("independent byte fixture prerequisite")
            .into_os_string(),
        runtime_object.into_os_string(),
    ]);
    args
}
