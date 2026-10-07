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

#[test]
fn linked_observation_retains_real_scalar_objects_and_never_runs_client() {
    let fixture = Fixture::new();
    let requirements = linked_requirements(&capture::reference(), "imported");
    let foreign = fixture.compile_observed(&library_source()).expect("real observed C compilation");
    let library = accept(&requirements, foreign.bytes(), &["free", "malloc"]);
    let marker = fixture.acquired_path();
    let client = format!(
        "#include <stdio.h>\nint main(void) {{ FILE *f=fopen(\"{}\",\"w\"); if(f)fclose(f); abort(); }}",
        marker.display()
    );
    let observation = fixture
        .observe_link(&requirements, &library, &foreign, &client)
        .expect("compile/link only");
    fixture.empty();
    assert!(!marker.exists(), "client side effect proves the observer did not run it");
    observation.check_elf_rejection_controls();
    foreign.check_failure_retention_controls(
        &fixture.observation_root().expect("independent observed fixture prerequisite"),
    );
    fixture.empty();
    let report = observation.report();
    assert_eq!(report["execution_authorized"], false);
    assert_eq!(report["target_executed"], false);
    assert_eq!(
        report["missing_prerequisites"]
            .as_array()
            .expect("independent observed fixture prerequisite")
            .len(),
        3
    );
    assert!(
        report["observed_linker_order"]
            .as_array()
            .expect("independent observed fixture prerequisite")
            .len()
            > 3
    );
    let directory = std::env::var_os("ZRYNA_LINKED_OUTPUT_EVIDENCE_DIR").map_or_else(
        || fixture.0.join("observed-scalar"),
        |root| PathBuf::from(root).join("scalar"),
    );
    observation.export(&directory).expect("create-only actual artifacts after stage cleanup");
    assert!(observation.export(&directory).is_err(), "no evidence overwrite");
    eprintln!("417-linked-output-observation {report}");
}

#[test]
fn linked_observation_reissued_authority_rejects_before_new_compile_or_link() {
    let fixture = Fixture::new();
    let requirements = linked_requirements(&capture::reference(), "imported");
    let foreign = fixture
        .compile_observed(&library_source())
        .expect("independent observed fixture prerequisite");
    let library = accept(&requirements, foreign.bytes(), &["free", "malloc"]);
    let other = linked_requirements(&capture::reference(), "imported");
    assert_eq!(requirements.object_sha256(), other.object_sha256());
    let failure = fixture
        .observe_link(&other, &library, &foreign, "int main(void){return 0;}")
        .expect_err("expected observational rejection");
    assert!(failure.invocations.is_empty());
    assert_eq!(failure.diagnostics[0].code(), "ZRYNA-C4102");
    fixture.empty();
}

#[test]
fn linked_observation_real_compile_and_link_failures_cleanup_then_retry() {
    let fixture = Fixture::new();
    let failure =
        fixture.compile_observed("not valid C").expect_err("expected observational rejection");
    assert_eq!(failure.invocations.len(), 1);
    fixture.empty();
    let requirements = linked_requirements(&capture::reference(), "imported");
    let foreign = fixture
        .compile_observed(&library_source())
        .expect("independent observed fixture prerequisite");
    let library = accept(&requirements, foreign.bytes(), &["free", "malloc"]);
    let failure = fixture
        .observe_link(
            &requirements,
            &library,
            &foreign,
            "extern void missing(void); int main(void){missing();return 0;}",
        )
        .expect_err("expected observational rejection");
    let report = failure.report();
    assert_eq!(report["invocations"][0]["success"], false);
    assert!(
        !report["invocations"][0]["stderr_bytes"]
            .as_array()
            .expect("independent observed fixture prerequisite")
            .is_empty()
    );
    fixture.empty();
    fixture
        .observe_link(&requirements, &library, &foreign, "int main(void){return 0;}")
        .expect("independent observed fixture prerequisite");
    fixture.empty();
    eprintln!("417-linked-output-failure {report}");
}
