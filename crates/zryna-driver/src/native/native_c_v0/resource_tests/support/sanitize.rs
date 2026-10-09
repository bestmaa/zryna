//! Independent sanitizer observation, separate from the production closed executable audit.

use super::*;
use std::ffi::OsString;

pub(super) fn run_sanitized(artifact: &ValidatedHandleEntries, client: &str) {
    let fixture = Fixture::new();
    let root = fixture.output();
    let limits = NativeProcessLimits::default();
    let tools = native::discover_linux_native_toolchain(limits).expect("retained GNU tools");
    let stage = NativeStage::create(&root, "handle-sanitizer").expect("owned sanitizer fixture");
    stage.write_input(&stage.object, artifact.bytes()).expect("audited program object");
    stage
        .write_input(&stage.harness, source(artifact, client, "", "").as_bytes())
        .expect("independent fixture and client");
    native::revalidate_tool(&tools.driver, &tools.driver_identity).expect("retained compiler");
    native::revalidate_tool(&tools.linker, &tools.linker_identity).expect("retained linker");
    let directory = stage.capability_directory_path();
    let executable = stage.capability_file_path("invocation.elf").expect("owned executable");
    let harness = stage.capability_file_path("invocation.c").expect("owned C source");
    let object = stage.capability_file_path("program.o").expect("owned object");
    let mut arguments = [
        "-std=c11",
        "-O0",
        "-g0",
        "-fno-pie",
        "-no-pie",
        "-fsanitize=address,undefined",
        "-fno-sanitize-recover=all",
        "-fno-omit-frame-pointer",
        "-o",
    ]
    .map(OsString::from)
    .to_vec();
    arguments.extend([
        executable.as_os_str().to_owned(),
        harness.as_os_str().to_owned(),
        object.as_os_str().to_owned(),
    ]);
    stage.revalidate().expect("retained stage");
    let compiled = native::run_bounded_process(
        &tools.driver,
        &arguments,
        &directory,
        limits.link_timeout(),
        limits.tool_output_bytes(),
        limits.tool_output_bytes(),
        ProcessPhase::Link,
        Some(&directory),
    )
    .expect("bounded sanitizer build");
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    assert!(compiled.stdout.is_empty() && compiled.stderr.is_empty());
    stage.revalidate().expect("retained sanitizer stage");
    // Sanitizer instrumentation and ambient imports are intentionally outside the production
    // executable seal. The generated machine object itself is not ASan-instrumented.
    let observed = native::run_bounded_process(
        &executable,
        &[],
        &directory,
        native::MAX_NATIVE_RUN_TIMEOUT,
        4096,
        native::MAX_NATIVE_RUN_STDERR_BYTES,
        ProcessPhase::Run,
        Some(&directory),
    )
    .expect("bounded sanitizer observation");
    assert!(stage.cleanup().is_empty());
    assert!(observed.status.success(), "{}", String::from_utf8_lossy(&observed.stderr));
    assert!(observed.stdout.is_empty() && observed.stderr.is_empty());
    assert_eq!(fs::read_dir(fixture.0.join(".zryna/out")).expect("complete inventory").count(), 0);
}
