use std::{
    error::Error,
    ffi::{OsStr, OsString},
    fmt,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use zryna_diagnostics::Diagnostic;
use zryna_source::SourceMap;

use crate::{
    AnalyzeRequest, FrontendCapabilities, ProviderInfo, SourceInput, syntax_v2, syntax_v3,
    syntax_v4,
};

#[cfg(unix)]
mod unix_process;
#[cfg(unix)]
use unix_process::spawn_worker;

const HANDSHAKE_ID: u32 = 1;
const ANALYZE_ID: u32 = 2;
const MAX_PROVIDER_ID_BYTES: usize = 128;
const MAX_PROVIDER_VERSION_BYTES: usize = 128;
const MAX_RESPONSE_LINES: usize = 2;
const MAX_WORKER_ARGUMENTS: usize = 64;
const MAX_WORKER_ARGUMENT_BYTES: usize = 64 * 1_024;
const MAX_CLEANUP_RESERVE: Duration = Duration::from_secs(2);

/// Maximum serialized bytes accepted for the worker handshake response.
pub const MAX_HANDSHAKE_RESPONSE_BYTES: usize = 64 * 1_024;
/// Maximum serialized bytes written for one worker request.
pub const MAX_WORKER_REQUEST_BYTES: usize = 72 * 1_024 * 1_024;
/// Maximum aggregate bytes accepted on worker stdout.
pub const MAX_WORKER_STDOUT_BYTES: usize =
    MAX_HANDSHAKE_RESPONSE_BYTES + syntax_v2::MAX_RESPONSE_BYTES + MAX_RESPONSE_LINES;
/// Maximum aggregate bytes accepted on protocol-v3 worker stdout.
pub const MAX_WORKER_STDOUT_BYTES_V3: usize =
    MAX_HANDSHAKE_RESPONSE_BYTES + syntax_v3::MAX_RESPONSE_BYTES + MAX_RESPONSE_LINES;
/// Maximum aggregate bytes accepted on protocol-v4 worker stdout.
pub const MAX_WORKER_STDOUT_BYTES_V4: usize =
    MAX_HANDSHAKE_RESPONSE_BYTES + syntax_v4::MAX_RESPONSE_BYTES + MAX_RESPONSE_LINES;
/// Maximum aggregate bytes accepted on worker stderr.
pub const MAX_WORKER_STDERR_BYTES: usize = 64 * 1_024;
/// Maximum wall-clock duration of one authenticated worker session.
pub const MAX_WORKER_TIMEOUT: Duration = Duration::from_secs(30);
/// Minimum wall-clock duration that leaves a defensible process-tree cleanup reserve.
pub const MIN_WORKER_TIMEOUT: Duration = Duration::from_secs(1);

/// Exact trusted identity and capabilities required from a frontend worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderExpectation {
    provider: String,
    provider_version: String,
    protocol_version: u32,
    capabilities: FrontendCapabilities,
}

impl ProviderExpectation {
    /// Creates a bounded exact provider expectation.
    ///
    /// # Errors
    ///
    /// Returns a configuration failure when an identity is empty or exceeds its fixed byte bound.
    pub fn new(
        provider: impl Into<String>,
        provider_version: impl Into<String>,
        protocol_version: u32,
        capabilities: FrontendCapabilities,
    ) -> Result<Self, WorkerError> {
        let provider = provider.into();
        let provider_version = provider_version.into();
        if provider.is_empty()
            || provider.len() > MAX_PROVIDER_ID_BYTES
            || provider_version.is_empty()
            || provider_version.len() > MAX_PROVIDER_VERSION_BYTES
            || protocol_version != syntax_v2::PROTOCOL_VERSION
        {
            return Err(WorkerError::new(WorkerFailure::Configuration));
        }
        Ok(Self { provider, provider_version, protocol_version, capabilities })
    }

    /// Returns the required provider identifier.
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Returns the required provider runtime version.
    #[must_use]
    pub fn provider_version(&self) -> &str {
        &self.provider_version
    }

    /// Returns the required Zryna protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> u32 {
        self.protocol_version
    }

    /// Returns the exact required capability set.
    #[must_use]
    pub const fn capabilities(&self) -> &FrontendCapabilities {
        &self.capabilities
    }
}

/// Exact protocol-v3 capabilities advertised by the syntax-only provider.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrontendCapabilitiesV3 {
    /// Whether the provider resolves module specifiers; this must remain false.
    pub module_resolution: bool,
    /// Whether the provider supplies semantic diagnostics; this must remain false.
    pub semantic_diagnostics: bool,
    /// Whether the provider emits the frozen `ControlFlowV1` syntax inventory.
    pub control_flow_v1: bool,
}

/// Version-witnessed protocol-v3 provider identity returned during handshake.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderInfoV3 {
    /// Stable provider name.
    pub provider: String,
    /// Exact upstream provider version.
    pub provider_version: String,
    /// Zryna-owned protocol version, required to be exactly three.
    pub protocol_version: u32,
    /// Exact protocol-v3 capability set.
    pub capabilities: FrontendCapabilitiesV3,
}

/// Exact trusted identity and capabilities required from a protocol-v3 frontend worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderExpectationV3 {
    provider: String,
    provider_version: String,
    capabilities: FrontendCapabilitiesV3,
}

impl ProviderExpectationV3 {
    /// Creates a bounded exact protocol-v3 provider expectation.
    ///
    /// # Errors
    ///
    /// Returns a configuration failure when an identity is empty or exceeds its fixed byte bound.
    pub fn new(
        provider: impl Into<String>,
        provider_version: impl Into<String>,
    ) -> Result<Self, WorkerError> {
        let provider = provider.into();
        let provider_version = provider_version.into();
        if provider.is_empty()
            || provider.len() > MAX_PROVIDER_ID_BYTES
            || provider_version.is_empty()
            || provider_version.len() > MAX_PROVIDER_VERSION_BYTES
        {
            return Err(WorkerError::new(WorkerFailure::Configuration));
        }
        Ok(Self {
            provider,
            provider_version,
            capabilities: FrontendCapabilitiesV3 {
                module_resolution: false,
                semantic_diagnostics: false,
                control_flow_v1: true,
            },
        })
    }

    /// Returns the required provider identifier.
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Returns the required provider runtime version.
    #[must_use]
    pub fn provider_version(&self) -> &str {
        &self.provider_version
    }

    /// Returns the exact required capability set.
    #[must_use]
    pub const fn capabilities(&self) -> &FrontendCapabilitiesV3 {
        &self.capabilities
    }
}

/// Exact protocol-v4 capabilities advertised by the ownership-syntax provider.
#[allow(clippy::struct_excessive_bools)] // Mirrors the frozen four-Boolean wire contract exactly.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrontendCapabilitiesV4 {
    /// Whether the provider resolves module specifiers; this must remain false.
    pub module_resolution: bool,
    /// Whether the provider supplies semantic diagnostics; this must remain false.
    pub semantic_diagnostics: bool,
    /// Whether the provider emits the frozen `ControlFlowV1` syntax inventory.
    pub control_flow_v1: bool,
    /// Whether the provider emits the frozen data-ownership syntax inventory.
    pub data_ownership_syntax_v1: bool,
}

/// Version-witnessed protocol-v4 provider identity returned during handshake.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderInfoV4 {
    /// Stable provider name.
    pub provider: String,
    /// Exact upstream provider version.
    pub provider_version: String,
    /// Zryna-owned protocol version, required to be exactly four.
    pub protocol_version: u32,
    /// Exact protocol-v4 capability set.
    pub capabilities: FrontendCapabilitiesV4,
}

/// Exact trusted identity and capabilities required from a protocol-v4 frontend worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderExpectationV4 {
    provider: String,
    provider_version: String,
    capabilities: FrontendCapabilitiesV4,
}

impl ProviderExpectationV4 {
    /// Creates a bounded exact protocol-v4 provider expectation.
    ///
    /// # Errors
    ///
    /// Returns a configuration failure when an identity is empty or exceeds its fixed byte bound.
    pub fn new(
        provider: impl Into<String>,
        provider_version: impl Into<String>,
    ) -> Result<Self, WorkerError> {
        let provider = provider.into();
        let provider_version = provider_version.into();
        if provider.is_empty()
            || provider.len() > MAX_PROVIDER_ID_BYTES
            || provider_version.is_empty()
            || provider_version.len() > MAX_PROVIDER_VERSION_BYTES
        {
            return Err(WorkerError::new(WorkerFailure::Configuration));
        }
        Ok(Self {
            provider,
            provider_version,
            capabilities: FrontendCapabilitiesV4 {
                module_resolution: false,
                semantic_diagnostics: false,
                control_flow_v1: true,
                data_ownership_syntax_v1: true,
            },
        })
    }

    /// Returns the required provider identifier.
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Returns the required provider runtime version.
    #[must_use]
    pub fn provider_version(&self) -> &str {
        &self.provider_version
    }

