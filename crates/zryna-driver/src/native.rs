//! Linux x86-64 native object emission, sealed invocation linking, and bounded execution.

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod link;
/// Internal total-scalar native C linking; resource-bearing entries remain separate.
pub mod native_c_v0;
pub(crate) mod ownership;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use link::link_and_audit_native_invocation;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod stage_input;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod stage_support;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use stage_support::{
    NativeStageIdentity, native_stage_error, native_stage_identity, stage_cleanup_warning,
};

pub use ownership::{
    DataOwnershipExecutableIdentity, PreparedDataOwnershipExecutable,
    prepare_data_ownership_executable, publish_data_ownership_executable,
    publish_data_ownership_object,
};

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
#[path = "native_process_diagnostics.rs"]
mod process_diagnostics;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
macro_rules! process_io_failure {
    ($context:ident, $operation:ident, $error:expr) => {{
        let error: Option<&io::Error> = $error;
        #[cfg(test)]
        $context.record(process_diagnostics::Operation::$operation, error);
        #[cfg(not(test))]
        let _ = error;
        process_io_error()
    }};
}

use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), test))]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::{
    ffi::OsString,
    io::Read,
    process::{ExitStatus, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError},
    thread,
    time::{Instant, SystemTime},
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use nix::{
    errno::Errno,
    sys::signal::{self, Signal},
    unistd::Pid,
};
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use object::{BinaryFormat, Endianness, Object, ObjectKind, ObjectSection, ObjectSymbol};
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use process_wrap::std::{ChildWrapper, CommandWrap, ProcessGroup};

use zryna_backend_native::control_flow_v1::ValidatedControlFlowNativeObjectArtifact;
use zryna_backend_native::{
    LinuxX8664ObjectTarget, ValidatedNativeObjectArtifact, select_object_target,
};
use zryna_diagnostics::Diagnostic;
use zryna_frontend::VerifiedFrontendProvider;
use zryna_ir::VerifiedProgram;
use zryna_source::SourceMap;

use crate::{
    SourceToIrError, compile_to_verified_ir,
    javascript::{
        ArtifactOutputRoot, MAX_ARTIFACT_STEM_BYTES, destination_exists_error,
        publish_complete_artifact, validate_artifact_stem,
    },
};

/// File extension used for Linux x86-64 ELF relocatable objects.
pub const NATIVE_OBJECT_ARTIFACT_EXTENSION: &str = "o";
/// Validated capability for the workspace's declared `.zryna/out` output root.
pub type NativeObjectOutputRoot = ArtifactOutputRoot;
/// Maximum portable artifact stem bytes accepted by the native object publisher.
pub const MAX_NATIVE_OBJECT_ARTIFACT_STEM_BYTES: usize = MAX_ARTIFACT_STEM_BYTES;
/// File extension used for audited Linux x86-64 invocation executables.
pub const NATIVE_EXECUTABLE_ARTIFACT_EXTENSION: &str = "elf";
/// Maximum accepted executable snapshot size.
pub const MAX_NATIVE_EXECUTABLE_BYTES: usize = 32 * 1_024 * 1_024;
/// Maximum captured bytes from either compiler-driver output stream.
pub const MAX_NATIVE_TOOL_OUTPUT_BYTES: usize = 64 * 1_024;
/// Maximum captured bytes from the invocation executable's stderr.
pub const MAX_NATIVE_RUN_STDERR_BYTES: usize = 16 * 1_024;
/// Maximum compiler-driver probe duration.
pub const MAX_NATIVE_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// Maximum compile-and-link duration.
pub const MAX_NATIVE_LINK_TIMEOUT: Duration = Duration::from_secs(30);
/// Maximum invocation-executable duration.
pub const MAX_NATIVE_RUN_TIMEOUT: Duration = Duration::from_secs(5);

const NATIVE_TOOLCHAIN_DRIVER: &str = "/usr/bin/gcc";
const MIN_NATIVE_PROCESS_TIMEOUT: Duration = Duration::from_millis(100);
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const NATIVE_PROCESS_CLEANUP_RESERVE: Duration = Duration::from_secs(5);
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MAX_STAGE_NAME_ATTEMPTS: u64 = 64;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MIN_SUPPORTED_GCC_MAJOR: u32 = 12;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MAX_SUPPORTED_GCC_MAJOR: u32 = 15;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MIN_SUPPORTED_GNU_LD_MINOR: u32 = 38;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MAX_SUPPORTED_GNU_LD_MINOR: u32 = 46;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const ELF_SECTION_FLAG_WRITE: u64 = 0x1;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const ELF_SECTION_FLAG_EXECUTE: u64 = 0x4;
#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), test))]
static NEXT_NATIVE_STAGE: AtomicU64 = AtomicU64::new(0);

/// One audited native object atomically published at a new destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedNativeObjectArtifact {
    path: PathBuf,
    diagnostics: Vec<Diagnostic>,
}

impl PublishedNativeObjectArtifact {
    /// Returns the exact published object path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns non-fatal publication diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Successful source-to-native-object compilation.
#[derive(Clone, Debug)]
pub struct NativeObjectBuildSuccess {
    artifact: PublishedNativeObjectArtifact,
    diagnostics: Vec<Diagnostic>,
}

impl NativeObjectBuildSuccess {
    /// Returns the atomically published audited object.
    #[must_use]
    pub const fn artifact(&self) -> &PublishedNativeObjectArtifact {
        &self.artifact
    }

    /// Returns deterministic non-fatal frontend and publication diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Failure before a new native object can be reported.
#[derive(Debug)]
pub enum NativeObjectBuildError {
    /// Source analysis, semantics, or IR verification failed.
    Source(SourceToIrError),
    /// Native MIR lowering rejected the verified program.
    Mir(Vec<Diagnostic>),
    /// Target selection or native object emission failed.
    Backend(Diagnostic),
    /// The complete object could not be published atomically.
    Publication(Diagnostic),
}

impl NativeObjectBuildError {
    /// Returns deterministic compiler diagnostics carried by this failure.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::Source(error) => error.diagnostics(),
            Self::Mir(diagnostics) => diagnostics,
            Self::Backend(diagnostic) | Self::Publication(diagnostic) => {
                std::slice::from_ref(diagnostic)
            }
        }
    }
}

impl fmt::Display for NativeObjectBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => error.fmt(formatter),
            Self::Mir(diagnostics) => {
                write!(
                    formatter,
                    "native MIR lowering failed with {} diagnostic(s)",
                    diagnostics.len()
                )
            }
            Self::Backend(diagnostic) => {
                write!(formatter, "native object emission failed: {diagnostic}")
            }
            Self::Publication(diagnostic) => {
                write!(formatter, "native object publication failed: {diagnostic}")
            }
        }
    }
}

impl Error for NativeObjectBuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Mir(_) | Self::Backend(_) | Self::Publication(_) => None,
        }
    }
}

/// Compiles real source through verified IR and native MIR to one audited ELF object.
///
/// Target selection happens before code generation and publication is create-only. No product
/// linker or generated executable is invoked.
///
/// # Errors
///
/// Returns a phase-specific failure and leaves no new destination on failure.
pub fn compile_native_object<Provider: VerifiedFrontendProvider + ?Sized>(
    frontend: &Provider,
    sources: &SourceMap,
    output_root: &NativeObjectOutputRoot,
    artifact_stem: &str,
    requested_target: &str,
) -> Result<NativeObjectBuildSuccess, NativeObjectBuildError> {
    let target = select_object_target(requested_target).map_err(NativeObjectBuildError::Backend)?;
    let compiled =
        compile_to_verified_ir(frontend, sources).map_err(NativeObjectBuildError::Source)?;
    let mir = zryna_native_mir::lower(compiled.program()).map_err(NativeObjectBuildError::Mir)?;
    let artifact =
        zryna_backend_native::emit_object(&mir, target).map_err(NativeObjectBuildError::Backend)?;
    let published = publish_native_object(&artifact, output_root, artifact_stem)
        .map_err(NativeObjectBuildError::Publication)?;
    let mut diagnostics = compiled.diagnostics().to_vec();
    diagnostics.extend_from_slice(published.diagnostics());
    Ok(NativeObjectBuildSuccess { artifact: published, diagnostics })
}

/// Atomically publishes one sealed, audited native object at an absent destination.
///
/// # Errors
///
/// Returns a stable diagnostic without creating or modifying the destination on failure.
pub fn publish_native_object(
    artifact: &ValidatedNativeObjectArtifact,
    output_root: &NativeObjectOutputRoot,
    artifact_stem: &str,
) -> Result<PublishedNativeObjectArtifact, Diagnostic> {
    let published = publish_complete_artifact(
        artifact.bytes(),
        output_root,
        artifact_stem,
        NATIVE_OBJECT_ARTIFACT_EXTENSION,
        "native object",
    )?;
    Ok(PublishedNativeObjectArtifact { path: published.path, diagnostics: published.diagnostics })
}

/// Selects the exact native target for callers that need to validate configuration first.
///
/// # Errors
///
/// Returns the backend's stable unsupported-target diagnostic.
pub fn select_native_object_target(
    requested_target: &str,
) -> Result<LinuxX8664ObjectTarget, Diagnostic> {
    select_object_target(requested_target)
}

/// Hard-bounded process limits for native tool probes, linking, and one invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeProcessLimits {
    probe_timeout: Duration,
    link_timeout: Duration,
    run_timeout: Duration,
    tool_output_bytes: usize,
    run_stderr_bytes: usize,
}

impl NativeProcessLimits {
    /// Creates limits that may tighten, but never exceed, the compiler hard caps.
    ///
    /// # Errors
    ///
    /// Rejects a duration below the cleanup-safe minimum, a duration above its hard cap, or a
    /// zero/oversized stream budget.
    pub fn new(
        probe_timeout: Duration,
        link_timeout: Duration,
        run_timeout: Duration,
        tool_output_bytes: usize,
        run_stderr_bytes: usize,
    ) -> Result<Self, Diagnostic> {
        if probe_timeout < MIN_NATIVE_PROCESS_TIMEOUT
            || probe_timeout > MAX_NATIVE_PROBE_TIMEOUT
            || link_timeout < MIN_NATIVE_PROCESS_TIMEOUT
            || link_timeout > MAX_NATIVE_LINK_TIMEOUT
            || run_timeout < MIN_NATIVE_PROCESS_TIMEOUT
            || run_timeout > MAX_NATIVE_RUN_TIMEOUT
            || tool_output_bytes == 0
            || tool_output_bytes > MAX_NATIVE_TOOL_OUTPUT_BYTES
            || run_stderr_bytes == 0
            || run_stderr_bytes > MAX_NATIVE_RUN_STDERR_BYTES
        {
            return Err(native_error(
                "ZRYNA-N4001",
                "native process limits are outside the supported bounds",
                "use nonzero stream budgets and durations within the documented native hard caps",
            ));
        }
        Ok(Self { probe_timeout, link_timeout, run_timeout, tool_output_bytes, run_stderr_bytes })
    }

