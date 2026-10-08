//! Reviewed fixture process helper; not a production admission API or OS-isolation proof.

use super::super::super::linked_output;
use super::*;
use crate::native::{self, ArtifactOutputRoot, NativeProcessLimits, NativeStage, ProcessPhase};
use std::{
    ffi::OsString,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

mod observation_tests;

static NEXT: AtomicUsize = AtomicUsize::new(0);
pub(super) struct Fixture(PathBuf);
impl Fixture {
    pub(super) fn observation_root(&self) -> Result<ArtifactOutputRoot, Diagnostic> {
        ArtifactOutputRoot::for_workspace(&self.0)
    }
    pub(super) fn compile_observed(
        &self,
        source: &str,
    ) -> Result<linked_output::CompiledObject, linked_output::Failure> {
        let tools = native::discover_linux_native_toolchain(NativeProcessLimits::default())?;
        linked_output::compile_object(&self.observation_root()?, source.as_bytes(), &tools)
    }
    pub(super) fn observe_link(
        &self,
        requirements: &HandleLinkRequirements,
        library: &CapturedForeignLibrary,
        foreign: &linked_output::CompiledObject,
        client: &str,
    ) -> Result<linked_output::Observation, linked_output::Failure> {
        let source = format!(
            "{}\n#include <assert.h>\n#include <stdlib.h>\n#include <limits.h>\n{client}",
            requirements.object().header()
        );
        linked_output::observe(
            &self.observation_root()?,
            requirements,
            library,
            foreign,
            source.as_bytes(),
            None,
            false,
        )
    }
    pub(super) fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zryna-foreign-object-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join(".zryna/out")).expect("independent fixture prerequisite");
        Self(path)
    }
    pub(super) fn acquired_path(&self) -> PathBuf {
        self.0.join("acquired.o")
    }
    pub(super) fn empty(&self) {
        assert_eq!(
            fs::read_dir(self.0.join(".zryna/out"))
                .expect("independent fixture prerequisite")
                .count(),
            0
        );
    }
    pub(super) fn stage(&self) -> NativeStage {
        NativeStage::create(
            &ArtifactOutputRoot::for_workspace(&self.0).expect("independent fixture prerequisite"),
            "foreign-object",
        )
        .expect("independent fixture prerequisite")
    }
    pub(super) fn compile(&self, source: &str) -> Vec<u8> {
        self.compile_variant(source, &[])
    }
    pub(super) fn compile_variant(&self, source: &str, extra: &[&str]) -> Vec<u8> {
        let stage = self.stage();
        stage
            .write_input(&stage.harness, source.as_bytes())
            .expect("independent fixture prerequisite");
        let object =
            stage.capability_file_path("runtime.o").expect("independent fixture prerequisite");
        let harness =
            stage.capability_file_path("invocation.c").expect("independent fixture prerequisite");
        let mut args: Vec<OsString> = [
            "-std=c11",
            "-O0",
            "-g0",
            "-fno-ident",
            "-fno-pie",
            "-fno-pic",
            "-fcf-protection=none",
            "-fno-unwind-tables",
            "-fno-asynchronous-unwind-tables",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-c",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        args.extend(extra.iter().map(OsString::from));
        args.extend([
            OsString::from("-o"),
            object.as_os_str().to_owned(),
            harness.as_os_str().to_owned(),
        ]);
        let output = compile(&stage, &args);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        let bytes = fs::read(object).expect("independent fixture prerequisite");
        assert!(stage.cleanup().is_empty());
        self.empty();
        bytes
    }
    pub(super) fn run(
        &self,
        requirements: &HandleLinkRequirements,
        snapshot: &CapturedForeignLibrary,
        client: &str,
        wrap: bool,
        link_success: bool,
    ) -> std::process::ExitStatus {
        snapshot.check_binding(requirements).expect("independent fixture prerequisite");
        let stage = self.stage();
        stage
            .write_input(&stage.object, requirements.object().bytes())
            .expect("independent fixture prerequisite");
        stage
            .write_input(&stage.directory.join("runtime.o"), snapshot.bytes())
            .expect("independent fixture prerequisite");
        assert!(
            requirements.private_runtime_source().is_none(),
            "handle/scalar fixture needs no private runtime"
        );
        let source = format!(
            "{}\n#include <assert.h>\n#include <stdlib.h>\n#include <limits.h>\n{client}",
            requirements.object().header()
        );
        stage
            .write_input(&stage.harness, source.as_bytes())
            .expect("independent fixture prerequisite");
        let executable =
            stage.capability_file_path("invocation.elf").expect("independent fixture prerequisite");
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
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        if wrap {
            args.extend(["-Wl,--wrap=malloc".into(), "-Wl,--wrap=free".into()]);
        }
        args.extend([
            OsString::from("-o"),
            executable.as_os_str().to_owned(),
            stage
                .capability_file_path("invocation.c")
                .expect("independent fixture prerequisite")
                .into_os_string(),
            stage
                .capability_file_path("program.o")
                .expect("independent fixture prerequisite")
                .into_os_string(),
            stage
                .capability_file_path("runtime.o")
                .expect("independent fixture prerequisite")
                .into_os_string(),
        ]);
        let linked = compile(&stage, &args);
        assert_eq!(
            linked.status.success(),
            link_success,
            "{}",
            String::from_utf8_lossy(&linked.stderr)
        );
        if !link_success {
            assert!(!linked.stderr.is_empty());
            assert!(stage.cleanup().is_empty());
            self.empty();
            return linked.status;
        }
        assert!(linked.stdout.is_empty() && linked.stderr.is_empty());
        native::audit_staged_executable(&executable, "zryna_c_v0_i_dispatch")
            .expect("independent fixture prerequisite");
        native::prepare_executable_mode(&executable).expect("independent fixture prerequisite");
        stage.revalidate().expect("independent fixture prerequisite");
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
        .expect("independent fixture prerequisite");
        assert!(stage.cleanup().is_empty());
        self.empty();
        assert!(output.stdout.is_empty());
        if output.status.success() {
            assert!(output.stderr.is_empty());
        }
        output.status
    }
}
pub(super) fn compile(stage: &NativeStage, args: &[OsString]) -> native::BoundedProcessOutput {
    let limits = NativeProcessLimits::default();
    let tools =
        native::discover_linux_native_toolchain(limits).expect("independent fixture prerequisite");
    native::revalidate_tool(&tools.driver, &tools.driver_identity)
        .expect("independent fixture prerequisite");
    native::revalidate_tool(&tools.linker, &tools.linker_identity)
        .expect("independent fixture prerequisite");
    stage.revalidate().expect("independent fixture prerequisite");
    let directory = stage.capability_directory_path();
    let output = native::run_bounded_process(
        &tools.driver,
        args,
        &directory,
        limits.link_timeout(),
        limits.tool_output_bytes(),
        limits.tool_output_bytes(),
        ProcessPhase::Link,
        Some(&directory),
    )
    .expect("independent fixture prerequisite");
    stage.revalidate().expect("independent fixture prerequisite");
    output
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("independent fixture prerequisite");
    }
}