    /// Returns the exact required capability set.
    #[must_use]
    pub const fn capabilities(&self) -> &FrontendCapabilitiesV4 {
        &self.capabilities
    }
}

/// Hard-bounded execution limits for one frontend worker session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerLimits {
    timeout: Duration,
    stdout_bytes: usize,
    stderr_bytes: usize,
}

impl WorkerLimits {
    /// Creates limits that may tighten, but never exceed, compiler hard caps.
    ///
    /// # Errors
    ///
    /// Returns a configuration failure for a zero value or a value above a hard cap.
    pub fn new(
        timeout: Duration,
        stdout_bytes: usize,
        stderr_bytes: usize,
    ) -> Result<Self, WorkerError> {
        if timeout < MIN_WORKER_TIMEOUT
            || timeout > MAX_WORKER_TIMEOUT
            || stdout_bytes == 0
            || stdout_bytes > MAX_WORKER_STDOUT_BYTES
            || stderr_bytes == 0
            || stderr_bytes > MAX_WORKER_STDERR_BYTES
        {
            return Err(WorkerError::new(WorkerFailure::Configuration));
        }
        Ok(Self { timeout, stdout_bytes, stderr_bytes })
    }

    /// Returns the whole-session deadline duration.
    #[must_use]
    pub const fn timeout(self) -> Duration {
        self.timeout
    }

    /// Returns the aggregate stdout byte limit.
    #[must_use]
    pub const fn stdout_bytes(self) -> usize {
        self.stdout_bytes
    }

    /// Returns the aggregate stderr byte limit.
    #[must_use]
    pub const fn stderr_bytes(self) -> usize {
        self.stderr_bytes
    }
}

impl Default for WorkerLimits {
    fn default() -> Self {
        Self {
            timeout: MAX_WORKER_TIMEOUT,
            stdout_bytes: MAX_WORKER_STDOUT_BYTES,
            stderr_bytes: MAX_WORKER_STDERR_BYTES,
        }
    }
}

/// Hard-bounded execution limits for one protocol-v3 frontend worker session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerLimitsV3 {
    timeout: Duration,
    stdout_bytes: usize,
    stderr_bytes: usize,
}

impl WorkerLimitsV3 {
    /// Creates v3 limits that may tighten, but never exceed, compiler hard caps.
    ///
    /// # Errors
    ///
    /// Returns a configuration failure for a zero value or a value above a hard cap.
    pub fn new(
        timeout: Duration,
        stdout_bytes: usize,
        stderr_bytes: usize,
    ) -> Result<Self, WorkerError> {
        if timeout < MIN_WORKER_TIMEOUT
            || timeout > MAX_WORKER_TIMEOUT
            || stdout_bytes == 0
            || stdout_bytes > MAX_WORKER_STDOUT_BYTES_V3
            || stderr_bytes == 0
            || stderr_bytes > MAX_WORKER_STDERR_BYTES
        {
            return Err(WorkerError::new(WorkerFailure::Configuration));
        }
        Ok(Self { timeout, stdout_bytes, stderr_bytes })
    }

    /// Returns the whole-session deadline duration.
    #[must_use]
    pub const fn timeout(self) -> Duration {
        self.timeout
    }

    /// Returns the aggregate stdout byte limit.
    #[must_use]
    pub const fn stdout_bytes(self) -> usize {
        self.stdout_bytes
    }

    /// Returns the aggregate stderr byte limit.
    #[must_use]
    pub const fn stderr_bytes(self) -> usize {
        self.stderr_bytes
    }
}

impl Default for WorkerLimitsV3 {
    fn default() -> Self {
        Self {
            timeout: MAX_WORKER_TIMEOUT,
            stdout_bytes: MAX_WORKER_STDOUT_BYTES_V3,
            stderr_bytes: MAX_WORKER_STDERR_BYTES,
        }
    }
}

/// Hard-bounded execution limits for one protocol-v4 frontend worker session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerLimitsV4 {
    timeout: Duration,
    stdout_bytes: usize,
    stderr_bytes: usize,
}

impl WorkerLimitsV4 {
    /// Creates v4 limits that may tighten, but never exceed, compiler hard caps.
    ///
    /// # Errors
    ///
    /// Returns a configuration failure for a zero value or a value above a hard cap.
    pub fn new(
        timeout: Duration,
        stdout_bytes: usize,
        stderr_bytes: usize,
    ) -> Result<Self, WorkerError> {
        if timeout < MIN_WORKER_TIMEOUT
            || timeout > MAX_WORKER_TIMEOUT
            || stdout_bytes == 0
            || stdout_bytes > MAX_WORKER_STDOUT_BYTES_V4
            || stderr_bytes == 0
            || stderr_bytes > MAX_WORKER_STDERR_BYTES
        {
            return Err(WorkerError::new(WorkerFailure::Configuration));
        }
        Ok(Self { timeout, stdout_bytes, stderr_bytes })
    }

    /// Returns the whole-session deadline duration.
    #[must_use]
    pub const fn timeout(self) -> Duration {
        self.timeout
    }

    /// Returns the aggregate stdout byte limit.
    #[must_use]
    pub const fn stdout_bytes(self) -> usize {
        self.stdout_bytes
    }

    /// Returns the aggregate stderr byte limit.
    #[must_use]
    pub const fn stderr_bytes(self) -> usize {
        self.stderr_bytes
    }
}

impl Default for WorkerLimitsV4 {
    fn default() -> Self {
        Self {
            timeout: MAX_WORKER_TIMEOUT,
            stdout_bytes: MAX_WORKER_STDOUT_BYTES_V4,
            stderr_bytes: MAX_WORKER_STDERR_BYTES,
        }
    }
}

/// Direct, no-shell command specification for a frontend worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerSpec {
    executable: PathBuf,
    arguments: Vec<OsString>,
    current_dir: PathBuf,
    expected: ProviderExpectation,
    limits: WorkerLimits,
}

impl WorkerSpec {
    /// Creates an absolute direct-execution worker specification.
    ///
    /// # Errors
    ///
    /// Returns a configuration failure unless both executable and working directory are absolute.
    pub fn new(
        executable: impl Into<PathBuf>,
        arguments: Vec<OsString>,
        current_dir: impl Into<PathBuf>,
        expected: ProviderExpectation,
        limits: WorkerLimits,
    ) -> Result<Self, WorkerError> {
        let executable = executable.into();
        let current_dir = current_dir.into();
        let argument_bytes = arguments.iter().try_fold(0_usize, |total, argument| {
            total.checked_add(argument.as_encoded_bytes().len())
        });
        let is_script_wrapper =
            executable.extension().and_then(OsStr::to_str).is_some_and(|extension| {
                extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
            });
        if !executable.is_absolute()
            || !current_dir.is_absolute()
            || is_script_wrapper
            || arguments.len() > MAX_WORKER_ARGUMENTS
            || argument_bytes.is_none_or(|bytes| bytes > MAX_WORKER_ARGUMENT_BYTES)
        {
            return Err(WorkerError::new(WorkerFailure::Configuration));
        }
        Ok(Self { executable, arguments, current_dir, expected, limits })
    }

    /// Returns the executable passed directly to the operating system.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns literal worker arguments without shell parsing.
    #[must_use]
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    /// Returns the absolute worker directory.
    #[must_use]
    pub fn current_dir(&self) -> &Path {
        &self.current_dir
    }

    /// Returns the exact trusted provider expectation.
    #[must_use]
    pub const fn expected(&self) -> &ProviderExpectation {
        &self.expected
    }

    /// Returns the hard-bounded execution limits.
    #[must_use]
    pub const fn limits(&self) -> WorkerLimits {
        self.limits
    }
}

/// Direct, no-shell command specification for a protocol-v3 frontend worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerSpecV3 {
    executable: PathBuf,
    arguments: Vec<OsString>,
    current_dir: PathBuf,
    expected: ProviderExpectationV3,
    limits: WorkerLimitsV3,
}

impl WorkerSpecV3 {
    /// Creates an absolute direct-execution protocol-v3 worker specification.
    ///
    /// # Errors
    ///
    /// Returns a configuration failure unless both executable and working directory are absolute,
    /// or when the argument inventory exceeds a fixed hard bound.
    pub fn new(
        executable: impl Into<PathBuf>,
        arguments: Vec<OsString>,
        current_dir: impl Into<PathBuf>,
        expected: ProviderExpectationV3,
        limits: WorkerLimitsV3,
    ) -> Result<Self, WorkerError> {
        let executable = executable.into();
        let current_dir = current_dir.into();
        let argument_bytes = arguments.iter().try_fold(0_usize, |total, argument| {
            total.checked_add(argument.as_encoded_bytes().len())
        });
        let is_script_wrapper =
            executable.extension().and_then(OsStr::to_str).is_some_and(|extension| {
                extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
            });
        if !executable.is_absolute()
            || !current_dir.is_absolute()
            || is_script_wrapper
            || arguments.len() > MAX_WORKER_ARGUMENTS
            || argument_bytes.is_none_or(|bytes| bytes > MAX_WORKER_ARGUMENT_BYTES)
        {
            return Err(WorkerError::new(WorkerFailure::Configuration));
        }
        Ok(Self { executable, arguments, current_dir, expected, limits })
    }