    /// Returns the compiler-driver probe deadline.
    #[must_use]
    pub const fn probe_timeout(self) -> Duration {
        self.probe_timeout
    }

    /// Returns the compile-and-link deadline.
    #[must_use]
    pub const fn link_timeout(self) -> Duration {
        self.link_timeout
    }

    /// Returns the invocation deadline.
    #[must_use]
    pub const fn run_timeout(self) -> Duration {
        self.run_timeout
    }

    /// Returns the per-stream compiler-driver byte budget.
    #[must_use]
    pub const fn tool_output_bytes(self) -> usize {
        self.tool_output_bytes
    }

    /// Returns the invocation stderr byte budget.
    #[must_use]
    pub const fn run_stderr_bytes(self) -> usize {
        self.run_stderr_bytes
    }
}

impl Default for NativeProcessLimits {
    fn default() -> Self {
        Self {
            probe_timeout: MAX_NATIVE_PROBE_TIMEOUT,
            link_timeout: MAX_NATIVE_LINK_TIMEOUT,
            run_timeout: MAX_NATIVE_RUN_TIMEOUT,
            tool_output_bytes: MAX_NATIVE_TOOL_OUTPUT_BYTES,
            run_stderr_bytes: MAX_NATIVE_RUN_STDERR_BYTES,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
struct ToolFileIdentity {
    device: u64,
    inode: u64,
    length: u64,
    modified: SystemTime,
}

/// Opaque capability for the documented Linux x86-64 GNU compiler-driver and linker pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinuxX8664LinkToolchain {
    driver: PathBuf,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    driver_identity: ToolFileIdentity,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    linker: PathBuf,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    linker_identity: ToolFileIdentity,
    gcc_version: Box<str>,
    linker_version: Box<str>,
}

impl LinuxX8664LinkToolchain {
    /// Returns the canonical compiler-driver path retained by this capability.
    #[must_use]
    pub fn driver(&self) -> &Path {
        &self.driver
    }

    /// Returns the validated GCC version text.
    #[must_use]
    pub fn gcc_version(&self) -> &str {
        &self.gcc_version
    }

    /// Returns the validated GNU linker version text.
    #[must_use]
    pub fn linker_version(&self) -> &str {
        &self.linker_version
    }
}

/// One audited native invocation executable published create-only.
#[derive(Clone, Debug)]
pub struct PublishedNativeExecutableArtifact {
    path: PathBuf,
    diagnostics: Vec<Diagnostic>,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output_root: ArtifactOutputRoot,
    prepared: PreparedNativeExecutable,
}

#[derive(Clone)]
pub(crate) struct PreparedNativeExecutable {
    bytes: Arc<[u8]>,
    result_type: zryna_abi::ScalarType,
    expected_symbol: Box<str>,
    ownership_identity: Option<DataOwnershipExecutableIdentity>,
    diagnostics: Vec<Diagnostic>,
}

impl PreparedNativeExecutable {
    #[must_use]
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub(crate) const fn result_type(&self) -> zryna_abi::ScalarType {
        self.result_type
    }

    #[must_use]
    pub(crate) fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub(crate) const fn ownership_identity(&self) -> Option<&DataOwnershipExecutableIdentity> {
        self.ownership_identity.as_ref()
    }
}

impl fmt::Debug for PreparedNativeExecutable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedNativeExecutable")
            .field("bytes", &self.bytes.len())
            .field("result_type", &self.result_type)
            .field("expected_symbol", &self.expected_symbol)
            .field("ownership_identity", &self.ownership_identity)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

impl PublishedNativeExecutableArtifact {
    /// Returns the exact published executable path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the verified result type carried by the four-byte output channel.
    #[must_use]
    pub const fn result_type(&self) -> zryna_abi::ScalarType {
        self.prepared.result_type()
    }

    /// Returns non-fatal publication or cleanup diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns the sealed `DataOwnershipV1` identity when this is an ownership executable.
    #[must_use]
    pub const fn data_ownership_identity(&self) -> Option<&DataOwnershipExecutableIdentity> {
        self.prepared.ownership_identity()
    }
}

/// Successful source-to-native-invocation executable compilation.
#[derive(Clone, Debug)]
pub struct NativeExecutableBuildSuccess {
    artifact: PublishedNativeExecutableArtifact,
    diagnostics: Vec<Diagnostic>,
}

impl NativeExecutableBuildSuccess {
    /// Returns the complete audited executable.
    #[must_use]
    pub const fn artifact(&self) -> &PublishedNativeExecutableArtifact {
        &self.artifact
    }

    /// Returns deterministic non-fatal frontend, publication, and cleanup diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Failure before a new native invocation executable can be reported.
#[derive(Debug)]
pub enum NativeExecutableBuildError {
    /// Source analysis, semantics, or IR verification failed.
    Source(SourceToIrError),
    /// One or more bounded native diagnostics rejected the operation.
    Native(Vec<Diagnostic>),
}

impl NativeExecutableBuildError {
    /// Returns deterministic diagnostics carried by this failure.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::Source(error) => error.diagnostics(),
            Self::Native(diagnostics) => diagnostics,
        }
    }
}

impl fmt::Display for NativeExecutableBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => error.fmt(formatter),
            Self::Native(diagnostics) => write!(
                formatter,
                "native executable build failed with {} diagnostic(s)",
                diagnostics.len()
            ),
        }
    }
}

impl Error for NativeExecutableBuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Native(_) => None,
        }
    }
}

/// Failure while invoking one already-published native executable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeRunError {
    diagnostic: Diagnostic,
}

impl NativeRunError {
    /// Returns the stable native runtime diagnostic.
    #[must_use]
    pub const fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }
}

impl fmt::Display for NativeRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native invocation failed: {}", self.diagnostic)
    }
}

impl Error for NativeRunError {}

/// Discovers and validates the one documented system GNU compiler-driver/linker capability.
///
/// Discovery ignores `PATH`, `CC`, `LD`, and compiler flag environment variables. On Linux
/// x86-64 it validates the canonical `/usr/bin/gcc` target, GCC version policy, subordinate GNU
/// linker path, and linker version through the same bounded process boundary used for linking.
///
/// # Errors
///
/// Returns a stable diagnostic on unsupported hosts, missing/replaced tools, probe failure,
/// timeout, output overflow, wrong target, or unsupported versions.
pub fn discover_linux_native_toolchain(
    limits: NativeProcessLimits,
) -> Result<LinuxX8664LinkToolchain, Diagnostic> {
    discover_linux_native_toolchain_at(Path::new(NATIVE_TOOLCHAIN_DRIVER), limits)
}

/// Compiles one real source program into one audited, create-only Linux invocation executable.
///
/// The typed invocation is verified by the scalar ABI authority embedded in `VerifiedProgram`.
/// Its exact values are baked into a deterministic C11 harness which writes only the four raw
/// little-endian result bytes. The public executable is created only after linking, independent
/// ELF audit, synchronization, and executable-mode assignment all succeed.
///
/// # Errors
///
/// Returns source or phase-specific native diagnostics. No new public executable is reported on
/// failure; an operation-owned private stage is removed unless cleanup itself fails explicitly.
#[allow(clippy::too_many_arguments)]
pub fn compile_native_invocation<Provider: VerifiedFrontendProvider + ?Sized>(
    frontend: &Provider,
    sources: &SourceMap,
    output_root: &ArtifactOutputRoot,
    artifact_stem: &str,
    requested_target: &str,
    toolchain: &LinuxX8664LinkToolchain,
    invocation: zryna_abi::Invocation,
    limits: NativeProcessLimits,
) -> Result<NativeExecutableBuildSuccess, NativeExecutableBuildError> {
    ensure_linux_x86_64_host().map_err(native_build_error)?;
    let target = select_object_target(requested_target)
        .map_err(|diagnostic| NativeExecutableBuildError::Native(vec![diagnostic]))?;
    validate_artifact_stem(artifact_stem).map_err(native_build_error)?;
    output_root.revalidate().map_err(native_build_error)?;
    let destination =
        output_root.path().join(format!("{artifact_stem}.{NATIVE_EXECUTABLE_ARTIFACT_EXTENSION}"));
    ensure_destination_absent(&destination, artifact_stem).map_err(native_build_error)?;

    let compiled =
        compile_to_verified_ir(frontend, sources).map_err(NativeExecutableBuildError::Source)?;
    let invocation = compiled
        .program()
        .prepare_invocation(invocation)
        .map_err(|error| native_build_error(invocation_error(error)))?;
    let mir =
        zryna_native_mir::lower(compiled.program()).map_err(NativeExecutableBuildError::Native)?;
    let object = zryna_backend_native::emit_object(&mir, target)
        .map_err(|diagnostic| NativeExecutableBuildError::Native(vec![diagnostic]))?;

    let prepared = prepare_native_invocation_from_verified(
        compiled.program(),
        &object,
        &invocation,
        output_root,
        toolchain,
        limits,
    )
    .map_err(NativeExecutableBuildError::Native)?;
    let mut published = publish_prepared_native_invocation(&prepared, output_root, artifact_stem)
        .map_err(NativeExecutableBuildError::Native)?;
    let mut diagnostics = compiled.diagnostics().to_vec();
    diagnostics.extend_from_slice(published.diagnostics());
    published.diagnostics = diagnostics.clone();
    Ok(NativeExecutableBuildSuccess { artifact: published, diagnostics })
}

