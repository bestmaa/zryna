use std::{ffi::OsString, fmt, path::Path, time::Duration};

use zryna_diagnostics::{Diagnostic, Severity};
use zryna_frontend::{
    FrontendCapabilities, ProviderExpectation, ProviderExpectationV3, ProviderExpectationV4,
    WorkerFrontend, WorkerFrontendV3, WorkerFrontendV4, WorkerLimits, WorkerLimitsV3,
    WorkerLimitsV4, WorkerSpec, WorkerSpecV3, WorkerSpecV4, syntax_v2,
};
use zryna_source::{NormalizedSourcePath, SourceMap};

use super::tooling_execution::ToolingExecutionClosure;
use super::{DiagnosticRevision, DiagnosticSession, DiagnosticSessionError};
use crate::ownership_closure::discover_ownership_module_closure_with_overlays_bounded;
use crate::runtime::{NodeRuntimeCapability, node_compatible_path};
use crate::{WorkspaceSourceRoot, discover_ownership_module_closure_with_overlays};

mod browser;
mod installed;

/// Pinned protocol-v2 and protocol-v3 compiler frontends retained by a tooling transport.
#[derive(Debug)]
pub struct ToolingCompiler {
    node: NodeRuntimeCapability,
    frontend: WorkerFrontend,
    frontend_v3: WorkerFrontendV3,
    frontend_v4: WorkerFrontendV4,
    execution: ToolingExecutionClosure,
}

/// Failure to configure or use the bounded tooling compiler.
#[derive(Debug)]
pub enum ToolingCompilerError {
    /// Runtime, adapter, or worker configuration was unavailable or changed.
    Configuration(Diagnostic),
    /// Revision admission failed after analysis completed.
    Session(DiagnosticSessionError),
}

impl fmt::Display for ToolingCompilerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(diagnostic) => write!(formatter, "{diagnostic}"),
            Self::Session(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ToolingCompilerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Configuration(_) => None,
            Self::Session(error) => Some(error),
        }
    }
}

impl ToolingCompiler {
    /// Discovers the pinned runtime and fixed protocol-v2 adapter below one compiler workspace.
    ///
    /// # Errors
    ///
    /// Rejects non-absolute paths, links/reparse points, missing fixed adapter entries, the wrong
    /// runtime version, or an invalid worker specification.
    pub fn discover(root: &Path, node: &Path) -> Result<Self, ToolingCompilerError> {
        if !root.is_absolute() || !node.is_absolute() {
            return Err(configuration_error(
                "tooling compiler root and Node.js runtime must be absolute paths",
                "pass the absolute compiler workspace and Node.js 22.22.1 executable",
            ));
        }
        let execution =
            ToolingExecutionClosure::capture(root).map_err(ToolingCompilerError::Configuration)?;
        let node = NodeRuntimeCapability::discover(node, root)
            .map_err(ToolingCompilerError::Configuration)?;
        Self::from_execution(execution, node)
    }

    /// Captures the fixed scalar tooling runtime from an independently verified distribution.
    ///
    /// No checkout, package manager, runtime override, or ambient provider search is used.
    /// Initial executable authentication remains the installer's responsibility.
    ///
    /// # Errors
    /// Rejects unsafe paths or any worker, dependency or runtime bytes differing from the pins.
    pub fn discover_installed(root: &Path) -> Result<Self, ToolingCompilerError> {
        Self::prepare_installed(root)?.finish()
    }