    /// Returns the executable passed directly to the operating system.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns literal worker arguments without shell parsing.
    #[must_use]
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    /// Returns the absolute worker directory.
    #[must_use]
    pub fn current_dir(&self) -> &Path {
        &self.current_dir
    }

    /// Returns the exact trusted protocol-v3 provider expectation.
    #[must_use]
    pub const fn expected(&self) -> &ProviderExpectationV3 {
        &self.expected
    }

    /// Returns the hard-bounded execution limits.
    #[must_use]
    pub const fn limits(&self) -> WorkerLimitsV3 {
        self.limits
    }
}

/// Direct, no-shell command specification for a protocol-v4 frontend worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerSpecV4 {
    executable: PathBuf,
    arguments: Vec<OsString>,
    current_dir: PathBuf,
    expected: ProviderExpectationV4,
    limits: WorkerLimitsV4,
}

impl WorkerSpecV4 {
    /// Creates an absolute direct-execution protocol-v4 worker specification.
    ///
    /// # Errors
    ///
    /// Returns a configuration failure unless both executable and working directory are absolute,
    /// or when the argument inventory exceeds a fixed hard bound.
    pub fn new(
        executable: impl Into<PathBuf>,
        arguments: Vec<OsString>,
        current_dir: impl Into<PathBuf>,
        expected: ProviderExpectationV4,
        limits: WorkerLimitsV4,
    ) -> Result<Self, WorkerError> {
        let executable = executable.into();
        let current_dir = current_dir.into();
        let argument_bytes = arguments.iter().try_fold(0_usize, |total, argument| {
            total.checked_add(argument.as_encoded_bytes().len())
        });
        let is_script_wrapper =
            executable.extension().and_then(OsStr::to_str).is_some_and(|extension| {
                extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
            });
        if !executable.is_absolute()
            || !current_dir.is_absolute()
            || is_script_wrapper
            || arguments.len() > MAX_WORKER_ARGUMENTS
            || argument_bytes.is_none_or(|bytes| bytes > MAX_WORKER_ARGUMENT_BYTES)
        {
            return Err(WorkerError::new(WorkerFailure::Configuration));
        }
        Ok(Self { executable, arguments, current_dir, expected, limits })
    }

    /// Returns the executable passed directly to the operating system.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns literal worker arguments without shell parsing.
    #[must_use]
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    /// Returns the absolute worker directory.
    #[must_use]
    pub fn current_dir(&self) -> &Path {
        &self.current_dir
    }

    /// Returns the exact trusted protocol-v4 provider expectation.
    #[must_use]
    pub const fn expected(&self) -> &ProviderExpectationV4 {
        &self.expected
    }

    /// Returns the hard-bounded execution limits.
    #[must_use]
    pub const fn limits(&self) -> WorkerLimitsV4 {
        self.limits
    }
}

/// A configured protocol-v2 worker that returns verified syntax only.
#[derive(Clone, Debug)]
pub struct WorkerFrontend {
    spec: WorkerSpec,
}

/// Provider abstraction whose only output is source-map-verified protocol-v2 syntax.
pub trait VerifiedFrontendProvider: Send + Sync {
    /// Authenticates the provider and analyzes the authoritative source map.
    ///
    /// # Errors
    ///
    /// Returns a fail-closed worker error before untrusted syntax reaches the driver.
    fn analyze_verified(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v2::ProjectSyntaxSnapshot, WorkerError>;
}

impl WorkerFrontend {
    /// Creates a worker frontend from a validated command specification.
    #[must_use]
    pub const fn new(spec: WorkerSpec) -> Self {
        Self { spec }
    }

    /// Returns the worker command specification.
    #[must_use]
    pub const fn spec(&self) -> &WorkerSpec {
        &self.spec
    }

    /// Authenticates one fresh worker, analyzes the authoritative source map, and verifies its reply.
    ///
    /// The analysis request is not written until the worker identity, runtime version, protocol,
    /// and complete capability set match the trusted expectation exactly.
    ///
    /// # Errors
    ///
    /// Returns a stable fail-closed worker failure for configuration, process, framing, identity,
    /// budget, provider, decoding, or syntax-verification errors.
    pub fn analyze_verified(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v2::ProjectSyntaxSnapshot, WorkerError> {
        let analyze_request = build_analyze_request(sources)?;
        let handshake_bytes =
            serialize_request(&HandshakeRequest { id: HANDSHAKE_ID, method: "handshake" })?;
        let analyze_bytes = serialize_request(&AnalyzeWireRequest {
            id: ANALYZE_ID,
            method: "analyze",
            params: &analyze_request,
        })?;

        let spawned = spawn_worker(&self.spec)?;
        let started = Instant::now();
        let deadline = started
            .checked_add(self.spec.limits.timeout)
            .ok_or_else(|| WorkerError::new(WorkerFailure::Configuration))?;
        let operation_deadline = deadline
            .checked_sub(cleanup_reserve(self.spec.limits.timeout))
            .ok_or_else(|| WorkerError::new(WorkerFailure::Configuration))?;
        let mut process = ChildGuard::new(spawned.process, spawned.stdin);
        let stdout = spawned.stdout;
        let stderr = spawned.stderr;

        let (sender, receiver) = mpsc::sync_channel(8);
        process.tasks.push(spawn_stdout_reader(
            stdout,
            sender.clone(),
            self.spec.limits.stdout_bytes,
        ));
        process.tasks.push(spawn_stderr_reader(
            stderr,
            sender.clone(),
            self.spec.limits.stderr_bytes,
        ));
        let mut stream_state = StreamState::default();

        let operation = (|| {
            process.write_request(&handshake_bytes)?;
            let handshake_line = receive_response_line(
                &receiver,
                &mut stream_state,
                operation_deadline,
                MAX_HANDSHAKE_RESPONSE_BYTES,
            )?;
            let handshake: ProviderInfo = parse_response(&handshake_line, HANDSHAKE_ID)?;
            verify_handshake(&handshake, &self.spec.expected)?;

            process.write_request_async(analyze_bytes, sender.clone())?;
            let snapshot_line = receive_response_line(
                &receiver,
                &mut stream_state,
                operation_deadline,
                syntax_v2::MAX_RESPONSE_BYTES,
            )?;
            let decoded: syntax_v2::RawProjectSyntaxSnapshot =
                parse_response(&snapshot_line, ANALYZE_ID)?;
            let canonical = serde_json::to_vec(&decoded)
                .map_err(|_| WorkerError::new(WorkerFailure::InvalidResponse))?;
            let raw = syntax_v2::decode_snapshot(&canonical)
                .map_err(|_| WorkerError::new(WorkerFailure::InvalidResponse))?;
            let status =
                finish_process(&mut process, &receiver, &mut stream_state, operation_deadline)?;
            if !status.success() {
                return Err(WorkerError::new(WorkerFailure::ProcessExit));
            }
            syntax_v2::verify_snapshot(raw, sources).map_err(WorkerError::snapshot_verification)
        })();

        finalize_process(&mut process, &receiver, &mut stream_state, sender, deadline, operation)
    }
}

impl VerifiedFrontendProvider for WorkerFrontend {
    fn analyze_verified(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v2::ProjectSyntaxSnapshot, WorkerError> {
        Self::analyze_verified(self, sources)
    }
}

/// A configured protocol-v3 worker that returns source-map-verified syntax only.
#[derive(Clone, Debug)]
pub struct WorkerFrontendV3 {
    spec: WorkerSpecV3,
}

/// Provider abstraction whose only output is source-map-verified protocol-v3 syntax.
pub trait VerifiedFrontendProviderV3: Send + Sync {
    /// Returns the minimum caller-supplied timeout required by this provider.
    fn minimum_analysis_timeout(&self) -> Duration {
        Duration::ZERO
    }

    /// Authenticates the exact v3 provider and analyzes the authoritative source map.
    ///
    /// # Errors
    ///
    /// Returns a fail-closed worker error before untrusted syntax reaches a compiler consumer.
    fn analyze_verified_v3(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v3::ProjectSyntaxSnapshot, WorkerError>;

    /// Analyzes with a caller-tightened whole-session timeout.
    ///
    /// Providers without an external process may use their normal bounded implementation. The
    /// worker-backed provider overrides this method and cannot outlive the supplied timeout.
    ///
    /// # Errors
    ///
    /// Returns the provider's fail-closed analysis error.
    fn analyze_verified_v3_with_timeout(
        &self,
        sources: &SourceMap,
        _timeout: Duration,
    ) -> Result<syntax_v3::ProjectSyntaxSnapshot, WorkerError> {
        self.analyze_verified_v3(sources)
    }
}

impl WorkerFrontendV3 {
    /// Creates a protocol-v3 worker frontend from a validated command specification.
    #[must_use]
    pub const fn new(spec: WorkerSpecV3) -> Self {
        Self { spec }
    }