/// Runs one published invocation executable and returns a typed scalar outcome.
///
/// Process exit status reports harness health only. A successful process must write exactly four
/// bytes and no stderr; the bytes are decoded as one little-endian native `i32` carrier and
/// normalized through scalar ABI v1.
///
/// # Errors
///
/// Returns a stable process, deadline, output, abnormal-exit, or framing failure.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn run_native_invocation(
    executable: &PublishedNativeExecutableArtifact,
    limits: NativeProcessLimits,
) -> Result<zryna_abi::ScalarOutcome, NativeRunError> {
    ownership::observation::run_published(executable, limits)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) fn run_prepared_native_invocation(
    executable: &PreparedNativeExecutable,
    output_root: &ArtifactOutputRoot,
    limits: NativeProcessLimits,
) -> Result<zryna_abi::ScalarOutcome, NativeRunError> {
    ensure_linux_x86_64_host().map_err(native_run_error)?;
    let stage = NativeStage::create(output_root, "run").map_err(native_run_error)?;
    let operation = (|| {
        stage.write_input(&stage.executable, executable.bytes())?;
        let executable_path = stage.capability_file_path("invocation.elf")?;
        let directory_path = stage.capability_directory_path();
        prepare_executable_mode(&executable_path)?;
        stage.revalidate()?;
        run_bounded_process(
            &executable_path,
            &[],
            &directory_path,
            limits.run_timeout(),
            5,
            limits.run_stderr_bytes(),
            ProcessPhase::Run,
            Some(&directory_path),
        )
    })();
    let cleanup = stage.cleanup();
    if let Some(diagnostic) = cleanup.into_iter().next() {
        return Err(native_run_error(diagnostic));
    }
    let output = operation.map_err(native_run_error)?;
    interpret_native_invocation_output(&output, executable.result_type())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn interpret_native_invocation_output(
    output: &BoundedProcessOutput,
    result_type: zryna_abi::ScalarType,
) -> Result<zryna_abi::ScalarOutcome, NativeRunError> {
    if !output.status.success() {
        return Err(native_run_error(native_error(
            "ZRYNA-N4021",
            "native invocation executable exited abnormally",
            "rebuild the executable and report the smallest reproducible source and invocation",
        )));
    }
    if output.stdout.len() != 4 || !output.stderr.is_empty() {
        return Err(native_run_error(native_error(
            "ZRYNA-N4022",
            "native invocation returned an invalid result frame",
            "rebuild the executable and report the smallest reproducible source and invocation",
        )));
    }
    let raw = i32::from_le_bytes(output.stdout.as_slice().try_into().map_err(|_| {
        native_run_error(native_error(
            "ZRYNA-N4022",
            "native invocation returned an invalid result frame",
            "rebuild the executable and report the smallest reproducible source and invocation",
        ))
    })?);
    match zryna_abi::normalize_result(
        zryna_abi::ScalarTarget::NativeLinuxX8664,
        result_type,
        zryna_abi::RawHostScalar::I32(raw),
    ) {
        Ok(value) => Ok(zryna_abi::ScalarOutcome::Returned { value }),
        Err(_) => Ok(zryna_abi::ScalarOutcome::HostError {
            code: zryna_abi::ScalarHostErrorCode::InvalidTargetResult,
        }),
    }
}

/// Rejects Linux invocation executables before staging on unsupported hosts.
///
/// # Errors
///
/// Always returns the stable unsupported-host diagnostic.
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub fn run_native_invocation(
    _executable: &PublishedNativeExecutableArtifact,
    _limits: NativeProcessLimits,
) -> Result<zryna_abi::ScalarOutcome, NativeRunError> {
    Err(native_run_error(native_error(
        "ZRYNA-N4002",
        "native linking and invocation require a Linux x86-64 host",
        "run this operation on Linux x86-64; other native hosts are not implemented",
    )))
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn run_prepared_native_invocation(
    _executable: &PreparedNativeExecutable,
    _output_root: &ArtifactOutputRoot,
    _limits: NativeProcessLimits,
) -> Result<zryna_abi::ScalarOutcome, NativeRunError> {
    Err(native_run_error(native_error(
        "ZRYNA-N4002",
        "native linking and invocation require a Linux x86-64 host",
        "run this operation on Linux x86-64; other native hosts are not implemented",
    )))
}

fn native_build_error(diagnostic: Diagnostic) -> NativeExecutableBuildError {
    NativeExecutableBuildError::Native(vec![diagnostic])
}

fn native_run_error(diagnostic: Diagnostic) -> NativeRunError {
    NativeRunError { diagnostic }
}

fn invocation_error(error: zryna_abi::InvocationError) -> Diagnostic {
    Diagnostic::error(
        error.code(),
        None,
        "native invocation does not match the verified scalar ABI export",
        "use one exact logical export with the verified arity and scalar argument types",
    )
}

fn native_error(code: &'static str, message: &'static str, guidance: &'static str) -> Diagnostic {
    Diagnostic::error(code, None, message, guidance)
}

fn ensure_linux_x86_64_host() -> Result<(), Diagnostic> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Ok(())
    } else {
        Err(native_error(
            "ZRYNA-N4002",
            "native linking and invocation require a Linux x86-64 host",
            "run this operation on Linux x86-64; other native hosts are not implemented",
        ))
    }
}

