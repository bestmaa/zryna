//! Installed material preparation, before any authenticated runtime probe.

use std::path::{Path, PathBuf};

use zryna_diagnostics::Diagnostic;

use super::{ToolingCompiler, ToolingCompilerError, ToolingExecutionClosure, configuration_error};
use crate::runtime::{ExpectedRuntime, NodeRuntimeCapability};

pub(crate) struct PreparedInstalledTooling {
    root: PathBuf,
    execution: ToolingExecutionClosure,
}

impl PreparedInstalledTooling {
    pub(crate) fn revalidate(&self) -> Result<(), Diagnostic> {
        self.execution.revalidate()
    }

    pub(crate) fn abort(self) -> Result<(), Diagnostic> {
        self.execution.abort()
    }

    pub(crate) fn finish(self) -> Result<ToolingCompiler, ToolingCompilerError> {
        let Self { root, execution } = self;
        let configured = (|| {
            execution.revalidate().map_err(ToolingCompilerError::Configuration)?;
            let (path, size, digest) = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
                (
                    "runtime/node/node.exe",
                    87_059_456,
                    "923a41f268ab49ede2e3363fbdd9e790609e385c6f3ca880b4ee9a56a8133e5a",
                )
            } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
                (
                    "runtime/node/bin/node",
                    124_674_920,
                    "243fd8938011479f41b3de101842150fa990f33fbbb3f7aabd330857f2d79e1d",
                )
            } else {
                return Err(configuration_error(
                    "unsupported installed tooling host",
                    "use a verified Windows or Linux x86-64 setup",
                ));
            };
            let node = NodeRuntimeCapability::discover_authenticated(
                &root.join(path),
                &root,
                ExpectedRuntime { size, sha256: digest.to_owned() },
            )
            .map_err(ToolingCompilerError::Configuration)?;
            execution.revalidate().map_err(ToolingCompilerError::Configuration)?;
            Ok(node)
        })();
        match configured {
            Ok(node) => ToolingCompiler::from_execution(execution, node),
            Err(primary) => Err(abort_execution(execution, primary)),
        }
    }
}

impl ToolingCompiler {
    pub(crate) fn prepare_installed(
        root: &Path,
    ) -> Result<PreparedInstalledTooling, ToolingCompilerError> {
        let execution = ToolingExecutionClosure::capture_installed(root)
            .map_err(ToolingCompilerError::Configuration)?;
        Ok(PreparedInstalledTooling { root: root.to_owned(), execution })
    }

    pub(crate) fn abort(self) -> Result<(), Diagnostic> {
        let Self { node, frontend, frontend_v3, frontend_v4, execution } = self;
        drop((node, frontend, frontend_v3, frontend_v4));
        execution.abort()
    }
}

pub(super) fn abort_execution(
    execution: ToolingExecutionClosure,
    primary: ToolingCompilerError,
) -> ToolingCompilerError {
    match execution.abort() {
        Ok(()) => primary,
        Err(cleanup) => match primary {
            ToolingCompilerError::Configuration(mut diagnostic) => {
                diagnostic.message =
                    format!("{}; owned cleanup failed: {cleanup}", diagnostic.message);
                ToolingCompilerError::Configuration(diagnostic)
            }
            primary @ ToolingCompilerError::Session(_) => ToolingCompilerError::Configuration(
                super::super::tooling_execution::execution_error(format!(
                    "{primary}; owned cleanup failed: {cleanup}"
                )),
            ),
        },
    }
}