    fn from_execution(
        execution: ToolingExecutionClosure,
        node: NodeRuntimeCapability,
    ) -> Result<Self, ToolingCompilerError> {
        let configured = (|| {
            let expected = ProviderExpectation::new(
                "typescript-6",
                "6.0.3",
                syntax_v2::PROTOCOL_VERSION,
                FrontendCapabilities { module_resolution: false, semantic_diagnostics: false },
            )
            .map_err(|_| {
                configuration_error(
                    "the fixed tooling frontend expectation is invalid",
                    "restore the registered protocol-v2 adapter contract",
                )
            })?;
            let spec = WorkerSpec::new(
                node.executable().map_err(ToolingCompilerError::Configuration)?,
                vec![OsString::from(node_compatible_path(execution.worker()))],
                node_compatible_path(execution.working_directory()),
                expected,
                WorkerLimits::default(),
            )
            .map_err(|_| {
                configuration_error(
                    "the fixed tooling frontend process could not be configured",
                    "restore the registered adapter and pinned runtime paths",
                )
            })?;
            let expected_v3 =
                ProviderExpectationV3::new("typescript-6", "6.0.3").map_err(|_| {
                    configuration_error(
                        "the fixed control-flow tooling frontend expectation is invalid",
                        "restore the registered protocol-v3 adapter contract",
                    )
                })?;
            let spec_v3 = WorkerSpecV3::new(
                node.executable().map_err(ToolingCompilerError::Configuration)?,
                vec![OsString::from(node_compatible_path(&execution.worker_v3()))],
                node_compatible_path(execution.working_directory()),
                expected_v3,
                WorkerLimitsV3::default(),
            )
            .map_err(|_| {
                configuration_error(
                    "the fixed control-flow tooling frontend process could not be configured",
                    "restore the registered adapter and pinned runtime paths",
                )
            })?;
            let expected_v4 =
                ProviderExpectationV4::new("typescript-6", "6.0.3").map_err(|_| {
                    configuration_error(
                        "the fixed data-ownership tooling frontend expectation is invalid",
                        "restore the registered protocol-v4 adapter contract",
                    )
                })?;
            let spec_v4 = WorkerSpecV4::new(
                node.executable().map_err(ToolingCompilerError::Configuration)?,
                vec![OsString::from(node_compatible_path(&execution.worker_v4()))],
                node_compatible_path(execution.working_directory()),
                expected_v4,
                WorkerLimitsV4::default(),
            )
            .map_err(|_| {
                configuration_error(
                    "the fixed data-ownership tooling frontend process could not be configured",
                    "restore the registered adapter and pinned runtime paths",
                )
            })?;
            Ok((
                WorkerFrontend::new(spec),
                WorkerFrontendV3::new(spec_v3),
                WorkerFrontendV4::new(spec_v4),
            ))
        })();
        match configured {
            Ok((frontend, frontend_v3, frontend_v4)) => {
                Ok(Self { node, frontend, frontend_v3, frontend_v4, execution })
            }
            Err(primary) => {
                drop(node);
                Err(installed::abort_execution(execution, primary))
            }
        }
    }

    /// Analyzes and admits one exact in-memory source revision.
    ///
    /// Frontend failures are retained as their authoritative diagnostic report. Semantic facts are
    /// retained only after the existing scalar checker succeeds.
    ///
    /// # Errors
    ///
    /// Rejects runtime replacement or a failed revision admission.
    pub fn admit(
        &self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
    ) -> Result<DiagnosticRevision, ToolingCompilerError> {
        self.node.revalidate().map_err(ToolingCompilerError::Configuration)?;
        self.execution.revalidate().map_err(ToolingCompilerError::Configuration)?;
        let analysis = crate::analyze_sources(&self.frontend, &sources);
        self.execution.revalidate().map_err(ToolingCompilerError::Configuration)?;
        self.node.revalidate().map_err(ToolingCompilerError::Configuration)?;
        match analysis {
            Ok(syntax) => session.admit_analysis(sources, &syntax),
            Err(error) => session.admit_worker_failure(sources, &error),
        }
        .map_err(ToolingCompilerError::Session)
    }