    /// Returns the protocol-v3 worker command specification.
    #[must_use]
    pub const fn spec(&self) -> &WorkerSpecV3 {
        &self.spec
    }

    /// Authenticates one fresh exact-v3 worker, analyzes the source map, and verifies its reply.
    ///
    /// The schema-three analysis request is not written until provider identity, runtime version,
    /// protocol version, and all three capabilities match the trusted expectation exactly.
    ///
    /// # Errors
    ///
    /// Returns a stable fail-closed worker failure for configuration, process, framing, identity,
    /// budget, provider, decoding, or protocol-v3 syntax-verification errors.
    pub fn analyze_verified_v3(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v3::ProjectSyntaxSnapshot, WorkerError> {
        let analyze_request = build_analyze_request_v3(sources)?;
        let handshake_bytes =
            serialize_request(&HandshakeRequest { id: HANDSHAKE_ID, method: "handshake" })?;
        let analyze_bytes = serialize_request(&AnalyzeWireRequest {
            id: ANALYZE_ID,
            method: "analyze",
            params: &analyze_request,
        })?;

        let spawned = spawn_worker(&self.spec)?;
        let deadline = Instant::now()
            .checked_add(self.spec.limits.timeout)
            .ok_or_else(|| WorkerError::new(WorkerFailure::Configuration))?;
        let operation_deadline = deadline
            .checked_sub(cleanup_reserve(self.spec.limits.timeout))
            .ok_or_else(|| WorkerError::new(WorkerFailure::Configuration))?;
        let mut process = ChildGuard::new(spawned.process, spawned.stdin);
        let (sender, receiver) = mpsc::sync_channel(8);
        process.tasks.push(spawn_stdout_reader(
            spawned.stdout,
            sender.clone(),
            self.spec.limits.stdout_bytes,
        ));
        process.tasks.push(spawn_stderr_reader(
            spawned.stderr,
            sender.clone(),
            self.spec.limits.stderr_bytes,
        ));
        let mut stream_state = StreamState::default();

        let operation = (|| {
            process.write_request(&handshake_bytes)?;
            let handshake_line = receive_response_line(
                &receiver,
                &mut stream_state,
                operation_deadline,
                MAX_HANDSHAKE_RESPONSE_BYTES,
            )?;
            let handshake: ProviderInfoV3 = parse_response(&handshake_line, HANDSHAKE_ID)?;
            verify_handshake_v3(&handshake, &self.spec.expected)?;

            process.write_request_async(analyze_bytes, sender.clone())?;
            let snapshot_line = receive_response_line(
                &receiver,
                &mut stream_state,
                operation_deadline,
                syntax_v3::MAX_RESPONSE_BYTES,
            )?;
            let decoded: syntax_v3::RawProjectSyntaxSnapshot =
                parse_response(&snapshot_line, ANALYZE_ID)?;
            let canonical = serde_json::to_vec(&decoded)
                .map_err(|_| WorkerError::new(WorkerFailure::InvalidResponse))?;
            let raw = syntax_v3::decode_snapshot(&canonical)
                .map_err(|_| WorkerError::new(WorkerFailure::InvalidResponse))?;
            let status =
                finish_process(&mut process, &receiver, &mut stream_state, operation_deadline)?;
            if !status.success() {
                return Err(WorkerError::new(WorkerFailure::ProcessExit));
            }
            syntax_v3::verify_snapshot(raw, sources).map_err(WorkerError::snapshot_verification)
        })();

        finalize_process(&mut process, &receiver, &mut stream_state, sender, deadline, operation)
    }

    /// Runs one exact-v3 analysis with a timeout no greater than this frontend's configured cap.
    ///
    /// # Errors
    ///
    /// Returns a configuration error when the requested timeout is outside the hard worker range,
    /// or the same fail-closed errors as [`Self::analyze_verified_v3`].
    pub fn analyze_verified_v3_with_timeout(
        &self,
        sources: &SourceMap,
        timeout: Duration,
    ) -> Result<syntax_v3::ProjectSyntaxSnapshot, WorkerError> {
        let timeout = timeout.min(self.spec.limits.timeout());
        let limits = WorkerLimitsV3::new(
            timeout,
            self.spec.limits.stdout_bytes(),
            self.spec.limits.stderr_bytes(),
        )?;
        let mut spec = self.spec.clone();
        spec.limits = limits;
        Self::new(spec).analyze_verified_v3(sources)
    }
}

impl VerifiedFrontendProviderV3 for WorkerFrontendV3 {
    fn minimum_analysis_timeout(&self) -> Duration {
        MIN_WORKER_TIMEOUT
    }

    fn analyze_verified_v3(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v3::ProjectSyntaxSnapshot, WorkerError> {
        Self::analyze_verified_v3(self, sources)
    }

    fn analyze_verified_v3_with_timeout(
        &self,
        sources: &SourceMap,
        timeout: Duration,
    ) -> Result<syntax_v3::ProjectSyntaxSnapshot, WorkerError> {
        Self::analyze_verified_v3_with_timeout(self, sources, timeout)
    }
}

/// A configured protocol-v4 worker that returns source-map-verified syntax only.
#[derive(Clone, Debug)]
pub struct WorkerFrontendV4 {
    spec: WorkerSpecV4,
}

/// Provider abstraction whose only output is source-map-verified protocol-v4 syntax.
pub trait VerifiedFrontendProviderV4: Send + Sync {
    /// Returns the minimum caller-supplied timeout required by this provider.
    fn minimum_analysis_timeout(&self) -> Duration {
        Duration::ZERO
    }

    /// Authenticates the exact v4 provider and analyzes the authoritative source map.
    ///
    /// # Errors
    ///
    /// Returns a fail-closed worker error before untrusted syntax reaches a compiler consumer.
    fn analyze_verified_v4(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v4::ProjectSyntaxSnapshot, WorkerError>;

    /// Analyzes with a caller-tightened whole-session timeout.
    ///
    /// # Errors
    ///
    /// Returns the provider's fail-closed analysis error.
    fn analyze_verified_v4_with_timeout(
        &self,
        sources: &SourceMap,
        _timeout: Duration,
    ) -> Result<syntax_v4::ProjectSyntaxSnapshot, WorkerError> {
        self.analyze_verified_v4(sources)
    }
}

impl WorkerFrontendV4 {
    /// Creates a protocol-v4 worker frontend from a validated command specification.
    #[must_use]
    pub const fn new(spec: WorkerSpecV4) -> Self {
        Self { spec }
    }

    /// Returns the protocol-v4 worker command specification.
    #[must_use]
    pub const fn spec(&self) -> &WorkerSpecV4 {
        &self.spec
    }

