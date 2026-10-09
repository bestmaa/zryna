//! Private fixture observations. These records cannot authorize linking, loading or execution.

use super::{CapturedForeignLibrary, HandleLinkRequirements};
use crate::native::{self, ArtifactOutputRoot, LinuxX8664LinkToolchain, NativeStage};
use std::{ffi::OsString, path::PathBuf, process::ExitStatus};
use zryna_diagnostics::Diagnostic;

mod elf;
mod export;
mod process;
mod producer;
mod runtime_object;
mod staging;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub(crate) struct Invocation {
    tools: LinuxX8664LinkToolchain,
    arguments: Vec<OsString>,
    directory: PathBuf,
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Actual source/object and originating tool invocation; no reviewed acquisition expectation.
#[derive(Clone, Debug)]
pub(crate) struct CompiledObject {
    source: Vec<u8>,
    bytes: Vec<u8>,
    invocation: Invocation,
}
impl CompiledObject {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub(crate) fn tools(&self) -> &LinuxX8664LinkToolchain {
        &self.invocation.tools
    }
}

/// Retains actual observations through cleanup, including failure stdout/stderr and tool origins.
#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) invocations: Vec<Invocation>,
}
impl From<Diagnostic> for Failure {
    fn from(error: Diagnostic) -> Self {
        Self { diagnostics: vec![error], invocations: Vec::new() }
    }
}

/// Genuine issuer capabilities are retained in memory; exported hashes cannot reconstruct them.
#[derive(Debug)]
pub(crate) struct Observation {
    requirements: HandleLinkRequirements,
    library: CapturedForeignLibrary,
    foreign: CompiledObject,
    client: CompiledObject,
    runtime: Option<CompiledObject>,
    link: Invocation,
    final_elf: Vec<u8>,
    dependencies: elf::Dependencies,
    trace: Vec<String>,
}

pub(crate) const MISSING: [&str; 3] = [
    "independently-issued-ordered-tool-sysroot-startup-and-runtime-material-expectations",
    "authoritative-loader-provider-version-and-load-search-policy",
    "authenticated-recipe-and-host-admission-for-production-execution",
];

pub(crate) use producer::{check_foreign, compile_object, observe};

pub(super) fn rejected() -> Diagnostic {
    native::native_error(
        "ZRYNA-C4102",
        "private linked-output observations lack exact retained fixture input binding",
        "retain the original issuers and actual bytes; observations never approve dependencies",
    )
}