    /// Analyzes one in-memory M2 source revision with the authenticated protocol-v3 worker.
    ///
    /// The selected entry must belong to the supplied source map. No file lookup, module
    /// discovery, artifact publication, or workspace program execution occurs here.
    ///
    /// # Errors
    ///
    /// Rejects runtime replacement or failed revision admission.
    pub fn admit_control_flow(
        &self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
        entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, ToolingCompilerError> {
        if sources.file_id(entrypoint).is_none() {
            return Err(ToolingCompilerError::Session(DiagnosticSessionError::SemanticAuthority));
        }
        self.node.revalidate().map_err(ToolingCompilerError::Configuration)?;
        self.execution.revalidate().map_err(ToolingCompilerError::Configuration)?;
        let analysis = self.frontend_v3.analyze_verified_v3(&sources);
        self.execution.revalidate().map_err(ToolingCompilerError::Configuration)?;
        self.node.revalidate().map_err(ToolingCompilerError::Configuration)?;
        match analysis {
            Ok(syntax) => session.admit_control_flow_analysis(sources, &syntax, entrypoint),
            Err(error) => session.admit_worker_failure(sources, &error),
        }
        .map_err(ToolingCompilerError::Session)
    }

    /// Analyzes an exact in-memory M3 source map using the authenticated protocol-v4 worker.
    ///
    /// # Errors
    /// Rejects runtime replacement or failed revision admission.
    pub fn admit_data_ownership(
        &self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
        entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, ToolingCompilerError> {
        if sources.file_id(entrypoint).is_none() {
            return Err(ToolingCompilerError::Session(DiagnosticSessionError::SemanticAuthority));
        }
        self.node.revalidate().map_err(ToolingCompilerError::Configuration)?;
        self.execution.revalidate().map_err(ToolingCompilerError::Configuration)?;
        let analysis = self.frontend_v4.analyze_verified_v4(&sources);
        self.execution.revalidate().map_err(ToolingCompilerError::Configuration)?;
        self.node.revalidate().map_err(ToolingCompilerError::Configuration)?;
        match analysis {
            Ok(syntax) => session.admit_data_ownership_analysis(sources, &syntax, entrypoint),
            Err(error) => session.admit_worker_failure(sources, &error),
        }
        .map_err(ToolingCompilerError::Session)
    }

    /// Discovers saved M3 imports under a retained root with exact open-buffer overlays.
    ///
    /// # Errors
    /// Rejects source-graph, runtime, worker, semantic, or retained-revision failure.
    pub fn admit_data_ownership_workspace(
        &self,
        session: &mut DiagnosticSession,
        root: &WorkspaceSourceRoot,
        overlays: &SourceMap,
        entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, ToolingCompilerError> {
        self.node.revalidate().map_err(ToolingCompilerError::Configuration)?;
        self.execution.revalidate().map_err(ToolingCompilerError::Configuration)?;
        let closure = discover_ownership_module_closure_with_overlays(
            root,
            entrypoint.clone(),
            overlays,
            &self.frontend_v4,
        );
        self.execution.revalidate().map_err(ToolingCompilerError::Configuration)?;
        self.node.revalidate().map_err(ToolingCompilerError::Configuration)?;
        match closure {
            Ok(closure) => session
                .admit_data_ownership_analysis(
                    closure.sources().clone(),
                    closure.syntax(),
                    entrypoint,
                )
                .map_err(ToolingCompilerError::Session),
            Err(crate::ModuleClosureError::Frontend(error)) => {
                if error.diagnostics().is_empty() {
                    session
                        .admit_worker_failure(overlays.clone(), &error)
                        .map_err(ToolingCompilerError::Session)
                } else {
                    admit_closure_diagnostics(session, overlays, error.diagnostics())
                }
            }
            Err(crate::ModuleClosureError::Rejected(diagnostics)) => {
                admit_closure_diagnostics(session, overlays, &diagnostics)
            }
        }
    }

    /// Rechecks a saved-import graph against the immutable admitted source map before edits.
    #[must_use]
    pub fn revalidate_data_ownership_workspace(
        &self,
        root: &WorkspaceSourceRoot,
        overlays: &SourceMap,
        entrypoint: &NormalizedSourcePath,
        admitted: &SourceMap,
        budget: Duration,
    ) -> bool {
        if self.node.revalidate().is_err() || self.execution.revalidate().is_err() {
            return false;
        }
        let Ok(closure) = discover_ownership_module_closure_with_overlays_bounded(
            root,
            entrypoint.clone(),
            overlays,
            &self.frontend_v4,
            budget,
        ) else {
            return false;
        };
        if self.node.revalidate().is_err() || self.execution.revalidate().is_err() {
            return false;
        }
        let actual = closure.sources();
        actual.len() == admitted.len()
            && (0..actual.len()).all(|raw| {
                let Ok(index) = u32::try_from(raw) else { return false };
                let (Ok(actual_id), Ok(admitted_id)) =
                    (actual.verify_file_id(index), admitted.verify_file_id(index))
                else {
                    return false;
                };
                match (actual.source(actual_id), admitted.source(admitted_id)) {
                    (Some(left), Some(right)) => {
                        left.path() == right.path() && left.text() == right.text()
                    }
                    _ => false,
                }
            })
    }
}

fn admit_closure_diagnostics(
    session: &mut DiagnosticSession,
    overlays: &SourceMap,
    diagnostics: &[Diagnostic],
) -> Result<DiagnosticRevision, ToolingCompilerError> {
    // Discovery spans belong to temporary provider batches, not the retained open-buffer map.
    let safe = diagnostics
        .iter()
        .map(|diagnostic| {
            let path = diagnostic.path().map(str::to_owned);
            match diagnostic.severity() {
                Severity::Error => Diagnostic::error(
                    diagnostic.code(),
                    path,
                    diagnostic.message(),
                    diagnostic.guidance(),
                ),
                Severity::Warning => Diagnostic::warning(
                    diagnostic.code(),
                    path,
                    diagnostic.message(),
                    diagnostic.guidance(),
                ),
            }
        })
        .collect::<Vec<_>>();
    session.admit_diagnostics(overlays.clone(), &safe).map_err(ToolingCompilerError::Session)
}

fn configuration_error(message: impl Into<String>, guidance: &'static str) -> ToolingCompilerError {
    ToolingCompilerError::Configuration(Diagnostic::error("ZRYNA-D3001", None, message, guidance))
}