    /// Authenticates one fresh exact-v4 worker, analyzes the source map, and verifies its reply.
    ///
    /// The schema-four analysis request is not written until provider identity, runtime version,
    /// protocol version, and all four capabilities match the trusted expectation exactly.
    ///
    /// # Errors
    ///
    /// Returns a stable fail-closed worker failure for configuration, process, framing, identity,
    /// budget, provider, decoding, or protocol-v4 syntax-verification errors.
    pub fn analyze_verified_v4(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v4::ProjectSyntaxSnapshot, WorkerError> {
        let analyze_request = build_analyze_request_v4(sources)?;
        let handshake_bytes =
            serialize_request(&HandshakeRequest { id: HANDSHAKE_ID, method: "handshake" })?;
        let analyze_bytes = serialize_request(&AnalyzeWireRequest {
            id: ANALYZE_ID,
            method: "analyze",
            params: &analyze_request,
        })?;

        let spawned = spawn_worker(&self.spec)?;
        let deadline = Instant::now()
            .checked_add(self.spec.limits.timeout)
            .ok_or_else(|| WorkerError::new(WorkerFailure::Configuration))?;
        let operation_deadline = deadline
            .checked_sub(cleanup_reserve(self.spec.limits.timeout))
            .ok_or_else(|| WorkerError::new(WorkerFailure::Configuration))?;
        let mut process = ChildGuard::new(spawned.process, spawned.stdin);
        let (sender, receiver) = mpsc::sync_channel(8);
        process.tasks.push(spawn_stdout_reader(
            spawned.stdout,
            sender.clone(),
            self.spec.limits.stdout_bytes,
        ));
        process.tasks.push(spawn_stderr_reader(
            spawned.stderr,
            sender.clone(),
            self.spec.limits.stderr_bytes,
        ));
        let mut stream_state = StreamState::default();

        let operation = (|| {
            process.write_request(&handshake_bytes)?;
            let handshake_line = receive_response_line(
                &receiver,
                &mut stream_state,
                operation_deadline,
                MAX_HANDSHAKE_RESPONSE_BYTES,
            )?;
            let handshake: ProviderInfoV4 = parse_response(&handshake_line, HANDSHAKE_ID)?;
            verify_handshake_v4(&handshake, &self.spec.expected)?;

            process.write_request_async(analyze_bytes, sender.clone())?;
            let snapshot_line = receive_response_line(
                &receiver,
                &mut stream_state,
                operation_deadline,
                syntax_v4::MAX_RESPONSE_BYTES,
            )?;
            let decoded: syntax_v4::RawProjectSyntaxSnapshot =
                parse_response(&snapshot_line, ANALYZE_ID)?;
            let canonical = serde_json::to_vec(&decoded)
                .map_err(|_| WorkerError::new(WorkerFailure::InvalidResponse))?;
            let raw = syntax_v4::decode_snapshot(&canonical)
                .map_err(|_| WorkerError::new(WorkerFailure::InvalidResponse))?;
            let status =
                finish_process(&mut process, &receiver, &mut stream_state, operation_deadline)?;
            if !status.success() {
                return Err(WorkerError::new(WorkerFailure::ProcessExit));
            }
            syntax_v4::verify_snapshot(raw, sources).map_err(WorkerError::snapshot_verification)
        })();

        finalize_process(&mut process, &receiver, &mut stream_state, sender, deadline, operation)
    }

    /// Runs one exact-v4 analysis with a timeout no greater than this frontend's configured cap.
    ///
    /// # Errors
    ///
    /// Returns a configuration error when the requested timeout is outside the hard worker range,
    /// or the same fail-closed errors as [`Self::analyze_verified_v4`].
    pub fn analyze_verified_v4_with_timeout(
        &self,
        sources: &SourceMap,
        timeout: Duration,
    ) -> Result<syntax_v4::ProjectSyntaxSnapshot, WorkerError> {
        let timeout = timeout.min(self.spec.limits.timeout());
        let limits = WorkerLimitsV4::new(
            timeout,
            self.spec.limits.stdout_bytes(),
            self.spec.limits.stderr_bytes(),
        )?;
        let mut spec = self.spec.clone();
        spec.limits = limits;
        Self::new(spec).analyze_verified_v4(sources)
    }
}

impl VerifiedFrontendProviderV4 for WorkerFrontendV4 {
    fn minimum_analysis_timeout(&self) -> Duration {
        MIN_WORKER_TIMEOUT
    }

    fn analyze_verified_v4(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v4::ProjectSyntaxSnapshot, WorkerError> {
        Self::analyze_verified_v4(self, sources)
    }

    fn analyze_verified_v4_with_timeout(
        &self,
        sources: &SourceMap,
        timeout: Duration,
    ) -> Result<syntax_v4::ProjectSyntaxSnapshot, WorkerError> {
        Self::analyze_verified_v4_with_timeout(self, sources, timeout)
    }
}

/// Stable frontend-worker failure category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerFailure {
    /// Trusted worker configuration is invalid.
    Configuration,
    /// The direct worker process could not be started.
    Spawn,
    /// Worker pipe I/O failed.
    ProcessIo,
    /// The whole-session deadline expired.
    Timeout,
    /// Worker stdout or stderr exceeded its hard byte budget.
    OutputLimit,
    /// NDJSON framing, JSON shape, or response correlation was invalid.
    InvalidResponse,
    /// The provider returned an explicit error response.
    ProviderRejected,
    /// Provider identity did not match the trusted expectation.
    ProviderIdentity,
    /// Provider runtime version did not match the trusted expectation.
    ProviderVersion,
    /// Provider protocol did not match the trusted expectation.
    ProviderProtocol,
    /// Provider capabilities did not match the trusted expectation.
    ProviderCapabilities,
    /// The worker exited unsuccessfully.
    ProcessExit,
    /// The provider snapshot failed structural verification.
    SnapshotVerification,
    /// The bounded worker cleanup protocol could not be confirmed before its deadline.
    Cleanup,
}

impl WorkerFailure {
    /// Returns the stable diagnostic code for this failure category.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Configuration | Self::Spawn => "ZRYNA-F1101",
            Self::ProviderIdentity
            | Self::ProviderVersion
            | Self::ProviderProtocol
            | Self::ProviderCapabilities => "ZRYNA-F1102",
            Self::InvalidResponse | Self::ProviderRejected => "ZRYNA-F1103",
            Self::Timeout | Self::OutputLimit => "ZRYNA-F1104",
            Self::ProcessExit => "ZRYNA-F1105",
            Self::ProcessIo => "ZRYNA-F1106",
            Self::SnapshotVerification => "ZRYNA-F1107",
            Self::Cleanup => "ZRYNA-F1108",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::Configuration => "frontend worker configuration is invalid",
            Self::Spawn => "frontend worker could not be started",
            Self::ProcessIo => "frontend worker process I/O failed",
            Self::Timeout => "frontend worker exceeded its execution deadline",
            Self::OutputLimit => "frontend worker exceeded a process-output limit",
            Self::InvalidResponse => "frontend worker returned an invalid protocol response",
            Self::ProviderRejected => "frontend worker rejected a protocol request",
            Self::ProviderIdentity => "frontend worker identity does not match",
            Self::ProviderVersion => "frontend worker runtime version does not match",
            Self::ProviderProtocol => "frontend worker protocol version does not match",
            Self::ProviderCapabilities => "frontend worker capabilities do not match",
            Self::ProcessExit => "frontend worker did not exit successfully",
            Self::SnapshotVerification => "frontend worker snapshot verification failed",
            Self::Cleanup => "frontend worker cleanup protocol failed",
        }
    }
}

/// Deterministic frontend-worker error with optional structural diagnostics.
#[derive(Debug)]
pub struct WorkerError {
    failure: WorkerFailure,
    diagnostics: Vec<Diagnostic>,
}

impl WorkerError {
    const fn new(failure: WorkerFailure) -> Self {
        Self { failure, diagnostics: Vec::new() }
    }

    fn snapshot_verification(diagnostics: Vec<Diagnostic>) -> Self {
        Self { failure: WorkerFailure::SnapshotVerification, diagnostics }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn failure(&self) -> WorkerFailure {
        self.failure
    }

    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.failure.code()
    }

