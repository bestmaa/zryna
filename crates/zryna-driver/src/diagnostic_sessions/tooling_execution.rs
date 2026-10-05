//! Captured executable closure for the pinned tooling frontend.

mod capture;
#[cfg(test)]
mod installed_tests;
mod stage;
#[cfg(test)]
mod tests;

use std::path::Path;

use zryna_diagnostics::Diagnostic;

use capture::CapturedToolingClosure;
use stage::ToolingStage;

/// A fixed, private copy of every non-runtime byte the tooling workers can execute.
///
/// The capture assumes a trusted compiler installation at discovery time and a private host
/// staging directory. It prevents later workspace path replacement from changing execution; it
/// is not an operating-system sandbox against an arbitrary same-user writer.
#[derive(Debug)]
pub(super) struct ToolingExecutionClosure {
    stage: ToolingStage,
}

impl ToolingExecutionClosure {
    pub(super) fn capture_installed(root: &Path) -> Result<Self, Diagnostic> {
        let captured = CapturedToolingClosure::capture_installed(root)?;
        Ok(Self { stage: ToolingStage::create(&captured)? })
    }

    pub(super) fn capture(root: &Path) -> Result<Self, Diagnostic> {
        let captured = CapturedToolingClosure::capture(root)?;
        Ok(Self { stage: ToolingStage::create(&captured)? })
    }

    pub(super) fn worker(&self) -> &Path {
        self.stage.worker()
    }

    pub(super) fn worker_v3(&self) -> std::path::PathBuf {
        self.stage.worker_v3()
    }

    pub(super) fn worker_v4(&self) -> std::path::PathBuf {
        self.stage.worker_v4()
    }

    pub(super) fn working_directory(&self) -> &Path {
        self.stage.working_directory()
    }

    pub(super) fn revalidate(&self) -> Result<(), Diagnostic> {
        self.stage.revalidate()
    }

    /// Explicitly disposes the owned stage; Drop alone is not cleanup evidence.
    pub(super) fn abort(self) -> Result<(), Diagnostic> {
        self.stage.abort()
    }
}

pub(super) fn execution_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-D3001",
        None,
        message,
        "restore the pinned TypeScript 6 tooling closure and retry on a private stable host",
    )
}
