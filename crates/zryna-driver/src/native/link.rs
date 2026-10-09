//! Retained-capability scalar link staging, bounded processes and executable audit.

use super::{
    ArtifactOutputRoot, LinuxX8664LinkToolchain, NativeProcessLimits, NativeStage, OsString,
    PreparedNativeBytes, ProcessPhase, audit_staged_executable, native_error, revalidate_tool,
    run_bounded_process,
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(super) fn link_and_audit_native_invocation(
    object_bytes: &[u8],
    harness_bytes: &[u8],
    expected_symbol: &str,
    output_root: &ArtifactOutputRoot,
    toolchain: &LinuxX8664LinkToolchain,
    limits: NativeProcessLimits,
) -> PreparedNativeBytes {
    let stage = NativeStage::create(output_root, "invocation").map_err(|error| vec![error])?;
    let operation = (|| {
        stage.write_input(&stage.object, object_bytes)?;
        stage.write_input(&stage.harness, harness_bytes)?;
        revalidate_tool(&toolchain.driver, &toolchain.driver_identity)?;
        revalidate_tool(&toolchain.linker, &toolchain.linker_identity)?;

        let capability_directory = stage.capability_directory_path();
        let capability_executable = stage.capability_file_path("invocation.elf")?;
        let capability_harness = stage.capability_file_path("invocation.c")?;
        let capability_object = stage.capability_file_path("program.o")?;
        let arguments = vec![
            OsString::from("-std=c11"),
            OsString::from("-O0"),
            OsString::from("-g0"),
            OsString::from("-fno-ident"),
            OsString::from("-fno-pie"),
            OsString::from("-no-pie"),
            OsString::from("-Wl,--build-id=none"),
            OsString::from("-Wl,--fatal-warnings"),
            OsString::from("-Wl,--no-undefined"),
            OsString::from("-Wl,-z,noexecstack,-z,relro,-z,now"),
            OsString::from("-o"),
            capability_executable.as_os_str().to_owned(),
            capability_harness.as_os_str().to_owned(),
            capability_object.as_os_str().to_owned(),
        ];
        stage.revalidate()?;
        let output = run_bounded_process(
            &toolchain.driver,
            &arguments,
            &capability_directory,
            limits.link_timeout(),
            limits.tool_output_bytes(),
            limits.tool_output_bytes(),
            ProcessPhase::Link,
            Some(&capability_directory),
        )?;
        if !output.status.success() {
            return Err(native_error(
                "ZRYNA-N4017",
                "the validated GNU toolchain rejected native linking",
                "verify the documented system toolchain and report the smallest reproducible source",
            ));
        }
        if !output.stdout.is_empty() || !output.stderr.is_empty() {
            return Err(native_error(
                "ZRYNA-N4017",
                "the validated GNU toolchain produced unexpected link output",
                "verify the documented system toolchain and report the smallest reproducible source",
            ));
        }
        stage.revalidate()?;
        let (_, sealed_bytes) = audit_staged_executable(&capability_executable, expected_symbol)?;
        Ok(sealed_bytes)
    })();
    let cleanup = stage.cleanup();
    match operation {
        Ok(sealed_bytes) => Ok((sealed_bytes, cleanup)),
        Err(error) => {
            let mut diagnostics = vec![error];
            diagnostics.extend(cleanup);
            Err(diagnostics)
        }
    }
}