    /// Returns bounded structural diagnostics when snapshot verification failed.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

impl fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.failure.message())
    }
}

impl Error for WorkerError {}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct HandshakeRequest<'a> {
    id: u32,
    method: &'a str,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct AnalyzeWireRequest<'a> {
    id: u32,
    method: &'a str,
    params: &'a AnalyzeRequest,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RpcResponse<T> {
    Success(RpcSuccess<T>),
    Failure(RpcFailure),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RpcSuccess<T> {
    id: u32,
    result: T,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RpcFailure {
    id: NullableId,
    #[serde(rename = "error")]
    _error: RpcProviderError,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum NullableId {
    Value(u32),
    Null(()),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RpcProviderError {
    #[serde(rename = "code")]
    _code: String,
    #[serde(rename = "message")]
    _message: String,
}

fn serialize_request(value: &impl Serialize) -> Result<Vec<u8>, WorkerError> {
    let mut writer = BoundedWriter::new(MAX_WORKER_REQUEST_BYTES);
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| WorkerError::new(WorkerFailure::Configuration))?;
    Ok(writer.bytes)
}

struct BoundedWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedWriter {
    const fn new(limit: usize) -> Self {
        Self { bytes: Vec::new(), limit }
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let next = self.bytes.len().checked_add(buffer.len()).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::FileTooLarge, "request limit exceeded")
        })?;
        if next > self.limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::FileTooLarge,
                "request limit exceeded",
            ));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn build_analyze_request(sources: &SourceMap) -> Result<AnalyzeRequest, WorkerError> {
    let mut files = Vec::with_capacity(sources.len());
    for index in 0..sources.len() {
        let raw =
            u32::try_from(index).map_err(|_| WorkerError::new(WorkerFailure::Configuration))?;
        let id = sources
            .verify_file_id(raw)
            .map_err(|_| WorkerError::new(WorkerFailure::Configuration))?;
        let source =
            sources.source(id).ok_or_else(|| WorkerError::new(WorkerFailure::Configuration))?;
        files.push(SourceInput {
            path: source.path().as_str().to_owned(),
            text: source.text().to_owned(),
        });
    }
    Ok(AnalyzeRequest { schema_version: syntax_v2::PROTOCOL_VERSION, files })
}

fn build_analyze_request_v3(sources: &SourceMap) -> Result<AnalyzeRequest, WorkerError> {
    let mut request = build_analyze_request(sources)?;
    request.schema_version = syntax_v3::PROTOCOL_VERSION;
    Ok(request)
}

fn build_analyze_request_v4(sources: &SourceMap) -> Result<AnalyzeRequest, WorkerError> {
    let mut request = build_analyze_request(sources)?;
    request.schema_version = syntax_v4::PROTOCOL_VERSION;
    Ok(request)
}

fn parse_response<T: DeserializeOwned>(bytes: &[u8], expected_id: u32) -> Result<T, WorkerError> {
    let response: RpcResponse<T> = serde_json::from_slice(bytes)
        .map_err(|_| WorkerError::new(WorkerFailure::InvalidResponse))?;
    match response {
        RpcResponse::Success(success) if success.id == expected_id => Ok(success.result),
        RpcResponse::Success(_) => Err(WorkerError::new(WorkerFailure::InvalidResponse)),
        RpcResponse::Failure(RpcFailure { id, _error: _ }) => match id {
            NullableId::Value(id) if id == expected_id => {
                Err(WorkerError::new(WorkerFailure::ProviderRejected))
            }
            NullableId::Value(_) | NullableId::Null(()) => {
                Err(WorkerError::new(WorkerFailure::InvalidResponse))
            }
        },
    }
}

fn verify_handshake(
    actual: &ProviderInfo,
    expected: &ProviderExpectation,
) -> Result<(), WorkerError> {
    if actual.provider != expected.provider {
        return Err(WorkerError::new(WorkerFailure::ProviderIdentity));
    }
    if actual.provider_version != expected.provider_version {
        return Err(WorkerError::new(WorkerFailure::ProviderVersion));
    }
    if actual.protocol_version != expected.protocol_version {
        return Err(WorkerError::new(WorkerFailure::ProviderProtocol));
    }
    if actual.capabilities != expected.capabilities {
        return Err(WorkerError::new(WorkerFailure::ProviderCapabilities));
    }
    Ok(())
}

fn verify_handshake_v3(
    actual: &ProviderInfoV3,
    expected: &ProviderExpectationV3,
) -> Result<(), WorkerError> {
    if actual.provider != expected.provider {
        return Err(WorkerError::new(WorkerFailure::ProviderIdentity));
    }
    if actual.provider_version != expected.provider_version {
        return Err(WorkerError::new(WorkerFailure::ProviderVersion));
    }
    if actual.protocol_version != syntax_v3::PROTOCOL_VERSION {
        return Err(WorkerError::new(WorkerFailure::ProviderProtocol));
    }
    if actual.capabilities != expected.capabilities {
        return Err(WorkerError::new(WorkerFailure::ProviderCapabilities));
    }
    Ok(())
}

fn verify_handshake_v4(
    actual: &ProviderInfoV4,
    expected: &ProviderExpectationV4,
) -> Result<(), WorkerError> {
    if actual.provider != expected.provider {
        return Err(WorkerError::new(WorkerFailure::ProviderIdentity));
    }
    if actual.provider_version != expected.provider_version {
        return Err(WorkerError::new(WorkerFailure::ProviderVersion));
    }
    if actual.protocol_version != syntax_v4::PROTOCOL_VERSION {
        return Err(WorkerError::new(WorkerFailure::ProviderProtocol));
    }
    if actual.capabilities != expected.capabilities {
        return Err(WorkerError::new(WorkerFailure::ProviderCapabilities));
    }
    Ok(())
}

fn cleanup_reserve(timeout: Duration) -> Duration {
    (timeout / 2).min(MAX_CLEANUP_RESERVE)
}

type WorkerInput = Box<dyn Write + Send>;
type WorkerOutput = Box<dyn Read + Send>;

struct SpawnedWorker {
    process: Box<dyn ManagedProcess>,
    stdin: WorkerInput,
    stdout: WorkerOutput,
    stderr: WorkerOutput,
}

trait ManagedProcess: Send {
    fn start_kill(&mut self) -> std::io::Result<()>;
    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>>;
    fn cleanup_confirmed(&self) -> std::io::Result<bool>;
}

trait SpawnSpec {
    fn executable(&self) -> &Path;
    fn arguments(&self) -> &[OsString];
    fn current_dir(&self) -> &Path;
}

impl SpawnSpec for WorkerSpec {
    fn executable(&self) -> &Path {
        &self.executable
    }

    fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    fn current_dir(&self) -> &Path {
        &self.current_dir
    }
}

impl SpawnSpec for WorkerSpecV3 {
    fn executable(&self) -> &Path {
        &self.executable
    }

    fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    fn current_dir(&self) -> &Path {
        &self.current_dir
    }
}

impl SpawnSpec for WorkerSpecV4 {
    fn executable(&self) -> &Path {
        &self.executable
    }

    fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    fn current_dir(&self) -> &Path {
        &self.current_dir
    }
}

#[cfg(windows)]
struct WindowsProcess {
    child: windows_spawn::Child,
    job: windows_spawn::Job,
    job_termination_requested: bool,
}

#[cfg(windows)]
impl ManagedProcess for WindowsProcess {
    fn start_kill(&mut self) -> std::io::Result<()> {
        self.job.terminate(1)?;
        self.job_termination_requested = true;
        Ok(())
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    fn cleanup_confirmed(&self) -> std::io::Result<bool> {
        Ok(self.job_termination_requested)
    }
}

#[cfg(windows)]
fn spawn_worker(spec: &impl SpawnSpec) -> Result<SpawnedWorker, WorkerError> {
    use windows_spawn::{Command, Job, SpawnOptions, Stdio};

    let job = Job::create().map_err(|_| WorkerError::new(WorkerFailure::Spawn))?;
    job.set_kill_on_close(true).map_err(|_| WorkerError::new(WorkerFailure::Spawn))?;

    let mut command = Command::new(spec.executable());
    command
        .args(spec.arguments())
        .current_dir(spec.current_dir())
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in ["SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let options = SpawnOptions::new().job(&job);
    let mut child =
        command.spawn_with(options).map_err(|_| WorkerError::new(WorkerFailure::Spawn))?;
    let stdin = child
        .stdin
        .take()
        .map(|stream| Box::new(stream) as WorkerInput)
        .ok_or_else(|| WorkerError::new(WorkerFailure::ProcessIo))?;
    let stdout = child
        .stdout
        .take()
        .map(|stream| Box::new(stream) as WorkerOutput)
        .ok_or_else(|| WorkerError::new(WorkerFailure::ProcessIo))?;
    let stderr = child
        .stderr
        .take()
        .map(|stream| Box::new(stream) as WorkerOutput)
        .ok_or_else(|| WorkerError::new(WorkerFailure::ProcessIo))?;
    Ok(SpawnedWorker {
        process: Box::new(WindowsProcess { child, job, job_termination_requested: false }),
        stdin,
        stdout,
        stderr,
    })
}

struct ChildGuard {
    child: Box<dyn ManagedProcess>,
    stdin: Option<WorkerInput>,
    tasks: Vec<thread::JoinHandle<()>>,
    armed: bool,
}

impl ChildGuard {
    fn new(child: Box<dyn ManagedProcess>, stdin: WorkerInput) -> Self {
        Self { child, stdin: Some(stdin), tasks: Vec::new(), armed: true }
    }

    fn child_mut(&mut self) -> &mut dyn ManagedProcess {
        self.child.as_mut()
    }

    fn write_request(&mut self, bytes: &[u8]) -> Result<(), WorkerError> {
        let stdin =
            self.stdin.as_mut().ok_or_else(|| WorkerError::new(WorkerFailure::ProcessIo))?;
        stdin
            .write_all(bytes)
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush())
            .map_err(|_| WorkerError::new(WorkerFailure::ProcessIo))
    }

    fn write_request_async(
        &mut self,
        bytes: Vec<u8>,
        sender: SyncSender<ProcessEvent>,
    ) -> Result<(), WorkerError> {
        let mut stdin =
            self.stdin.take().ok_or_else(|| WorkerError::new(WorkerFailure::ProcessIo))?;
        self.tasks.push(thread::spawn(move || {
            let result = stdin
                .write_all(&bytes)
                .and_then(|()| stdin.write_all(b"\n"))
                .and_then(|()| stdin.flush());
            drop(stdin);
            let event =
                if result.is_ok() { ProcessEvent::StdinDone } else { ProcessEvent::StdinIo };
            let _ = sender.send(event);
        }));
        Ok(())
    }

    const fn disarm(&mut self) {
        self.armed = false;
    }

    fn close_owned_stdin(&mut self, state: &mut StreamState) {
        if self.stdin.take().is_some() {
            state.io.stdin_done = true;
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.stdin.take();
        if self.armed {
            let _ = self.child.start_kill();
        }
    }
}

#[derive(Debug)]
enum ProcessEvent {
    StdoutLine(Vec<u8>),
    StdoutFrameLimit,
    StdoutLimit,
    StdoutTrailing,
    StdoutEof,
    StdoutIo,
    StderrLimit,
    StderrEof,
    StderrIo,
    StdinDone,
    StdinIo,
    TasksJoined,
    TasksFailed,
}

#[derive(Default)]
struct StreamState {
    io: IoState,
    cleanup: CleanupState,
}

#[derive(Default)]
struct IoState {
    stdout_eof: bool,
    stderr_eof: bool,
    stdin_done: bool,
}

#[derive(Default)]
struct CleanupState {
    tree_done: bool,
    tasks_joined: bool,
}

fn spawn_stdout_reader(
    mut stdout: impl Read + Send + 'static,
    sender: SyncSender<ProcessEvent>,
    total_limit: usize,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8 * 1_024];
        let mut frame = Vec::new();
        let mut frame_index = 0_usize;
        let mut total = 0_usize;
        let mut total_exceeded = false;
        let mut frame_exceeded = false;
        loop {
            let count = match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => count,
                Err(_) => {
                    let _ = sender.send(ProcessEvent::StdoutIo);
                    return;
                }
            };
            total = total.saturating_add(count);
            if total > total_limit && !total_exceeded {
                total_exceeded = true;
                let _ = sender.send(ProcessEvent::StdoutLimit);
            }
            for byte in &buffer[..count] {
                if *byte == b'\n' {
                    if !frame_exceeded && frame_index <= MAX_RESPONSE_LINES {
                        let _ = sender.send(ProcessEvent::StdoutLine(std::mem::take(&mut frame)));
                    }
                    frame.clear();
                    frame_exceeded = false;
                    frame_index = frame_index.saturating_add(1);
                    continue;
                }
                let frame_limit = if frame_index == 0 {
                    MAX_HANDSHAKE_RESPONSE_BYTES
                } else {
                    syntax_v2::MAX_RESPONSE_BYTES
                };
                if !frame_exceeded && frame.len() < frame_limit {
                    frame.push(*byte);
                } else if !frame_exceeded {
                    frame_exceeded = true;
                    frame.clear();
                    let _ = sender.send(ProcessEvent::StdoutFrameLimit);
                }
            }
        }
        if !frame.is_empty() || frame_exceeded {
            let _ = sender.send(ProcessEvent::StdoutTrailing);
        }
        let _ = sender.send(ProcessEvent::StdoutEof);
    })
}

fn spawn_stderr_reader(
    mut stderr: impl Read + Send + 'static,
    sender: SyncSender<ProcessEvent>,
    limit: usize,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8 * 1_024];
        let mut total = 0_usize;
        let mut exceeded = false;
        loop {
            match stderr.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    total = total.saturating_add(count);
                    if total > limit && !exceeded {
                        exceeded = true;
                        let _ = sender.send(ProcessEvent::StderrLimit);
                    }
                }
                Err(_) => {
                    let _ = sender.send(ProcessEvent::StderrIo);
                    return;
                }
            }
        }
        let _ = sender.send(ProcessEvent::StderrEof);
    })
}