fn ensure_destination_absent(destination: &Path, artifact_stem: &str) -> Result<(), Diagnostic> {
    match fs::symlink_metadata(destination) {
        Ok(_) => Err(destination_exists_error(
            artifact_stem,
            NATIVE_EXECUTABLE_ARTIFACT_EXTENSION,
            "native executable",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(native_error(
            "ZRYNA-N4014",
            "native executable destination could not be inspected",
            "use a writable declared output root with no link-like components",
        )),
    }
}

const MAX_NATIVE_HARNESS_BYTES: usize = 128 * 1_024;

#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), test))]
fn render_invocation_harness(
    invocation: &zryna_abi::VerifiedInvocation<'_>,
) -> Result<Vec<u8>, Diagnostic> {
    use std::fmt::Write as _;

    let export = invocation.export();
    let mut source = String::from("#include <stdint.h>\n#include <stdio.h>\n");
    write!(source, "extern int32_t {}(", export.native_linux_x86_64_symbol().as_str())
        .map_err(|_| harness_error())?;
    if export.parameters().is_empty() {
        source.push_str("void");
    } else {
        for (index, _) in export.parameters().iter().enumerate() {
            if index != 0 {
                source.push_str(", ");
            }
            source.push_str("int32_t");
        }
    }
    source.push_str(");\nint main(void) {\n  int32_t result = ");
    source.push_str(export.native_linux_x86_64_symbol().as_str());
    source.push('(');
    for (index, value) in invocation.arguments().iter().copied().enumerate() {
        if index != 0 {
            source.push_str(", ");
        }
        let zryna_abi::RawHostScalar::I32(value) =
            zryna_abi::encode_argument(zryna_abi::ScalarTarget::NativeLinuxX8664, value)
        else {
            return Err(harness_error());
        };
        if value == i32::MIN {
            source.push_str("INT32_MIN");
        } else {
            write!(source, "INT32_C({value})").map_err(|_| harness_error())?;
        }
    }
    source.push_str(
        ");\n  uint32_t bits = (uint32_t)result;\n  unsigned char output[4] = {\n    (unsigned char)(bits & UINT32_C(255)),\n    (unsigned char)((bits >> 8) & UINT32_C(255)),\n    (unsigned char)((bits >> 16) & UINT32_C(255)),\n    (unsigned char)((bits >> 24) & UINT32_C(255))\n  };\n  if (fwrite(output, 1, sizeof(output), stdout) != sizeof(output)) return 70;\n  if (fflush(stdout) != 0) return 71;\n  return 0;\n}\n",
    );
    if source.len() > MAX_NATIVE_HARNESS_BYTES {
        return Err(native_error(
            "ZRYNA-N4010",
            "native invocation harness exceeds its fixed byte budget",
            "use an invocation with fewer scalar arguments",
        ));
    }
    Ok(source.into_bytes())
}

#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), test))]
fn harness_error() -> Diagnostic {
    native_error(
        "ZRYNA-N4010",
        "native invocation harness generation failed",
        "report this compiler invariant failure with the smallest reproducible invocation",
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[derive(Clone, Copy, Debug)]
enum ProcessPhase {
    Probe,
    Link,
    Run,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl ProcessPhase {
    const fn timeout_message(self) -> &'static str {
        match self {
            Self::Probe => "native tool probe exceeded its execution deadline",
            Self::Link => "native compile-and-link exceeded its execution deadline",
            Self::Run => "native invocation exceeded its execution deadline",
        }
    }

    const fn output_message(self) -> &'static str {
        match self {
            Self::Probe => "native tool probe exceeded its output budget",
            Self::Link => "native compile-and-link exceeded its output budget",
            Self::Run => "native invocation exceeded its output budget",
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[derive(Debug)]
struct BoundedProcessOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
struct PendingProcessOutput {
    stdout: Receiver<Result<CapturedStream, Diagnostic>>,
    stderr: Receiver<Result<CapturedStream, Diagnostic>>,
    timed_out: bool,
    output_exceeded: bool,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn discover_linux_native_toolchain_at(
    driver: &Path,
    limits: NativeProcessLimits,
) -> Result<LinuxX8664LinkToolchain, Diagnostic> {
    let (driver, driver_identity) = canonical_tool(driver).map_err(|()| {
        native_error(
            "ZRYNA-N4003",
            "the documented GNU compiler driver is unavailable",
            "install a supported GNU compiler toolchain at /usr/bin/gcc on Linux x86-64",
        )
    })?;
    let working_directory = driver.parent().ok_or_else(|| {
        native_error(
            "ZRYNA-N4004",
            "the GNU compiler driver path is unsupported",
            "install the documented compiler driver under an absolute system directory",
        )
    })?;

    let target =
        probe_utf8_line(&driver, &[OsString::from("-dumpmachine")], working_directory, limits)?;
    if target != "x86_64-linux-gnu" {
        return Err(native_error(
            "ZRYNA-N4004",
            "the GNU compiler driver targets an unsupported platform",
            "use a native x86_64-linux-gnu GCC installation",
        ));
    }

    let gcc_version = probe_utf8_line(
        &driver,
        &[OsString::from("-dumpfullversion"), OsString::from("-dumpversion")],
        working_directory,
        limits,
    )?;
    let gcc_major = parse_major_version(&gcc_version).ok_or_else(unsupported_toolchain)?;
    if !(MIN_SUPPORTED_GCC_MAJOR..=MAX_SUPPORTED_GCC_MAJOR).contains(&gcc_major) {
        return Err(unsupported_toolchain());
    }

    let linker_text = probe_utf8_line(
        &driver,
        &[OsString::from("-print-prog-name=ld")],
        working_directory,
        limits,
    )?;
    let linker_path = resolve_reported_linker(working_directory, &linker_text)
        .ok_or_else(unsupported_toolchain)?;
    let (linker, linker_identity) =
        canonical_tool(&linker_path).map_err(|()| unsupported_toolchain())?;
    let linker_output = run_bounded_process(
        &linker,
        &[OsString::from("--version")],
        linker.parent().ok_or_else(unsupported_toolchain)?,
        limits.probe_timeout(),
        limits.tool_output_bytes(),
        limits.tool_output_bytes(),
        ProcessPhase::Probe,
        None,
    )?;
    if !linker_output.status.success() || !linker_output.stderr.is_empty() {
        return Err(unsupported_toolchain());
    }
    let linker_stdout =
        std::str::from_utf8(&linker_output.stdout).map_err(|_| unsupported_toolchain())?;
    let linker_first_line = linker_stdout.lines().next().ok_or_else(unsupported_toolchain)?;
    if !linker_first_line.starts_with("GNU ld ") && !linker_first_line.starts_with("GNU ld (") {
        return Err(unsupported_toolchain());
    }
    let linker_version =
        linker_first_line.split_ascii_whitespace().next_back().ok_or_else(unsupported_toolchain)?;
    let (ld_major, ld_minor) =
        parse_major_minor_version(linker_version).ok_or_else(unsupported_toolchain)?;
    if ld_major != 2
        || !(MIN_SUPPORTED_GNU_LD_MINOR..=MAX_SUPPORTED_GNU_LD_MINOR).contains(&ld_minor)
    {
        return Err(unsupported_toolchain());
    }

    Ok(LinuxX8664LinkToolchain {
        driver,
        driver_identity,
        linker,
        linker_identity,
        gcc_version: gcc_version.into_boxed_str(),
        linker_version: linker_version.to_owned().into_boxed_str(),
    })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn resolve_reported_linker(driver_directory: &Path, reported: &str) -> Option<PathBuf> {
    let reported = Path::new(reported);
    if reported.is_absolute() {
        return Some(reported.to_owned());
    }
    let mut components = reported.components();
    let std::path::Component::Normal(name) = components.next()? else {
        return None;
    };
    if components.next().is_some() {
        return None;
    }
    Some(driver_directory.join(name))
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn discover_linux_native_toolchain_at(
    _driver: &Path,
    _limits: NativeProcessLimits,
) -> Result<LinuxX8664LinkToolchain, Diagnostic> {
    Err(native_error(
        "ZRYNA-N4002",
        "native linking and invocation require a Linux x86-64 host",
        "run this operation on Linux x86-64; other native hosts are not implemented",
    ))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn unsupported_toolchain() -> Diagnostic {
    native_error(
        "ZRYNA-N4004",
        "the system GNU compiler-driver or linker identity is unsupported",
        "use x86_64-linux-gnu GCC 12 through 15 with GNU ld 2.38 through 2.46",
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn canonical_tool(path: &Path) -> Result<(PathBuf, ToolFileIdentity), ()> {
    use std::os::unix::fs::MetadataExt;

    if !path.is_absolute() {
        return Err(());
    }
    let canonical = fs::canonicalize(path).map_err(|_| ())?;
    if !canonical.is_absolute() {
        return Err(());
    }
    let metadata = fs::metadata(&canonical).map_err(|_| ())?;
    if !metadata.is_file() || metadata.mode() & 0o111 == 0 {
        return Err(());
    }
    let modified = metadata.modified().map_err(|_| ())?;
    Ok((
        canonical,
        ToolFileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            length: metadata.len(),
            modified,
        },
    ))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn revalidate_tool(path: &Path, expected: &ToolFileIdentity) -> Result<(), Diagnostic> {
    let (canonical, actual) = canonical_tool(path).map_err(|()| replaced_toolchain())?;
    if canonical != path || &actual != expected {
        return Err(replaced_toolchain());
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn replaced_toolchain() -> Diagnostic {
    native_error(
        "ZRYNA-N4005",
        "the validated native system toolchain changed before use",
        "rediscover the native toolchain capability and retry",
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn probe_utf8_line(
    program: &Path,
    arguments: &[OsString],
    working_directory: &Path,
    limits: NativeProcessLimits,
) -> Result<String, Diagnostic> {
    let output = run_bounded_process(
        program,
        arguments,
        working_directory,
        limits.probe_timeout(),
        limits.tool_output_bytes(),
        limits.tool_output_bytes(),
        ProcessPhase::Probe,
        None,
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(unsupported_toolchain());
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| unsupported_toolchain())?;
    let line = text.strip_suffix('\n').unwrap_or(text);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty() || line.contains(['\n', '\r', '\0']) {
        return Err(unsupported_toolchain());
    }
    Ok(line.to_owned())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn parse_major_version(value: &str) -> Option<u32> {
    value.split('.').next()?.parse().ok()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn parse_major_minor_version(value: &str) -> Option<(u32, u32)> {
    let mut parts = value.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
struct NativeStage {
    directory: PathBuf,
    object: PathBuf,
    harness: PathBuf,
    runtime_source: PathBuf,
    executable: PathBuf,
    directory_handle: cap_std::fs::Dir,
    directory_identity: NativeStageIdentity,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl NativeStage {
    fn create(output_root: &ArtifactOutputRoot, artifact_stem: &str) -> Result<Self, Diagnostic> {
        use std::os::unix::fs::DirBuilderExt;

        output_root.revalidate()?;
        for _ in 0..MAX_STAGE_NAME_ATTEMPTS {
            let sequence = NEXT_NATIVE_STAGE.fetch_add(1, Ordering::Relaxed);
            let directory = output_root
                .path()
                .join(format!(".zryna-link-{artifact_stem}-{}-{sequence}", std::process::id()));
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            match builder.create(&directory) {
                Ok(()) => {
                    let directory_handle = fs::File::open(&directory).map_err(|_| {
                        let _ = fs::remove_dir(&directory);
                        native_stage_error()
                    })?;
                    let directory_identity = native_stage_identity(
                        &directory_handle.metadata().map_err(|_| native_stage_error())?,
                    )?;
                    let directory_handle = cap_std::fs::Dir::from_std_file(directory_handle);
                    return Ok(Self {
                        object: directory.join("program.o"),
                        harness: directory.join("invocation.c"),
                        runtime_source: directory.join("runtime.c"),
                        executable: directory.join("invocation.elf"),
                        directory,
                        directory_handle,
                        directory_identity,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(_) => {
                    return Err(native_error(
                        "ZRYNA-N4015",
                        "native private staging directory could not be created",
                        "use a writable declared output root with no link-like components",
                    ));
                }
            }
        }
        Err(native_error(
            "ZRYNA-N4015",
            "native private staging name budget was exhausted",
            "remove stale .zryna-link staging directories and retry",
        ))
    }

    fn revalidate(&self) -> Result<(), Diagnostic> {
        use cap_std::fs::MetadataExt as _;
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt};

        let held = self.directory_handle.dir_metadata().map_err(|_| native_stage_error())?;
        let current = fs::symlink_metadata(&self.directory).map_err(|_| native_stage_error())?;
        if !held.is_dir()
            || !current.is_dir()
            || current.file_type().is_symlink()
            || held.dev() != self.directory_identity.device
            || held.ino() != self.directory_identity.inode
            || current.dev() != self.directory_identity.device
            || current.ino() != self.directory_identity.inode
            || current.permissions().mode() & 0o777 != 0o700
        {
            return Err(native_stage_error());
        }
        Ok(())
    }

    fn capability_directory_path(&self) -> PathBuf {
        use std::os::fd::AsRawFd as _;

        PathBuf::from(format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            self.directory_handle.as_raw_fd()
        ))
    }

    fn capability_file_path(&self, name: &str) -> Result<PathBuf, Diagnostic> {
        if !matches!(
            name,
            "program.o" | "invocation.c" | "runtime.c" | "runtime.o" | "invocation.elf"
        ) {
            return Err(native_stage_error());
        }
        Ok(self.capability_directory_path().join(name))
    }

    fn cleanup(&self) -> Vec<Diagnostic> {
        if self.revalidate().is_err() {
            return vec![stage_cleanup_warning()];
        }
        let mut failed = false;
        for name in ["invocation.elf", "invocation.c", "runtime.o", "runtime.c", "program.o"] {
            match self.directory_handle.remove_file(name) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => failed = true,
            }
        }
        match self.directory_handle.entries() {
            Ok(mut entries) => {
                if entries.next().is_some() || fs::remove_dir(&self.directory).is_err() {
                    failed = true;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => failed = true,
        }
        if failed { vec![stage_cleanup_warning()] } else { Vec::new() }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) fn prepare_native_invocation_from_verified(
    program: &VerifiedProgram,
    object: &ValidatedNativeObjectArtifact,
    invocation: &zryna_abi::VerifiedInvocation<'_>,
    output_root: &ArtifactOutputRoot,
    toolchain: &LinuxX8664LinkToolchain,
    limits: NativeProcessLimits,
) -> Result<PreparedNativeExecutable, Vec<Diagnostic>> {
    ensure_linux_x86_64_host().map_err(|error| vec![error])?;
    let invocation_export = invocation.export();
    let Some(program_export) = program.scalar_abi().exports().nth(invocation_export.index()) else {
        return Err(vec![harness_error()]);
    };
    if program_export.logical_name() != invocation_export.logical_name()
        || program_export.javascript_name() != invocation_export.javascript_name()
        || program_export.webassembly_name() != invocation_export.webassembly_name()
        || program_export.native_linux_x86_64_symbol()
            != invocation_export.native_linux_x86_64_symbol()
        || program_export.parameters() != invocation_export.parameters()
        || program_export.result() != invocation_export.result()
    {
        return Err(vec![harness_error()]);
    }
    let harness = render_invocation_harness(invocation).map_err(|error| vec![error])?;
    let expected_symbol = invocation_export.native_linux_x86_64_symbol().as_str();
    let (sealed_bytes, diagnostics) = link_and_audit_native_invocation(
        object.bytes(),
        &harness,
        expected_symbol,
        output_root,
        toolchain,
        limits,
    )?;
    if diagnostics.iter().any(|diagnostic| diagnostic.code() == "ZRYNA-N4016") {
        return Err(diagnostics);
    }
    Ok(PreparedNativeExecutable {
        bytes: Arc::from(sealed_bytes),
        result_type: invocation_export.result(),
        expected_symbol: Box::from(expected_symbol),
        ownership_identity: None,
        diagnostics,
    })
}

/// Links one already-audited internal M2 object for one artifact-bound typed invocation.
///
/// This remains an internal compiler boundary until the M2 profile and manifest are activated.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[allow(dead_code, reason = "internal M2 profile is exercised before its public activation")]
pub(crate) fn prepare_control_flow_native_invocation(
    object: &ValidatedControlFlowNativeObjectArtifact,
    invocation: zryna_abi::Invocation,
    output_root: &ArtifactOutputRoot,
    toolchain: &LinuxX8664LinkToolchain,
    limits: NativeProcessLimits,
) -> Result<PreparedNativeExecutable, Vec<Diagnostic>> {
    ensure_linux_x86_64_host().map_err(|error| vec![error])?;
    let invocation =
        object.prepare_invocation(invocation).map_err(|error| vec![invocation_error(error)])?;
    let harness = render_invocation_harness(&invocation).map_err(|error| vec![error])?;
    let export = invocation.export();
    let expected_symbol = export.native_linux_x86_64_symbol().as_str();
    let (sealed_bytes, diagnostics) = link_and_audit_native_invocation(
        object.bytes(),
        &harness,
        expected_symbol,
        output_root,
        toolchain,
        limits,
    )?;
    if diagnostics.iter().any(|diagnostic| diagnostic.code() == "ZRYNA-N4016") {
        return Err(diagnostics);
    }
    Ok(PreparedNativeExecutable {
        bytes: Arc::from(sealed_bytes),
        result_type: export.result(),
        expected_symbol: Box::from(expected_symbol),
        ownership_identity: None,
        diagnostics,
    })
}

/// Links one M2 object using the invocation already verified by the universal M2 program.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) fn prepare_control_flow_native_invocation_from_verified(
    program: &zryna_ir::control_flow_v1::VerifiedProgram,
    object: &ValidatedControlFlowNativeObjectArtifact,
    invocation: &zryna_abi::VerifiedInvocation<'_>,
    output_root: &ArtifactOutputRoot,
    toolchain: &LinuxX8664LinkToolchain,
    limits: NativeProcessLimits,
) -> Result<PreparedNativeExecutable, Vec<Diagnostic>> {
    ensure_linux_x86_64_host().map_err(|error| vec![error])?;
    let export = invocation.export();
    let Some(program_export) = program.scalar_abi().exports().nth(export.index()) else {
        return Err(vec![harness_error()]);
    };
    let Some(object_export) = object.scalar_abi().exports().nth(export.index()) else {
        return Err(vec![harness_error()]);
    };
    for candidate in [program_export, object_export] {
        if candidate.logical_name() != export.logical_name()
            || candidate.javascript_name() != export.javascript_name()
            || candidate.webassembly_name() != export.webassembly_name()
            || candidate.native_linux_x86_64_symbol() != export.native_linux_x86_64_symbol()
            || candidate.parameters() != export.parameters()
            || candidate.result() != export.result()
        {
            return Err(vec![harness_error()]);
        }
    }
    let harness = render_invocation_harness(invocation).map_err(|error| vec![error])?;
    let expected_symbol = export.native_linux_x86_64_symbol().as_str();
    let (sealed_bytes, diagnostics) = link_and_audit_native_invocation(
        object.bytes(),
        &harness,
        expected_symbol,
        output_root,
        toolchain,
        limits,
    )?;
    if diagnostics.iter().any(|diagnostic| diagnostic.code() == "ZRYNA-N4016") {
        return Err(diagnostics);
    }
    Ok(PreparedNativeExecutable {
        bytes: Arc::from(sealed_bytes),
        result_type: export.result(),
        expected_symbol: Box::from(expected_symbol),
        ownership_identity: None,
        diagnostics,
    })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
type PreparedNativeBytes = Result<(Box<[u8]>, Vec<Diagnostic>), Vec<Diagnostic>>;

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn prepare_native_invocation_from_verified(
    _program: &VerifiedProgram,
    _object: &ValidatedNativeObjectArtifact,
    _invocation: &zryna_abi::VerifiedInvocation<'_>,
    _output_root: &ArtifactOutputRoot,
    _toolchain: &LinuxX8664LinkToolchain,
    _limits: NativeProcessLimits,
) -> Result<PreparedNativeExecutable, Vec<Diagnostic>> {
    Err(vec![native_error(
        "ZRYNA-N4002",
        "native linking and invocation require a Linux x86-64 host",
        "run this operation on Linux x86-64; other native hosts are not implemented",
    )])
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
#[allow(dead_code, reason = "internal M2 profile is exercised before its public activation")]
pub(crate) fn prepare_control_flow_native_invocation(
    _object: &ValidatedControlFlowNativeObjectArtifact,
    _invocation: zryna_abi::Invocation,
    _output_root: &ArtifactOutputRoot,
    _toolchain: &LinuxX8664LinkToolchain,
    _limits: NativeProcessLimits,
) -> Result<PreparedNativeExecutable, Vec<Diagnostic>> {
    Err(vec![native_error(
        "ZRYNA-N4002",
        "native linking and invocation require a Linux x86-64 host",
        "run this operation on Linux x86-64; other native hosts are not implemented",
    )])
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn prepare_control_flow_native_invocation_from_verified(
    _program: &zryna_ir::control_flow_v1::VerifiedProgram,
    _object: &ValidatedControlFlowNativeObjectArtifact,
    _invocation: &zryna_abi::VerifiedInvocation<'_>,
    _output_root: &ArtifactOutputRoot,
    _toolchain: &LinuxX8664LinkToolchain,
    _limits: NativeProcessLimits,
) -> Result<PreparedNativeExecutable, Vec<Diagnostic>> {
    Err(vec![native_error(
        "ZRYNA-N4002",
        "native linking and invocation require a Linux x86-64 host",
        "run this operation on Linux x86-64; other native hosts are not implemented",
    )])
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn publish_prepared_native_invocation(
    prepared: &PreparedNativeExecutable,
    output_root: &ArtifactOutputRoot,
    artifact_stem: &str,
) -> Result<PublishedNativeExecutableArtifact, Vec<Diagnostic>> {
    validate_artifact_stem(artifact_stem).map_err(|error| vec![error])?;
    output_root.revalidate().map_err(|error| vec![error])?;
    let destination =
        output_root.path().join(format!("{artifact_stem}.{NATIVE_EXECUTABLE_ARTIFACT_EXTENSION}"));
    ensure_destination_absent(&destination, artifact_stem).map_err(|error| vec![error])?;

    let stage = NativeStage::create(output_root, artifact_stem).map_err(|error| vec![error])?;
    let operation = (|| {
        stage.write_input(&stage.executable, prepared.bytes())?;
        let capability_executable = stage.capability_file_path("invocation.elf")?;
        stage.revalidate()?;
        let (executable_identity, copied_bytes) =
            audit_staged_executable(&capability_executable, &prepared.expected_symbol)?;
        if copied_bytes.as_ref() != prepared.bytes() {
            return Err(executable_audit_error());
        }
        prepare_executable_mode(&capability_executable)?;
        stage.revalidate()?;
        let current_identity = regular_file_identity(&capability_executable).map_err(|()| {
            native_error(
                "ZRYNA-N4018",
                "the audited native executable changed before publication",
                "retry with the documented system toolchain and a private output root",
            )
        })?;
        if current_identity != executable_identity {
            return Err(native_error(
                "ZRYNA-N4018",
                "the audited native executable changed before publication",
                "retry with the documented system toolchain and a private output root",
            ));
        }
        output_root.revalidate()?;
        ensure_destination_absent(&destination, artifact_stem)?;
        match fs::hard_link(&capability_executable, &destination) {
            Ok(()) => Ok(destination),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Err(destination_exists_error(
                    artifact_stem,
                    NATIVE_EXECUTABLE_ARTIFACT_EXTENSION,
                    "native executable",
                ))
            }
            Err(_) => Err(native_error(
                "ZRYNA-N4019",
                "the audited native executable could not be published create-only",
                "use a writable declared output root on one filesystem and retry with a fresh stem",
            )),
        }
    })();
    let cleanup = stage.cleanup();
    match operation {
        Ok(path) => {
            let mut diagnostics = prepared.diagnostics().to_vec();
            diagnostics.extend(cleanup);
            Ok(PublishedNativeExecutableArtifact {
                path,
                diagnostics,
                output_root: output_root.clone(),
                prepared: prepared.clone(),
            })
        }
        Err(error) => {
            let mut diagnostics = vec![error];
            diagnostics.extend_from_slice(prepared.diagnostics());
            diagnostics.extend(cleanup);
            Err(diagnostics)
        }
    }
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn publish_prepared_native_invocation(
    _prepared: &PreparedNativeExecutable,
    _output_root: &ArtifactOutputRoot,
    _artifact_stem: &str,
) -> Result<PublishedNativeExecutableArtifact, Vec<Diagnostic>> {
    Err(vec![native_error(
        "ZRYNA-N4002",
        "native linking and invocation require a Linux x86-64 host",
        "run this operation on Linux x86-64; other native hosts are not implemented",
    )])
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn prepare_executable_mode(path: &Path) -> Result<(), Diagnostic> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|_| {
        native_error(
            "ZRYNA-N4019",
            "native executable permissions could not be applied before publication",
            "use a Linux filesystem that supports executable owner permissions",
        )
    })?;
    fs::File::open(path).and_then(|file| file.sync_all()).map_err(|_| {
        native_error(
            "ZRYNA-N4019",
            "native executable could not be synchronized before publication",
            "use a writable Linux filesystem with sufficient space",
        )
    })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn audit_staged_executable(
    path: &Path,
    expected_symbol: &str,
) -> Result<(ToolFileIdentity, Box<[u8]>), Diagnostic> {
    use std::os::unix::fs::OpenOptionsExt;

    let before = regular_file_identity(path).map_err(|()| executable_audit_error())?;
    if before.length == 0
        || usize::try_from(before.length)
            .map_or(true, |length| length > MAX_NATIVE_EXECUTABLE_BYTES)
    {
        return Err(executable_audit_error());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(before.length).unwrap_or(0));
    let mut source = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| executable_audit_error())?;
    let opened =
        tool_identity_from_metadata(&source.metadata().map_err(|_| executable_audit_error())?)
            .map_err(|()| executable_audit_error())?;
    if opened != before {
        return Err(executable_audit_error());
    }
    source.read_to_end(&mut bytes).map_err(|_| executable_audit_error())?;
    let after = regular_file_identity(path).map_err(|()| executable_audit_error())?;
    if before != after || bytes.len() != usize::try_from(before.length).unwrap_or(usize::MAX) {
        return Err(executable_audit_error());
    }
    let file = object::File::parse(bytes.as_slice()).map_err(|_| executable_audit_error())?;
    if file.format() != BinaryFormat::Elf
        || file.architecture() != object::Architecture::X86_64
        || file.endianness() != Endianness::Little
        || file.kind() != ObjectKind::Executable
        || !file.is_64()
        || file.entry() == 0
    {
        return Err(executable_audit_error());
    }
    if file.sections().any(|section| {
        matches!(
            section.flags(),
            object::SectionFlags::Elf { sh_flags }
                if sh_flags & ELF_SECTION_FLAG_WRITE != 0
                    && sh_flags & ELF_SECTION_FLAG_EXECUTE != 0
        )
    }) {
        return Err(executable_audit_error());
    }
    let symbol_valid = file.symbols().any(|symbol| {
        symbol.name() == Ok(expected_symbol)
            && symbol.is_global()
            && symbol.kind() == object::SymbolKind::Text
            && !symbol.is_undefined()
            && symbol.address() != 0
            && symbol.size() != 0
    });
    if !symbol_valid {
        return Err(executable_audit_error());
    }
    Ok((before, bytes.into_boxed_slice()))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn executable_audit_error() -> Diagnostic {
    native_error(
        "ZRYNA-N4018",
        "system linker output failed the native executable audit",
        "use the documented Linux x86-64 GNU toolchain and report the smallest reproducible source",
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn regular_file_identity(path: &Path) -> Result<ToolFileIdentity, ()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    tool_identity_from_metadata(&metadata)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn tool_identity_from_metadata(metadata: &fs::Metadata) -> Result<ToolFileIdentity, ()> {
    use std::os::unix::fs::MetadataExt;

    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(());
    }
    Ok(ToolFileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        length: metadata.len(),
        modified: metadata.modified().map_err(|_| ())?,
    })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[allow(clippy::too_many_arguments)]
#[allow(
    clippy::too_many_lines,
    reason = "test-only error context keeps process control flow intact"
)]
fn run_bounded_process(
    program: &Path,
    arguments: &[OsString],
    working_directory: &Path,
    timeout: Duration,
    stdout_limit: usize,
    stderr_limit: usize,
    phase: ProcessPhase,
    temporary_directory: Option<&Path>,
) -> Result<BoundedProcessOutput, Diagnostic> {
    #[cfg(test)]
    let context = process_diagnostics::Context::new(phase);
    let started = Instant::now();
    let deadline =
        started.checked_add(timeout).ok_or_else(|| process_io_failure!(context, Deadline, None))?;
    let cleanup_deadline =
        deadline.checked_add(NATIVE_PROCESS_CLEANUP_RESERVE).ok_or_else(process_cleanup_error)?;
    let tmpdir = temporary_directory.unwrap_or(Path::new("/tmp"));
    let mut native = std::process::Command::new(program);
    native
        .args(arguments)
        .current_dir(working_directory)
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .env("SOURCE_DATE_EPOCH", "0")
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", tmpdir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut command = CommandWrap::from(native);
    command.wrap(ProcessGroup::leader());
    let mut child = crate::process_spawn::spawn(|| command.spawn())
        .map_err(|error| process_io_failure!(context, Spawn, Some(&error)))?;
    let group_id = child.id().cast_signed();
    let operation = (|| {
        let stdout =
            child.stdout().take().ok_or_else(|| process_io_failure!(context, StdoutPipe, None))?;
        let stderr =
            child.stderr().take().ok_or_else(|| process_io_failure!(context, StderrPipe, None))?;
        let (stdout_sender, stdout_receiver) = mpsc::sync_channel(1);
        let (stderr_sender, stderr_receiver) = mpsc::sync_channel(1);
        let (overflow_sender, overflow_receiver) = mpsc::sync_channel(2);
        let stdout_overflow = overflow_sender.clone();
        thread::spawn(move || {
            let _ = stdout_sender.send(capture_stream(
                stdout,
                stdout_limit,
                &stdout_overflow,
                #[cfg(test)]
                (context, process_diagnostics::Operation::ReadStdout),
            ));
        });
        thread::spawn(move || {
            let _ = stderr_sender.send(capture_stream(
                stderr,
                stderr_limit,
                &overflow_sender,
                #[cfg(test)]
                (context, process_diagnostics::Operation::ReadStderr),
            ));
        });

        let mut timed_out = false;
        let mut output_exceeded = false;
        loop {
            match overflow_receiver.try_recv() {
                Ok(()) => {
                    output_exceeded = true;
                    break;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
            }
            match child
                .try_wait()
                .map_err(|error| process_io_failure!(context, TryWait, Some(&error)))?
            {
                Some(_) => break,
                None if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                None => {
                    timed_out = true;
                    break;
                }
            }
        }
        Ok(PendingProcessOutput {
            stdout: stdout_receiver,
            stderr: stderr_receiver,
            timed_out,
            output_exceeded,
        })
    })();

    let status = cleanup_spawned_process(child.as_mut(), group_id, cleanup_deadline)?;
    let pending = operation?;
    let stdout = receive_capture(&pending.stdout, cleanup_deadline)?;
    let stderr = receive_capture(&pending.stderr, cleanup_deadline)?;

    if pending.timed_out {
        return Err(native_error(
            "ZRYNA-N4007",
            phase.timeout_message(),
            "retry with a smaller input or tighten the operation before the documented hard deadline",
        ));
    }
    if pending.output_exceeded || stdout.exceeded || stderr.exceeded {
        return Err(native_error(
            "ZRYNA-N4008",
            phase.output_message(),
            "use a conforming quiet system toolchain and report repeated unexpected output",
        ));
    }
    Ok(BoundedProcessOutput { status, stdout: stdout.bytes, stderr: stderr.bytes })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn cleanup_spawned_process(
    child: &mut dyn ChildWrapper,
    raw_group_id: i32,
    deadline: Instant,
) -> Result<ExitStatus, Diagnostic> {
    let group = Pid::from_raw(raw_group_id);
    let mut status = None;
    loop {
        let _ = child.start_kill();
        if status.is_none()
            && let Ok(Some(value)) = child.try_wait()
        {
            status = Some(value);
        }
        let group_gone = match signal::killpg(group, None) {
            Err(Errno::ESRCH) => true,
            Ok(()) | Err(Errno::EPERM) => {
                let _ = signal::killpg(group, Signal::SIGKILL);
                false
            }
            Err(_) => return Err(process_cleanup_error()),
        };
        if group_gone && let Some(status) = status {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(process_cleanup_error());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn receive_capture(
    receiver: &Receiver<Result<CapturedStream, Diagnostic>>,
    deadline: Instant,
) -> Result<CapturedStream, Diagnostic> {
    let remaining =
        deadline.checked_duration_since(Instant::now()).ok_or_else(process_cleanup_error)?;
    match receiver.recv_timeout(remaining) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
            Err(process_cleanup_error())
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
struct CapturedStream {
    bytes: Vec<u8>,
    exceeded: bool,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn capture_stream(
    mut stream: impl Read,
    limit: usize,
    overflow: &SyncSender<()>,
    #[cfg(test)] context: (process_diagnostics::Context, process_diagnostics::Operation),
) -> Result<CapturedStream, Diagnostic> {
    let mut bytes = Vec::with_capacity(limit.min(8 * 1_024));
    let mut buffer = [0_u8; 8 * 1_024];
    let mut exceeded = false;
    loop {
        let read = stream.read(&mut buffer).map_err(|error| {
            #[cfg(test)]
            context.0.record(context.1, Some(&error));
            #[cfg(not(test))]
            let _ = error;
            process_io_error()
        })?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let retained = read.min(remaining);
        bytes.extend_from_slice(&buffer[..retained]);
        if retained != read && !exceeded {
            exceeded = true;
            let _ = overflow.try_send(());
        }
    }
    Ok(CapturedStream { bytes, exceeded })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn process_io_error() -> Diagnostic {
    native_error(
        "ZRYNA-N4006",
        "native system process could not be started or observed safely",
        "verify the documented system toolchain and retry on Linux x86-64",
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn process_cleanup_error() -> Diagnostic {
    native_error(
        "ZRYNA-N4009",
        "native system process cleanup could not be confirmed",
        "stop the affected process group before retrying the native operation",
    )
}

#[cfg(test)]
mod link_run_tests {
    use super::*;

    fn control_flow_value(
        id: u32,
        ty: zryna_ir::Type,
    ) -> zryna_native_mir::control_flow_v1::raw::ValueDefinition {
        zryna_native_mir::control_flow_v1::raw::ValueDefinition {
            id: zryna_native_mir::control_flow_v1::raw::ValueId(id),
            ty,
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one link-run fixture covers calls, bool, branches, and loops"
    )]
    fn control_flow_call_program() -> zryna_native_mir::control_flow_v1::VerifiedProgram {
        use zryna_ir::Type;
        use zryna_native_mir::control_flow_v1::{self, raw};

        let helper = raw::Function {
            id: raw::FunctionId { module: raw::ModuleId(1), declaration: 0 },
            internal_symbol: "zryna_m2_i_m1_f0".to_owned(),
            entry_export: None,
            convention: raw::CallingConvention::ZRYNA_INTERNAL_CONTROL_FLOW_V1,
            parameters: vec![control_flow_value(0, Type::I32)],
            result: Type::I32,
            blocks: vec![raw::Block {
                id: raw::BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    raw::Instruction {
                        result: control_flow_value(1, Type::I32),
                        kind: raw::InstructionKind::I32Literal(1),
                    },
                    raw::Instruction {
                        result: control_flow_value(2, Type::I32),
                        kind: raw::InstructionKind::I32Add {
                            lhs: raw::ValueId(0),
                            rhs: raw::ValueId(1),
                        },
                    },
                ],
                terminators: vec![raw::Terminator::Return(raw::ValueId(2))],
            }],
        };
        let exported = raw::Function {
            id: raw::FunctionId { module: raw::ModuleId(0), declaration: 0 },
            internal_symbol: "zryna_m2_i_m0_f0".to_owned(),
            entry_export: Some("run".to_owned()),
            convention: raw::CallingConvention::ZRYNA_INTERNAL_CONTROL_FLOW_V1,
            parameters: vec![control_flow_value(0, Type::I32)],
            result: Type::I32,
            blocks: vec![raw::Block {
                id: raw::BlockId(0),
                parameters: Vec::new(),
                instructions: vec![raw::Instruction {
                    result: control_flow_value(1, Type::I32),
                    kind: raw::InstructionKind::DirectCall {
                        callee: raw::FunctionId { module: raw::ModuleId(1), declaration: 0 },
                        arguments: vec![raw::ValueId(0)],
                    },
                }],
                terminators: vec![raw::Terminator::Return(raw::ValueId(1))],
            }],
        };
        let truth = raw::Function {
            id: raw::FunctionId { module: raw::ModuleId(0), declaration: 1 },
            internal_symbol: "zryna_m2_i_m0_f1".to_owned(),
            entry_export: Some("truth".to_owned()),
            convention: raw::CallingConvention::ZRYNA_INTERNAL_CONTROL_FLOW_V1,
            parameters: vec![control_flow_value(0, Type::Bool)],
            result: Type::Bool,
            blocks: vec![raw::Block {
                id: raw::BlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminators: vec![raw::Terminator::Return(raw::ValueId(0))],
            }],
        };
        let choose = raw::Function {
            id: raw::FunctionId { module: raw::ModuleId(0), declaration: 2 },
            internal_symbol: "zryna_m2_i_m0_f2".to_owned(),
            entry_export: Some("choose".to_owned()),
            convention: raw::CallingConvention::ZRYNA_INTERNAL_CONTROL_FLOW_V1,
            parameters: vec![
                control_flow_value(0, Type::Bool),
                control_flow_value(1, Type::I32),
                control_flow_value(2, Type::I32),
            ],
            result: Type::I32,
            blocks: vec![
                raw::Block {
                    id: raw::BlockId(0),
                    parameters: Vec::new(),
                    instructions: Vec::new(),
                    terminators: vec![raw::Terminator::Branch {
                        condition: raw::ValueId(0),
                        true_target: raw::BlockId(1),
                        true_arguments: vec![raw::ValueId(1)],
                        false_target: raw::BlockId(2),
                        false_arguments: vec![raw::ValueId(2)],
                    }],
                },
                raw::Block {
                    id: raw::BlockId(1),
                    parameters: vec![control_flow_value(3, Type::I32)],
                    instructions: Vec::new(),
                    terminators: vec![raw::Terminator::Return(raw::ValueId(3))],
                },
                raw::Block {
                    id: raw::BlockId(2),
                    parameters: vec![control_flow_value(4, Type::I32)],
                    instructions: Vec::new(),
                    terminators: vec![raw::Terminator::Return(raw::ValueId(4))],
                },
            ],
        };
        let countdown = raw::Function {
            id: raw::FunctionId { module: raw::ModuleId(0), declaration: 3 },
            internal_symbol: "zryna_m2_i_m0_f3".to_owned(),
            entry_export: Some("countdown".to_owned()),
            convention: raw::CallingConvention::ZRYNA_INTERNAL_CONTROL_FLOW_V1,
            parameters: vec![control_flow_value(0, Type::I32)],
            result: Type::I32,
            blocks: vec![
                raw::Block {
                    id: raw::BlockId(0),
                    parameters: Vec::new(),
                    instructions: Vec::new(),
                    terminators: vec![raw::Terminator::Jump {
                        target: raw::BlockId(1),
                        arguments: vec![raw::ValueId(0)],
                    }],
                },
                raw::Block {
                    id: raw::BlockId(1),
                    parameters: vec![control_flow_value(1, Type::I32)],
                    instructions: vec![
                        raw::Instruction {
                            result: control_flow_value(2, Type::I32),
                            kind: raw::InstructionKind::I32Literal(0),
                        },
                        raw::Instruction {
                            result: control_flow_value(3, Type::Bool),
                            kind: raw::InstructionKind::I32GtS {
                                lhs: raw::ValueId(1),
                                rhs: raw::ValueId(2),
                            },
                        },
                    ],
                    terminators: vec![raw::Terminator::Branch {
                        condition: raw::ValueId(3),
                        true_target: raw::BlockId(2),
                        true_arguments: vec![raw::ValueId(1)],
                        false_target: raw::BlockId(3),
                        false_arguments: vec![raw::ValueId(1)],
                    }],
                },
                raw::Block {
                    id: raw::BlockId(2),
                    parameters: vec![control_flow_value(4, Type::I32)],
                    instructions: vec![
                        raw::Instruction {
                            result: control_flow_value(5, Type::I32),
                            kind: raw::InstructionKind::I32Literal(1),
                        },
                        raw::Instruction {
                            result: control_flow_value(6, Type::I32),
                            kind: raw::InstructionKind::I32Sub {
                                lhs: raw::ValueId(4),
                                rhs: raw::ValueId(5),
                            },
                        },
                    ],
                    terminators: vec![raw::Terminator::Jump {
                        target: raw::BlockId(1),
                        arguments: vec![raw::ValueId(6)],
                    }],
                },
                raw::Block {
                    id: raw::BlockId(3),
                    parameters: vec![control_flow_value(7, Type::I32)],
                    instructions: Vec::new(),
                    terminators: vec![raw::Terminator::Return(raw::ValueId(7))],
                },
            ],
        };
        control_flow_v1::verify(raw::Program {
            entry_module: raw::ModuleId(0),
            modules: vec![
                raw::Module {
                    id: raw::ModuleId(0),
                    functions: vec![exported, truth, choose, countdown],
                },
                raw::Module { id: raw::ModuleId(1), functions: vec![helper] },
            ],
        })
        .expect("M2 driver fixture must verify")
    }

    fn prepared_invocation(
        ty: zryna_abi::raw::Type,
        value: zryna_abi::ScalarValue,
    ) -> (zryna_abi::VerifiedScalarAbiModule, zryna_abi::Invocation) {
        let module =
            zryna_abi::verify_v1(zryna_abi::raw::Module::new(vec![zryna_abi::raw::Export::new(
                "identity".to_owned(),
                zryna_abi::raw::Signature::new(vec![ty], ty),
            )]))
            .expect("fixture ABI must verify");
        let invocation = zryna_abi::Invocation::new("identity".to_owned(), vec![value]);
        (module, invocation)
    }

    #[test]
    fn native_process_limits_only_tighten_hard_caps() {
        assert!(NativeProcessLimits::default().tool_output_bytes() > 0);
        assert!(
            NativeProcessLimits::new(
                MIN_NATIVE_PROCESS_TIMEOUT,
                MIN_NATIVE_PROCESS_TIMEOUT,
                MIN_NATIVE_PROCESS_TIMEOUT,
                1,
                1,
            )
            .is_ok()
        );
        assert_eq!(
            NativeProcessLimits::new(
                Duration::ZERO,
                MAX_NATIVE_LINK_TIMEOUT,
                MAX_NATIVE_RUN_TIMEOUT,
                MAX_NATIVE_TOOL_OUTPUT_BYTES,
                MAX_NATIVE_RUN_STDERR_BYTES,
            )
            .expect_err("zero duration must fail")
            .code(),
            "ZRYNA-N4001"
        );
        assert_eq!(
            NativeProcessLimits::new(
                MAX_NATIVE_PROBE_TIMEOUT,
                MAX_NATIVE_LINK_TIMEOUT,
                MAX_NATIVE_RUN_TIMEOUT,
                MAX_NATIVE_TOOL_OUTPUT_BYTES + 1,
                MAX_NATIVE_RUN_STDERR_BYTES,
            )
            .expect_err("oversized output must fail")
            .code(),
            "ZRYNA-N4001"
        );
    }

    #[test]
    fn harness_uses_only_sealed_symbols_and_typed_carriers() {
        let (i32_module, i32_invocation) =
            prepared_invocation(zryna_abi::raw::Type::I32, zryna_abi::ScalarValue::I32(i32::MIN));
        let prepared = i32_module.prepare_invocation(i32_invocation).expect("i32 invocation");
        let source = String::from_utf8(render_invocation_harness(&prepared).expect("i32 harness"))
            .expect("harness UTF-8");
        assert!(source.contains("extern int32_t zryna_v1_e_identity(int32_t);"));
        assert!(source.contains("zryna_v1_e_identity(INT32_MIN)"));
        assert!(source.ends_with("}\n"));

        let (bool_module, bool_invocation) =
            prepared_invocation(zryna_abi::raw::Type::Bool, zryna_abi::ScalarValue::Bool(true));
        let prepared =
            bool_module.prepare_invocation(bool_invocation).expect("Boolean ABI invocation");
        let source = String::from_utf8(render_invocation_harness(&prepared).expect("bool harness"))
            .expect("harness UTF-8");
        assert!(source.contains("zryna_v1_e_identity(INT32_C(1))"));
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn version_parsers_are_fail_closed() {
        assert_eq!(parse_major_version("15.2.0"), Some(15));
        assert_eq!(parse_major_version("15x"), None);
        assert_eq!(parse_major_minor_version("2.46"), Some((2, 46)));
        assert_eq!(parse_major_minor_version("2"), None);
        assert_eq!(parse_major_minor_version("GNU ld 2.46"), None);
        assert_eq!(
            resolve_reported_linker(Path::new("/usr/bin"), "ld"),
            Some(PathBuf::from("/usr/bin/ld"))
        );
        assert_eq!(
            resolve_reported_linker(Path::new("/usr/bin"), "/opt/toolchain/ld"),
            Some(PathBuf::from("/opt/toolchain/ld"))
        );
        assert_eq!(resolve_reported_linker(Path::new("/usr/bin"), "../ld"), None);
        assert_eq!(resolve_reported_linker(Path::new("/usr/bin"), "bin/ld"), None);
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn documented_system_toolchain_discovers_repeatedly() {
        let first = discover_linux_native_toolchain(NativeProcessLimits::default())
            .expect("documented toolchain");
        let second = discover_linux_native_toolchain(NativeProcessLimits::default())
            .expect("repeated documented toolchain");
        assert_eq!(first, second);
        assert!(first.driver().is_absolute());
        assert!(
            (MIN_SUPPORTED_GCC_MAJOR..=MAX_SUPPORTED_GCC_MAJOR).contains(
                &parse_major_version(first.gcc_version()).expect("validated GCC version")
            )
        );
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn control_flow_native_object_links_and_runs_typed_calls_branches_and_loops() {
        let program = control_flow_call_program();
        let target =
            select_object_target(zryna_backend_native::NATIVE_OBJECT_TARGET).expect("M2 target");
        let object = zryna_backend_native::control_flow_v1::emit_object(&program, target)
            .expect("audited M2 object");
        let toolchain = discover_linux_native_toolchain(NativeProcessLimits::default())
            .expect("documented toolchain");
        let workspace = std::env::temp_dir().join(format!(
            "zryna-m2-native-run-{}-{}",
            std::process::id(),
            NEXT_NATIVE_STAGE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(workspace.join(".zryna/out")).expect("M2 output root");
        let output_root =
            ArtifactOutputRoot::for_workspace(&workspace).expect("M2 output capability");
        let deterministic_invocation =
            || zryna_abi::Invocation::new("run".to_owned(), vec![zryna_abi::ScalarValue::I32(41)]);
        let first_executable = prepare_control_flow_native_invocation(
            &object,
            deterministic_invocation(),
            &output_root,
            &toolchain,
            NativeProcessLimits::default(),
        )
        .expect("first deterministic M2 executable");
        let repeated_executable = prepare_control_flow_native_invocation(
            &object,
            deterministic_invocation(),
            &output_root,
            &toolchain,
            NativeProcessLimits::default(),
        )
        .expect("repeated deterministic M2 executable");
        assert_eq!(first_executable.bytes(), repeated_executable.bytes());
        let cases = [
            ("run", vec![zryna_abi::ScalarValue::I32(41)], zryna_abi::ScalarValue::I32(42)),
            (
                "run",
                vec![zryna_abi::ScalarValue::I32(i32::MAX)],
                zryna_abi::ScalarValue::I32(i32::MIN),
            ),
            ("truth", vec![zryna_abi::ScalarValue::Bool(true)], zryna_abi::ScalarValue::Bool(true)),
            (
                "truth",
                vec![zryna_abi::ScalarValue::Bool(false)],
                zryna_abi::ScalarValue::Bool(false),
            ),
            (
                "choose",
                vec![
                    zryna_abi::ScalarValue::Bool(true),
                    zryna_abi::ScalarValue::I32(7),
                    zryna_abi::ScalarValue::I32(9),
                ],
                zryna_abi::ScalarValue::I32(7),
            ),
            (
                "choose",
                vec![
                    zryna_abi::ScalarValue::Bool(false),
                    zryna_abi::ScalarValue::I32(7),
                    zryna_abi::ScalarValue::I32(9),
                ],
                zryna_abi::ScalarValue::I32(9),
            ),
            ("countdown", vec![zryna_abi::ScalarValue::I32(4)], zryna_abi::ScalarValue::I32(0)),
        ];
        for (export, arguments, expected) in cases {
            let prepared = prepare_control_flow_native_invocation(
                &object,
                zryna_abi::Invocation::new(export.to_owned(), arguments),
                &output_root,
                &toolchain,
                NativeProcessLimits::default(),
            )
            .expect("M2 invocation must link");
            let outcome = run_prepared_native_invocation(
                &prepared,
                &output_root,
                NativeProcessLimits::default(),
            )
            .expect("M2 invocation must run");
            assert_eq!(outcome, zryna_abi::ScalarOutcome::Returned { value: expected });
        }
        let invalid = prepare_control_flow_native_invocation(
            &object,
            zryna_abi::Invocation::new("truth".to_owned(), vec![zryna_abi::ScalarValue::I32(2)]),
            &output_root,
            &toolchain,
            NativeProcessLimits::default(),
        )
        .expect_err("wrong typed Boolean carrier must fail before linking");
        assert_eq!(invalid[0].code(), "ZRYNA-B2103");
        assert_eq!(fs::read_dir(output_root.path()).expect("clean output root").count(), 0);
        fs::remove_dir_all(workspace).expect("M2 workspace cleanup");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn retained_stage_identity_rejects_directory_replacement() {
        use std::os::unix::fs::DirBuilderExt;

        let workspace = std::env::temp_dir().join(format!(
            "zryna-native-stage-identity-{}-{}",
            std::process::id(),
            NEXT_NATIVE_STAGE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(workspace.join(".zryna/out")).expect("stage output root");
        let output_root =
            ArtifactOutputRoot::for_workspace(&workspace).expect("stage output capability");
        let stage = NativeStage::create(&output_root, "identity").expect("private stage");
        stage.write_input(&stage.object, b"retained object").expect("handle-relative input");
        let retained_object = stage.capability_file_path("program.o").expect("capability path");
        let moved = output_root.path().join("retained-original");
        fs::rename(&stage.directory, &moved).expect("move original stage");
        let mut replacement = fs::DirBuilder::new();
        replacement.mode(0o700);
        replacement.create(&stage.directory).expect("replacement stage");

        assert_eq!(stage.revalidate().expect_err("replacement must fail").code(), "ZRYNA-N4015");
        let cleanup = stage.cleanup();
        assert_eq!(cleanup.len(), 1);
        assert_eq!(cleanup[0].code(), "ZRYNA-N4016");
        assert!(stage.directory.is_dir(), "replacement must not be removed");
        assert_eq!(fs::read(&retained_object).expect("retained object bytes"), b"retained object");
        assert!(!stage.directory.join("program.o").exists());

        fs::remove_dir(&stage.directory).expect("replacement cleanup");
        fs::remove_file(moved.join("program.o")).expect("retained input cleanup");
        fs::remove_dir(&moved).expect("original cleanup");
        fs::remove_dir_all(workspace).expect("stage workspace cleanup");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn toolchain_discovery_fails_closed_for_missing_and_wrong_tools() {
        assert_eq!(
            discover_linux_native_toolchain_at(
                Path::new("/zryna-missing-system-tool"),
                NativeProcessLimits::default(),
            )
            .expect_err("missing tool must fail")
            .code(),
            "ZRYNA-N4003"
        );
        assert_eq!(
            discover_linux_native_toolchain_at(
                Path::new("/usr/bin/true"),
                NativeProcessLimits::default(),
            )
            .expect_err("non-GCC tool must fail")
            .code(),
            "ZRYNA-N4004"
        );
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn changed_tool_identity_and_failed_link_never_publish() {
        let mut toolchain = discover_linux_native_toolchain(NativeProcessLimits::default())
            .expect("documented toolchain");
        toolchain.driver_identity.length = toolchain.driver_identity.length.saturating_add(1);
        assert_eq!(
            revalidate_tool(&toolchain.driver, &toolchain.driver_identity)
                .expect_err("changed identity must fail")
                .code(),
            "ZRYNA-N4005"
        );

        let (false_driver, false_identity) =
            canonical_tool(Path::new("/usr/bin/false")).expect("false helper identity");
        toolchain.driver = false_driver;
        toolchain.driver_identity = false_identity;
        let workspace = std::env::temp_dir().join(format!(
            "zryna-native-link-failure-{}-{}",
            std::process::id(),
            NEXT_NATIVE_STAGE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(workspace.join(".zryna/out")).expect("link failure output root");
        let output_root =
            ArtifactOutputRoot::for_workspace(&workspace).expect("validated link output root");
        let error = link_and_audit_native_invocation(
            b"not reached by false",
            b"not reached by false",
            "zryna_v1_e_probe",
            &output_root,
            &toolchain,
            NativeProcessLimits::default(),
        )
        .expect_err("failed driver must not publish");
        assert_eq!(error[0].code(), "ZRYNA-N4017");
        assert_eq!(fs::read_dir(output_root.path()).expect("empty output root").count(), 0);
        fs::remove_dir_all(workspace).expect("link failure workspace cleanup");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn executable_audit_rejects_unexpected_symbols_and_links() {
        assert_eq!(
            audit_staged_executable(Path::new("/usr/bin/true"), "zryna_v1_e_missing")
                .expect_err("unsealed system executable must fail")
                .code(),
            "ZRYNA-N4018"
        );

        let workspace = std::env::temp_dir().join(format!(
            "zryna-native-audit-link-{}-{}",
            std::process::id(),
            NEXT_NATIVE_STAGE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&workspace).expect("audit link workspace");
        let link = workspace.join("candidate.elf");
        std::os::unix::fs::symlink("/usr/bin/true", &link).expect("audit symlink fixture");
        assert_eq!(
            audit_staged_executable(&link, "zryna_v1_e_missing")
                .expect_err("link-like candidate must fail")
                .code(),
            "ZRYNA-N4018"
        );
        fs::remove_file(link).expect("audit link cleanup");
        fs::remove_dir(workspace).expect("audit workspace cleanup");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn direct_process_arguments_are_literal_and_outputs_are_bounded() {
        let literal = "value;$(touch zryna-injection-marker)|`false`&more";
        let output = run_bounded_process(
            Path::new("/usr/bin/printf"),
            &[OsString::from("%s"), OsString::from(literal)],
            Path::new("/tmp"),
            Duration::from_secs(1),
            1_024,
            1_024,
            ProcessPhase::Probe,
            None,
        )
        .expect("literal argv process");
        assert!(output.status.success());
        assert_eq!(output.stdout, literal.as_bytes());
        assert!(!Path::new("/tmp/zryna-injection-marker").exists());

        let overflow = run_bounded_process(
            Path::new("/usr/bin/printf"),
            &[OsString::from("12345")],
            Path::new("/tmp"),
            Duration::from_secs(1),
            4,
            4,
            ProcessPhase::Probe,
            None,
        )
        .expect_err("fifth stdout byte must fail");
        assert_eq!(overflow.code(), "ZRYNA-N4008");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn direct_process_timeout_kills_and_reaps_the_group() {
        let error = run_bounded_process(
            Path::new("/usr/bin/sleep"),
            &[OsString::from("5")],
            Path::new("/tmp"),
            Duration::from_millis(100),
            16,
            16,
            ProcessPhase::Run,
            None,
        )
        .expect_err("sleep must time out");
        assert_eq!(error.code(), "ZRYNA-N4007");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn successful_leader_still_closes_descendant_held_pipes() {
        let started = Instant::now();
        let output = run_bounded_process(
            Path::new("/bin/sh"),
            &[OsString::from("-c"), OsString::from("(sleep 5) & exit 0")],
            Path::new("/tmp"),
            Duration::from_secs(1),
            16,
            16,
            ProcessPhase::Run,
            None,
        )
        .expect("leader success must terminate the remaining process group");
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn live_output_overflow_stops_the_process_group_promptly() {
        let started = Instant::now();
        let error = run_bounded_process(
            Path::new("/bin/sh"),
            &[OsString::from("-c"), OsString::from("while :; do printf 12345; done")],
            Path::new("/tmp"),
            Duration::from_secs(5),
            4,
            4,
            ProcessPhase::Run,
            None,
        )
        .expect_err("live output overflow must stop the process group");
        assert_eq!(error.code(), "ZRYNA-N4008");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn invocation_output_interpretation_distinguishes_bad_frames_and_abnormal_exit() {
        use std::os::unix::process::ExitStatusExt;

        assert_eq!(
            interpret_native_invocation_output(
                &BoundedProcessOutput {
                    status: ExitStatus::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                },
                zryna_abi::ScalarType::I32,
            )
            .expect_err("empty result frame must fail")
            .diagnostic()
            .code(),
            "ZRYNA-N4022"
        );

        assert_eq!(
            interpret_native_invocation_output(
                &BoundedProcessOutput {
                    status: ExitStatus::from_raw(1 << 8),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                },
                zryna_abi::ScalarType::I32,
            )
            .expect_err("nonzero exit must fail")
            .diagnostic()
            .code(),
            "ZRYNA-N4021"
        );

        assert_eq!(
            interpret_native_invocation_output(
                &BoundedProcessOutput {
                    status: ExitStatus::from_raw(libc::SIGKILL),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                },
                zryna_abi::ScalarType::I32,
            )
            .expect_err("signal termination must fail")
            .diagnostic()
            .code(),
            "ZRYNA-N4021"
        );
    }

    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    #[test]
    fn control_flow_native_unsupported_host_rejects_before_tool_discovery() {
        assert_eq!(
            discover_linux_native_toolchain(NativeProcessLimits::default())
                .expect_err("non-Linux host must fail")
                .code(),
            "ZRYNA-N4002"
        );

        let workspace = std::env::temp_dir().join(format!(
            "zryna-native-unsupported-run-{}-{}",
            std::process::id(),
            NEXT_NATIVE_STAGE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(workspace.join(".zryna/out")).expect("unsupported run output root");
        let output_root =
            ArtifactOutputRoot::for_workspace(&workspace).expect("unsupported output capability");
        let program = control_flow_call_program();
        let target =
            select_object_target(zryna_backend_native::NATIVE_OBJECT_TARGET).expect("M2 target");
        let object = zryna_backend_native::control_flow_v1::emit_object(&program, target)
            .expect("portable M2 object emission");
        let unavailable_toolchain = LinuxX8664LinkToolchain {
            driver: PathBuf::from("unused"),
            gcc_version: Box::from("unused"),
            linker_version: Box::from("unused"),
        };
        let unsupported = prepare_control_flow_native_invocation(
            &object,
            zryna_abi::Invocation::new("run".to_owned(), vec![zryna_abi::ScalarValue::I32(41)]),
            &output_root,
            &unavailable_toolchain,
            NativeProcessLimits::default(),
        )
        .expect_err("M2 preparation must fail before staging on unsupported hosts");
        assert_eq!(unsupported[0].code(), "ZRYNA-N4002");
        assert_eq!(fs::read_dir(output_root.path()).expect("clean output root").count(), 0);
        let artifact = PublishedNativeExecutableArtifact {
            path: workspace.join(".zryna/out/probe.elf"),
            diagnostics: Vec::new(),
            prepared: PreparedNativeExecutable {
                bytes: Arc::from([]),
                result_type: zryna_abi::ScalarType::I32,
                expected_symbol: Box::from("zryna_v1_e_probe"),
                ownership_identity: None,
                diagnostics: Vec::new(),
            },
        };
        assert_eq!(
            run_native_invocation(&artifact, NativeProcessLimits::default())
                .expect_err("non-Linux run must fail")
                .diagnostic()
                .code(),
            "ZRYNA-N4002"
        );
        fs::remove_dir_all(workspace).expect("unsupported run workspace cleanup");
    }
}
