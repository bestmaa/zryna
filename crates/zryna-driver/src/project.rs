use std::path::PathBuf;

mod frozen;
use frozen::RootPackageSources;

use zryna_abi::ScalarValue;
use zryna_diagnostics::Diagnostic;
use zryna_package::{PackageSourceKind, ResolvedGraph};

use crate::{
    BuildRequest, CommandFailure, CommandFailureKind, PackageLockMode, PackageResolutionRequest,
    TargetSelection,
};

/// One package-authenticated standalone M1 build request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectBuildRequest {
    /// Absolute compiler checkout that must pass the complete architecture gate.
    pub compiler_root: PathBuf,
    /// Absolute standalone package root used only for source and project-owned state.
    pub project_root: PathBuf,
    /// Portable project-relative `.zry` entrypoint declared by the package inventory.
    pub entrypoint: String,
    /// Portable output stem.
    pub artifact_stem: String,
    /// Explicit target selection covered by the package compatibility record.
    pub targets: TargetSelection,
    /// Absolute direct Node.js executable used by the frontend and target runtimes.
    pub node_runtime: PathBuf,
}

/// One package-authenticated standalone M1 run request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRunRequest {
    /// Shared standalone build configuration.
    pub build: ProjectBuildRequest,
    /// Exact logical scalar export.
    pub logical_export: String,
    /// Ordered typed arguments.
    pub arguments: Vec<ScalarValue>,
}

/// Authenticates, builds, and atomically commits one standalone project bundle.
///
/// # Errors
///
/// Returns stable compiler, package, source, target, or publication diagnostics without replacing
/// existing project state.
pub fn build_project(
    request: &ProjectBuildRequest,
) -> Result<crate::CommandSuccess, CommandFailure> {
    crate::pipeline::build_project_request(request)
}

/// Authenticates, builds, executes, and commits one standalone project run bundle.
///
/// # Errors
///
/// Returns stable diagnostics without publishing a partial or unauthenticated project bundle.
pub fn run_project(request: ProjectRunRequest) -> Result<crate::CommandSuccess, CommandFailure> {
    crate::pipeline::run_project_request(request)
}

pub(crate) struct ProjectAdmission {
    resolution: PackageResolutionRequest,
    graph: ResolvedGraph,
    entrypoint_path: String,
    entrypoint: Vec<u8>,
    sources: RootPackageSources,
}

impl ProjectAdmission {
    pub(crate) fn discover(request: &ProjectBuildRequest) -> Result<Self, CommandFailure> {
        Self::discover_profile(request, "i32-v1", RootPackageSources::snapshot)
    }

    pub(crate) fn discover_installed(
        request: &ProjectBuildRequest,
        installation: &crate::distribution::InstalledCompiler,
        profile: crate::distribution::InstalledProfile,
    ) -> Result<Self, CommandFailure> {
        installation.revalidate().map_err(|diagnostic| CommandFailure {
            kind: CommandFailureKind::Preparation,
            diagnostics: vec![diagnostic],
        })?;
        Self::discover_profile(request, profile.package_profile(), RootPackageSources::retained)
    }