fn receive_response_line(
    receiver: &Receiver<ProcessEvent>,
    state: &mut StreamState,
    deadline: Instant,
    frame_limit: usize,
) -> Result<Vec<u8>, WorkerError> {
    loop {
        let event = receive_event(receiver, deadline)?;
        match event {
            ProcessEvent::StdoutLine(line) if line.len() <= frame_limit => return Ok(line),
            ProcessEvent::StdoutLine(_)
            | ProcessEvent::StdoutFrameLimit
            | ProcessEvent::StdoutTrailing => {
                return Err(WorkerError::new(WorkerFailure::InvalidResponse));
            }
            ProcessEvent::StdoutEof => {
                state.io.stdout_eof = true;
                return Err(WorkerError::new(WorkerFailure::InvalidResponse));
            }
            ProcessEvent::StdoutLimit | ProcessEvent::StderrLimit => {
                return Err(WorkerError::new(WorkerFailure::OutputLimit));
            }
            ProcessEvent::StdoutIo => {
                state.io.stdout_eof = true;
                return Err(WorkerError::new(WorkerFailure::ProcessIo));
            }
            ProcessEvent::StderrIo => {
                state.io.stderr_eof = true;
                return Err(WorkerError::new(WorkerFailure::ProcessIo));
            }
            ProcessEvent::StdinIo => {
                state.io.stdin_done = true;
                return Err(WorkerError::new(WorkerFailure::ProcessIo));
            }
            ProcessEvent::StderrEof => state.io.stderr_eof = true,
            ProcessEvent::StdinDone => state.io.stdin_done = true,
            ProcessEvent::TasksJoined | ProcessEvent::TasksFailed => {
                return Err(WorkerError::new(WorkerFailure::ProcessIo));
            }
        }
    }
}

fn finish_process(
    process: &mut ChildGuard,
    receiver: &Receiver<ProcessEvent>,
    state: &mut StreamState,
    deadline: Instant,
) -> Result<ExitStatus, WorkerError> {
    let mut status = None;
    loop {
        if status.is_none() {
            status = process
                .child_mut()
                .try_wait()
                .map_err(|_| WorkerError::new(WorkerFailure::ProcessIo))?;
        }
        if state.io.stdout_eof
            && state.io.stderr_eof
            && state.io.stdin_done
            && let Some(status) = status
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(WorkerError::new(WorkerFailure::Timeout));
        }
        let wait =
            deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(10));
        match receiver.recv_timeout(wait) {
            Ok(ProcessEvent::StdoutEof) => state.io.stdout_eof = true,
            Ok(ProcessEvent::StderrEof) => state.io.stderr_eof = true,
            Ok(
                ProcessEvent::StdoutLine(_)
                | ProcessEvent::StdoutTrailing
                | ProcessEvent::StdoutFrameLimit,
            ) => {
                return Err(WorkerError::new(WorkerFailure::InvalidResponse));
            }
            Ok(ProcessEvent::StdoutLimit | ProcessEvent::StderrLimit) => {
                return Err(WorkerError::new(WorkerFailure::OutputLimit));
            }
            Ok(ProcessEvent::StdinDone) => state.io.stdin_done = true,
            Err(RecvTimeoutError::Timeout) => {}
            Ok(ProcessEvent::StdoutIo) => {
                state.io.stdout_eof = true;
                return Err(WorkerError::new(WorkerFailure::ProcessIo));
            }
            Ok(ProcessEvent::StderrIo) => {
                state.io.stderr_eof = true;
                return Err(WorkerError::new(WorkerFailure::ProcessIo));
            }
            Ok(ProcessEvent::StdinIo) => {
                state.io.stdin_done = true;
                return Err(WorkerError::new(WorkerFailure::ProcessIo));
            }
            Ok(ProcessEvent::TasksJoined | ProcessEvent::TasksFailed) => {
                return Err(WorkerError::new(WorkerFailure::ProcessIo));
            }
            Err(RecvTimeoutError::Disconnected) => {
                if state.io.stdout_eof && state.io.stderr_eof && state.io.stdin_done {
                    thread::sleep(wait);
                } else {
                    return Err(WorkerError::new(WorkerFailure::ProcessIo));
                }
            }
        }
    }
}

fn finalize_process<T>(
    process: &mut ChildGuard,
    receiver: &Receiver<ProcessEvent>,
    state: &mut StreamState,
    sender: SyncSender<ProcessEvent>,
    deadline: Instant,
    operation: Result<T, WorkerError>,
) -> Result<T, WorkerError> {
    process.close_owned_stdin(state);
    let kill_result = process.child_mut().start_kill();
    if kill_result.as_ref().is_err_and(|error| !process_was_gone(error)) {
        return Err(WorkerError::new(WorkerFailure::Cleanup));
    }

    let task_joiner = start_task_joiner(process, sender);
    let cleanup = drain_terminated_process(process, receiver, state, deadline);
    if cleanup.is_err() {
        return Err(WorkerError::new(WorkerFailure::Cleanup));
    }
    task_joiner.join().map_err(|_| WorkerError::new(WorkerFailure::Cleanup))?;
    process.disarm();
    operation
}

fn start_task_joiner(
    process: &mut ChildGuard,
    sender: SyncSender<ProcessEvent>,
) -> thread::JoinHandle<()> {
    let tasks = std::mem::take(&mut process.tasks);
    thread::spawn(move || {
        let mut failed = false;
        for task in tasks {
            if task.join().is_err() {
                failed = true;
            }
        }
        let event = if failed { ProcessEvent::TasksFailed } else { ProcessEvent::TasksJoined };
        let _ = sender.send(event);
    })
}

fn process_was_gone(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(3)
    }
    #[cfg(windows)]
    {
        let _ = error;
        false
    }
    #[cfg(not(any(unix, windows)))]
    {
        matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidInput)
    }
}

