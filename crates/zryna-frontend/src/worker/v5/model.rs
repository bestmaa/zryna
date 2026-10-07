//! Exact v5 identity and bounded direct-execution configuration.
use super::super::{
    Duration, MAX_PROVIDER_ID_BYTES, MAX_PROVIDER_VERSION_BYTES, MAX_WORKER_ARGUMENT_BYTES,
    MAX_WORKER_ARGUMENTS, MAX_WORKER_STDERR_BYTES, MAX_WORKER_TIMEOUT, MIN_WORKER_TIMEOUT, OsStr,
    OsString, Path, PathBuf, SpawnSpec, WorkerError, WorkerFailure,
};
use super::MAX_WORKER_STDOUT_BYTES_V5;
use serde::{Deserialize, Serialize};
/// Exact five-Boolean syntax-only v5 capability witness.
#[allow(clippy::struct_excessive_bools)] // Mirrors the exact versioned wire witness.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrontendCapabilitiesV5 {
    /// Whether the provider resolves module specifiers; this must remain false.
    pub module_resolution: bool,
    /// Whether the provider supplies semantic diagnostics; this must remain false.
    pub semantic_diagnostics: bool,
    /// Whether the provider emits the frozen `ControlFlowV1` syntax inventory.
    pub control_flow_v1: bool,
    /// Whether the provider emits the frozen data-ownership syntax inventory.
    pub data_ownership_syntax_v1: bool,
    /// Whether the provider emits the independently versioned bounded generic syntax.
    pub bounded_generics_syntax_v1: bool,
}

/// Version-witnessed protocol-v5 provider identity returned during handshake.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderInfoV5 {
    /// Stable provider name.
    pub provider: String,
    /// Exact upstream provider version.
    pub provider_version: String,
    /// Zryna-owned protocol version, required to be exactly five.
    pub protocol_version: u32,
    /// Exact protocol-v5 capability set.
    pub capabilities: FrontendCapabilitiesV5,
}

/// Exact trusted identity and capabilities required from a protocol-v5 frontend worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderExpectationV5 {
    pub(super) provider: String,
    pub(super) provider_version: String,
    pub(super) capabilities: FrontendCapabilitiesV5,
}

impl ProviderExpectationV5 {
    /// Creates a bounded exact protocol-v5 provider expectation.
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
            capabilities: FrontendCapabilitiesV5 {
                module_resolution: false,
                semantic_diagnostics: false,
                control_flow_v1: true,
                data_ownership_syntax_v1: true,
                bounded_generics_syntax_v1: true,
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
    pub const fn capabilities(&self) -> &FrontendCapabilitiesV5 {
        &self.capabilities
    }
}

/// Hard-bounded execution limits for one frontend worker session.
/// Hard-bounded execution limits for one protocol-v5 frontend worker session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerLimitsV5 {
    pub(super) timeout: Duration,
    pub(super) stdout_bytes: usize,
    pub(super) stderr_bytes: usize,
}

impl WorkerLimitsV5 {
    /// Creates v5 limits that may tighten, but never exceed, compiler hard caps.
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
            || stdout_bytes > MAX_WORKER_STDOUT_BYTES_V5
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

impl Default for WorkerLimitsV5 {
    fn default() -> Self {
        Self {
            timeout: MAX_WORKER_TIMEOUT,
            stdout_bytes: MAX_WORKER_STDOUT_BYTES_V5,
            stderr_bytes: MAX_WORKER_STDERR_BYTES,
        }
    }
}

/// Direct, no-shell command specification for a frontend worker.
/// Direct, no-shell command specification for a protocol-v5 frontend worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerSpecV5 {
    pub(super) executable: PathBuf,
    pub(super) arguments: Vec<OsString>,
    pub(super) current_dir: PathBuf,
    pub(super) expected: ProviderExpectationV5,
    pub(super) limits: WorkerLimitsV5,
}

impl WorkerSpecV5 {
    /// Creates an absolute direct-execution protocol-v5 worker specification.
    ///
    /// # Errors
    ///
    /// Returns a configuration failure unless both executable and working directory are absolute,
    /// or when the argument inventory exceeds a fixed hard bound.
    pub fn new(
        executable: impl Into<PathBuf>,
        arguments: Vec<OsString>,
        current_dir: impl Into<PathBuf>,
        expected: ProviderExpectationV5,
        limits: WorkerLimitsV5,
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

    /// Returns the exact trusted protocol-v5 provider expectation.
    #[must_use]
    pub const fn expected(&self) -> &ProviderExpectationV5 {
        &self.expected
    }

    /// Returns the hard-bounded execution limits.
    #[must_use]
    pub const fn limits(&self) -> WorkerLimitsV5 {
        self.limits
    }
}

impl SpawnSpec for WorkerSpecV5 {
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