    fn discover_profile(
        request: &ProjectBuildRequest,
        profile: &str,
        resolve: fn(
            &PackageResolutionRequest,
        ) -> Result<
            (crate::PackageResolutionSuccess, RootPackageSources),
            zryna_package::ResolveError,
        >,
    ) -> Result<Self, CommandFailure> {
        if !request.compiler_root.is_absolute() || !request.project_root.is_absolute() {
            return Err(project_error(
                "ZRYNA-C2001",
                CommandFailureKind::Request,
                "compiler and project roots must be absolute",
                "resolve --root and --project-root to absolute real directories",
            ));
        }
        if request.targets.component() {
            return Err(project_error(
                "ZRYNA-C2001",
                CommandFailureKind::Request,
                "standalone package compatibility does not include component emission",
                "select javascript, webassembly, native, or all",
            ));
        }
        let parent = request.project_root.parent().ok_or_else(|| {
            project_error(
                "ZRYNA-C2001",
                CommandFailureKind::Request,
                "project root must have a real parent directory",
                "select the generated standalone project directory",
            )
        })?;
        let package = request
            .project_root
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                project_error(
                    "ZRYNA-C2001",
                    CommandFailureKind::Request,
                    "project root name must be portable UTF-8",
                    "select a lowercase portable project directory",
                )
            })?
            .to_owned();
        let resolution = PackageResolutionRequest {
            source_root: parent.to_path_buf(),
            package: package.clone(),
            git_cache: None,
            mode: PackageLockMode::Frozen,
        };
        let (success, sources) = resolve(&resolution).map_err(|error| package_failure(&error))?;
        validate_graph(success.graph(), &package, request.targets, profile)?;
        let entrypoint =
            sources.root_file(&request.entrypoint).map(<[u8]>::to_vec).ok_or_else(|| {
                project_error(
                    "ZRYNA-P4004",
                    CommandFailureKind::Source,
                    "entrypoint is not declared by the authenticated root package",
                    "select an exact source path from the root package manifest",
                )
            })?;
        Ok(Self {
            resolution,
            graph: success.graph().clone(),
            entrypoint_path: request.entrypoint.clone(),
            entrypoint,
            sources,
        })
    }

    pub(crate) fn entrypoint_text(&self) -> Result<String, CommandFailure> {
        String::from_utf8(self.entrypoint.clone()).map_err(|_| {
            project_error(
                "ZRYNA-P4004",
                CommandFailureKind::Source,
                "authenticated project entrypoint is not valid UTF-8",
                "store canonical UTF-8 source bytes in the root package",
            )
        })
    }

    pub(crate) fn revalidate(&self) -> Result<(), CommandFailure> {
        self.sources.revalidate_retained().map_err(|error| package_failure(&error))?;
        let (current, files) = crate::package_resolution::resolve_project_package(&self.resolution)
            .map_err(|error| package_failure(&error))?;
        if current.graph() != &self.graph {
            return Err(project_error(
                "ZRYNA-P4010",
                CommandFailureKind::Source,
                "standalone project package identity changed during compilation",
                "stop concurrent project mutation and retry",
            ));
        }
        let entrypoint_matches = files
            .iter()
            .any(|file| file.path == self.entrypoint_path && file.bytes == self.entrypoint);
        if !entrypoint_matches {
            return Err(project_error(
                "ZRYNA-P4010",
                CommandFailureKind::Source,
                "standalone project entrypoint changed during compilation",
                "stop concurrent project mutation and retry",
            ));
        }
        Ok(())
    }
}

impl ProjectBuildRequest {
    pub(crate) fn as_workspace_request(&self) -> BuildRequest {
        BuildRequest {
            workspace_root: self.project_root.clone(),
            entrypoint: self.entrypoint.clone(),
            artifact_stem: self.artifact_stem.clone(),
            targets: self.targets,
            node_runtime: self.node_runtime.clone(),
        }
    }
}

fn validate_graph(
    graph: &ResolvedGraph,
    root_locator: &str,
    targets: TargetSelection,
    profile: &str,
) -> Result<(), CommandFailure> {
    let compatibility = graph.compatibility();
    if !compiler_version_is_compatible(&compatibility.compiler, env!("CARGO_PKG_VERSION"))
        || compatibility.profile != profile
    {
        return Err(project_error(
            "ZRYNA-P4009",
            CommandFailureKind::Source,
            "standalone project compiler version or profile is incompatible",
            "use a package compatible with this compiler and the selected profile",
        ));
    }
    for target in selected_targets(targets) {
        if !compatibility.targets.iter().any(|available| available == target) {
            return Err(project_error(
                "ZRYNA-P4009",
                CommandFailureKind::Source,
                "standalone project does not declare the selected target",
                "select a target listed by the root package compatibility record",
            ));
        }
    }
    let descendant_prefix = format!("{root_locator}/");
    if graph.packages().iter().any(|package| {
        package.source().kind == PackageSourceKind::Local
            && package.source().locator != root_locator
            && !package.source().locator.starts_with(&descendant_prefix)
    }) {
        return Err(project_error(
            "ZRYNA-P4005",
            CommandFailureKind::Source,
            "local package dependency escapes the explicit project tree",
            "place every local package below the selected project root",
        ));
    }
    Ok(())
}

fn compiler_version_is_compatible(project: &str, compiler: &str) -> bool {
    project == compiler || matches!((project, compiler), ("0.2.1" | "0.2.2", "0.2.3"))
}

fn selected_targets(selection: TargetSelection) -> &'static [&'static str] {
    match selection {
        TargetSelection::JavaScript => &["javascript"],
        TargetSelection::WebAssembly => &["webassembly"],
        TargetSelection::Native => &["native-linux-x86_64"],
        TargetSelection::Component | TargetSelection::BrowserComponent => &[],
        TargetSelection::All => &["javascript", "native-linux-x86_64", "webassembly"],
    }
}

fn package_failure(error: &zryna_package::ResolveError) -> CommandFailure {
    project_error(
        error.code(),
        CommandFailureKind::Source,
        error.detail(),
        "restore the canonical manifest, declared source inventory, and frozen lock",
    )
}

fn project_error(
    code: &'static str,
    kind: CommandFailureKind,
    message: impl Into<String>,
    help: impl Into<String>,
) -> CommandFailure {
    CommandFailure { kind, diagnostics: vec![Diagnostic::error(code, None, message, help)] }
}

#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;