fn drain_terminated_process(
    process: &mut ChildGuard,
    receiver: &Receiver<ProcessEvent>,
    state: &mut StreamState,
    deadline: Instant,
) -> Result<(), WorkerError> {
    loop {
        update_tree_state(process, state)?;
        if state.cleanup.tree_done
            && state.cleanup.tasks_joined
            && state.io.stdout_eof
            && state.io.stderr_eof
            && state.io.stdin_done
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(WorkerError::new(WorkerFailure::Cleanup));
        }
        let wait =
            deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(10));
        match receiver.recv_timeout(wait) {
            Ok(ProcessEvent::StdoutEof | ProcessEvent::StdoutIo) => state.io.stdout_eof = true,
            Ok(ProcessEvent::StderrEof | ProcessEvent::StderrIo) => state.io.stderr_eof = true,
            Ok(ProcessEvent::StdinDone | ProcessEvent::StdinIo) => state.io.stdin_done = true,
            Ok(ProcessEvent::TasksJoined) => state.cleanup.tasks_joined = true,
            Ok(ProcessEvent::TasksFailed) => {
                return Err(WorkerError::new(WorkerFailure::Cleanup));
            }
            Ok(
                ProcessEvent::StdoutLine(_)
                | ProcessEvent::StdoutFrameLimit
                | ProcessEvent::StdoutLimit
                | ProcessEvent::StdoutTrailing
                | ProcessEvent::StderrLimit,
            )
            | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if !(state.io.stdout_eof && state.io.stderr_eof && state.io.stdin_done) {
                    return Err(WorkerError::new(WorkerFailure::Cleanup));
                }
            }
        }
    }
}

fn update_tree_state(process: &mut ChildGuard, state: &mut StreamState) -> Result<(), WorkerError> {
    let leader_exited = process
        .child_mut()
        .try_wait()
        .map_err(|_| WorkerError::new(WorkerFailure::Cleanup))?
        .is_some();
    let cleanup_confirmed =
        process.child.cleanup_confirmed().map_err(|_| WorkerError::new(WorkerFailure::Cleanup))?;
    state.cleanup.tree_done = leader_exited && cleanup_confirmed;
    Ok(())
}

fn receive_event(
    receiver: &Receiver<ProcessEvent>,
    deadline: Instant,
) -> Result<ProcessEvent, WorkerError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(WorkerError::new(WorkerFailure::Timeout));
    }
    match receiver.recv_timeout(remaining) {
        Ok(event) => Ok(event),
        Err(RecvTimeoutError::Timeout) => Err(WorkerError::new(WorkerFailure::Timeout)),
        Err(RecvTimeoutError::Disconnected) => Err(WorkerError::new(WorkerFailure::ProcessIo)),
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsStr, io::Cursor};

    use super::*;

    #[test]
    fn limits_can_tighten_but_not_expand_hard_caps() {
        assert!(WorkerLimits::new(MIN_WORKER_TIMEOUT, 1, 1).is_ok());
        assert!(
            WorkerLimits::new(MIN_WORKER_TIMEOUT.saturating_sub(Duration::from_nanos(1)), 1, 1)
                .is_err()
        );
        assert!(
            WorkerLimits::new(
                MAX_WORKER_TIMEOUT + Duration::from_millis(1),
                MAX_WORKER_STDOUT_BYTES,
                MAX_WORKER_STDERR_BYTES,
            )
            .is_err()
        );
    }

    #[test]
    fn request_writer_accepts_exact_limit_and_rejects_one_more() {
        let mut writer = BoundedWriter::new(3);
        assert_eq!(writer.write(b"abc").expect("exact write"), 3);
        assert!(writer.write(b"d").is_err());
        assert_eq!(writer.bytes, b"abc");
    }

    #[test]
    fn stderr_reader_accepts_exact_limit_and_rejects_one_more() {
        let (sender, receiver) = mpsc::sync_channel(8);
        let task = spawn_stderr_reader(Cursor::new(vec![b'e'; 8]), sender, 8);
        assert!(matches!(receiver.recv(), Ok(ProcessEvent::StderrEof)));
        task.join().expect("stderr reader must stop");

        let (sender, receiver) = mpsc::sync_channel(8);
        let task = spawn_stderr_reader(Cursor::new(vec![b'e'; 9]), sender, 8);
        assert!(matches!(receiver.recv(), Ok(ProcessEvent::StderrLimit)));
        assert!(matches!(receiver.recv(), Ok(ProcessEvent::StderrEof)));
        task.join().expect("stderr reader must stop");
    }

    #[test]
    fn expectation_requires_protocol_v2() {
        assert!(
            ProviderExpectation::new(
                "typescript-6",
                "6.0.3",
                1,
                FrontendCapabilities { module_resolution: false, semantic_diagnostics: false },
            )
            .is_err()
        );
    }

    #[test]
    fn stdout_reader_accepts_exact_frame_cap_and_rejects_cap_plus_one() {
        let mut exact_line = vec![b'x'; MAX_HANDSHAKE_RESPONSE_BYTES];
        exact_line.push(b'\n');
        let (sender, receiver) = mpsc::sync_channel(8);
        let task = spawn_stdout_reader(Cursor::new(exact_line), sender, MAX_WORKER_STDOUT_BYTES);
        assert!(
            matches!(receiver.recv(), Ok(ProcessEvent::StdoutLine(line)) if line.len() == MAX_HANDSHAKE_RESPONSE_BYTES)
        );
        assert!(matches!(receiver.recv(), Ok(ProcessEvent::StdoutEof)));
        task.join().expect("stdout reader must stop");

        let mut over = vec![b'x'; MAX_HANDSHAKE_RESPONSE_BYTES + 1];
        over.push(b'\n');
        let (sender, receiver) = mpsc::sync_channel(8);
        let task = spawn_stdout_reader(Cursor::new(over), sender, MAX_WORKER_STDOUT_BYTES);
        assert!(matches!(receiver.recv(), Ok(ProcessEvent::StdoutFrameLimit)));
        assert!(matches!(receiver.recv(), Ok(ProcessEvent::StdoutEof)));
        task.join().expect("stdout reader must stop");
    }

    #[test]
    fn strict_response_rejects_duplicate_and_mixed_envelopes() {
        let duplicate = br#"{"id":1,"id":1,"result":{"provider":"p","provider_version":"1","protocol_version":2,"capabilities":{"module_resolution":false,"semantic_diagnostics":false}}}"#;
        assert_eq!(
            parse_response::<ProviderInfo>(duplicate, HANDSHAKE_ID)
                .expect_err("duplicate fields must fail")
                .failure(),
            WorkerFailure::InvalidResponse
        );
        let mixed = br#"{"id":1,"result":{},"error":{"code":"x","message":"x"}}"#;
        assert_eq!(
            parse_response::<ProviderInfo>(mixed, HANDSHAKE_ID)
                .expect_err("mixed envelope must fail")
                .failure(),
            WorkerFailure::InvalidResponse
        );
    }

    #[test]
    fn provider_error_requires_the_exact_request_id() {
        let matching = br#"{"id":1,"error":{"code":"x","message":"x"}}"#;
        assert_eq!(
            parse_response::<ProviderInfo>(matching, HANDSHAKE_ID)
                .expect_err("provider error must fail")
                .failure(),
            WorkerFailure::ProviderRejected
        );
        let null_id = br#"{"id":null,"error":{"code":"x","message":"x"}}"#;
        assert_eq!(
            parse_response::<ProviderInfo>(null_id, HANDSHAKE_ID)
                .expect_err("null correlation must fail")
                .failure(),
            WorkerFailure::InvalidResponse
        );
    }

    #[test]
    fn handshake_verifies_every_exact_field() {
        let expected = ProviderExpectation::new(
            "typescript-6",
            "6.0.3",
            2,
            FrontendCapabilities { module_resolution: false, semantic_diagnostics: false },
        )
        .expect("valid expectation");
        let mut actual = ProviderInfo {
            provider: "typescript-6".to_owned(),
            provider_version: "6.0.3".to_owned(),
            protocol_version: 2,
            capabilities: expected.capabilities.clone(),
        };
        assert!(verify_handshake(&actual, &expected).is_ok());
        actual.provider = "other".to_owned();
        assert_eq!(
            verify_handshake(&actual, &expected).expect_err("identity must fail").failure(),
            WorkerFailure::ProviderIdentity
        );
        actual.provider = "typescript-6".to_owned();
        actual.provider_version = "6.0.2".to_owned();
        assert_eq!(
            verify_handshake(&actual, &expected).expect_err("version must fail").failure(),
            WorkerFailure::ProviderVersion
        );
        actual.provider_version = "6.0.3".to_owned();
        actual.protocol_version = 1;
        assert_eq!(
            verify_handshake(&actual, &expected).expect_err("protocol must fail").failure(),
            WorkerFailure::ProviderProtocol
        );
        actual.protocol_version = 2;
        actual.capabilities.module_resolution = true;
        assert_eq!(
            verify_handshake(&actual, &expected).expect_err("capability must fail").failure(),
            WorkerFailure::ProviderCapabilities
        );
    }

    #[test]
    fn worker_spec_requires_absolute_direct_paths() {
        let expected = ProviderExpectation::new(
            "typescript-6",
            "6.0.3",
            2,
            FrontendCapabilities { module_resolution: false, semantic_diagnostics: false },
        )
        .expect("valid expectation");
        assert!(
            WorkerSpec::new(
                OsStr::new("node"),
                Vec::new(),
                Path::new("."),
                expected,
                WorkerLimits::default(),
            )
            .is_err()
        );
    }
}
