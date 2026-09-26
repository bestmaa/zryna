//! Zryna command-line interface.

#![forbid(unsafe_code)]

mod installed;
mod ownership;
mod package;
mod profile;
mod project;
mod project_filesystem;
mod render;

use render::{render_cli_failure, render_failure, render_success};

use std::{ffi::OsString, path::PathBuf, process::ExitCode};

use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use profile::{CliProfile, CliTarget};
use zryna_abi::ScalarValue;
use zryna_diagnostics::Diagnostic;
use zryna_driver::{
    BuildRequest, CommandKind, ControlFlowBuildRequest, ControlFlowRunRequest,
    DataOwnershipBuildRequest, DataOwnershipRunRequest, RunRequest, TargetSelection,
};

#[derive(Debug, Parser)]
#[command(name = "zryna", version, about = "Strict Zryna compiler workspace tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate the canonical repository architecture.
    Architecture {
        #[command(subcommand)]
        command: ArchitectureCommand,
    },
    /// Report local compiler-workspace health.
    Doctor(ArchitectureOptions),
    /// Resolve one standalone source package and verify or update its lockfile.
    Package {
        #[command(subcommand)]
        command: package::Command,
    },
    /// Create one deterministic standalone source project without replacing files.
    New(project::NewOptions),
    /// Compile one Zryna entrypoint into one atomic target bundle.
    Build(CompileOptions),
    /// Compile and invoke one scalar export, then commit one atomic target bundle.
    Run(RunOptions),
}

#[derive(Debug, Subcommand)]
enum ArchitectureCommand {
    /// Run the mandatory fail-closed architecture gate.
    Check(ArchitectureOptions),
}

#[derive(Clone, Debug, clap::Args)]
struct ArchitectureOptions {
    /// Workspace root.
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, clap::Args)]
struct CompileOptions {
    /// One portable workspace-relative .zry entrypoint.
    entrypoint: String,
    /// Explicit target selection.
    #[arg(long, value_enum)]
    target: CliTarget,
    /// Select an exact versioned profile; omission preserves M1.
    #[arg(long, value_enum)]
    profile: Option<CliProfile>,
    /// Source-checkout workspace root; defaults to the current directory.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Explicit standalone project root; omission preserves repository-local behavior.
    #[arg(long)]
    project_root: Option<PathBuf>,
    /// Portable output stem; defaults to the entrypoint stem.
    #[arg(long)]
    name: Option<String>,
    /// Absolute direct Node.js 22.22.1 executable.
    #[arg(long, required = !zryna_driver::distribution::InstalledCompiler::is_distribution_build())]
    node: Option<PathBuf>,
    /// Emit one versioned JSON response.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, clap::Args)]
struct RunOptions {
    #[command(flatten)]
    compile: CompileOptions,
    /// Exact logical scalar export.
    #[arg(long)]
    export: String,
    /// Ordered canonical typed argument, for example --arg=i32:42.
    #[arg(long = "arg", value_parser = profile::parse_scalar_argument)]
    arguments: Vec<ScalarValue>,
}

fn main() -> ExitCode {
    let cli = match parse_cli_from(std::env::args_os()) {
        Ok(cli) => cli,
        Err(error) => {
            let successful_display =
                matches!(error.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion);
            let _ = error.print();
            return if successful_display { ExitCode::SUCCESS } else { ExitCode::from(2) };
        }
    };
    match cli.command {
        Command::Architecture { command: ArchitectureCommand::Check(options) }
        | Command::Doctor(options) => run_architecture_check(&options),
        Command::Package { command } => package::run(command),
        Command::New(options) => project::create(&options),
        Command::Build(options) => run_build(options),
        Command::Run(options) => run_command(options),
    }
}

fn parse_cli_from<I, T>(arguments: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let arguments = arguments.into_iter().map(Into::into).collect::<Vec<_>>();
    let typed_scalars = profile::selects_typed_scalars(&arguments);
    if !typed_scalars {
        return Cli::try_parse_from(arguments);
    }
    let mut command = Cli::command();
    command = command.mut_subcommand("run", |run| {
        run.mut_arg("arguments", |argument| {
            argument.value_parser(profile::parse_control_flow_argument)
        })
    });
    let matches = command.try_get_matches_from(arguments)?;
    Cli::from_arg_matches(&matches)
}

fn run_architecture_check(options: &ArchitectureOptions) -> ExitCode {
    let root = match absolute_workspace_path(&options.root) {
        Ok(root) => root,
        Err(diagnostic) => {
            return render_cli_failure(CommandKind::Build, options.json, 2, &[diagnostic]);
        }
    };
    let report = zryna_driver::check_workspace(&root);
    if options.json {
        match serde_json::to_string_pretty(&report) {
            Ok(output) => println!("{output}"),
            Err(_) => return ExitCode::from(70),
        }
    } else if report.is_valid() {
        println!("Zryna architecture check passed");
    } else {
        for diagnostic in &report.diagnostics {
            eprintln!("{diagnostic}");
        }
    }
    if report.is_valid() { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

fn run_build(options: CompileOptions) -> ExitCode {
    if zryna_driver::distribution::InstalledCompiler::is_distribution_build() {
        return installed::execute(options, None);
    }
    let json_mode = options.json;
    let request = match build_request(options) {
        Ok(request) => request,
        Err(diagnostic) => {
            return render_cli_failure(CommandKind::Build, json_mode, 2, &[diagnostic]);
        }
    };
    let result = match request {
        ProfileBuildRequest::DataOwnershipV1(request) => {
            return ownership::render(
                zryna_driver::build_data_ownership_candidate(&request),
                CommandKind::Build,
                json_mode,
            );
        }
        ProfileBuildRequest::M1(request) => zryna_driver::build_workspace(&request),
        ProfileBuildRequest::Project(request) => zryna_driver::build_project(&request),
        ProfileBuildRequest::ControlFlowV1(request) => {
            zryna_driver::build_control_flow_workspace(&request)
        }
    };
    match result {
        Ok(success) => render_success(&success, json_mode),
        Err(failure) => render_failure(CommandKind::Build, json_mode, &failure),
    }
}

fn run_command(options: RunOptions) -> ExitCode {
    if zryna_driver::distribution::InstalledCompiler::is_distribution_build() {
        return installed::execute(options.compile, Some((options.export, options.arguments)));
    }
    let json_mode = options.compile.json;
    let export = options.export;
    let arguments = options.arguments;
    let build = match build_request(options.compile) {
        Ok(request) => request,
        Err(diagnostic) => {
            return render_cli_failure(CommandKind::Run, json_mode, 2, &[diagnostic]);
        }
    };
    let result = match build {
        ProfileBuildRequest::DataOwnershipV1(build) => {
            return ownership::render(
                zryna_driver::run_data_ownership_candidate(DataOwnershipRunRequest {
                    build,
                    logical_export: export,
                    arguments,
                }),
                CommandKind::Run,
                json_mode,
            );
        }
        ProfileBuildRequest::M1(build) => {
            zryna_driver::run_workspace(RunRequest { build, logical_export: export, arguments })
        }
        ProfileBuildRequest::Project(build) => {
            zryna_driver::run_project(zryna_driver::ProjectRunRequest {
                build,
                logical_export: export,
                arguments,
            })
        }
        ProfileBuildRequest::ControlFlowV1(build) => {
            zryna_driver::run_control_flow_workspace(ControlFlowRunRequest {
                build,
                logical_export: export,
                arguments,
            })
        }
    };
    match result {
        Ok(success) => render_success(&success, json_mode),
        Err(failure) => render_failure(CommandKind::Run, json_mode, &failure),
    }
}

enum ProfileBuildRequest {
    M1(BuildRequest),
    Project(zryna_driver::ProjectBuildRequest),
    ControlFlowV1(ControlFlowBuildRequest),
    DataOwnershipV1(DataOwnershipBuildRequest),
}

fn build_request(options: CompileOptions) -> Result<ProfileBuildRequest, Diagnostic> {
    let root = absolute_workspace_path(&options.root.unwrap_or_else(|| PathBuf::from(".")))?;
    let project_root = options.project_root.as_ref().map(absolute_workspace_path).transpose()?;
    let node = options.node.ok_or_else(cli_path_error)?;
    if !node.is_absolute() {
        return Err(cli_path_error());
    }
    let stem = options.name.unwrap_or_else(|| profile::default_stem(&options.entrypoint));
    let targets = if options.profile == Some(CliProfile::BrowserComponentV1) {
        if options.target != CliTarget::Component {
            return Err(Diagnostic::error(
                "ZRYNA-C1012",
                None,
                "browser-component-v1 requires the component build target",
                "use build --profile browser-component-v1 --target component",
            ));
        }
        TargetSelection::BrowserComponent
    } else {
        options.target.into()
    };
    if let Some(project_root) = project_root {
        if options.profile.is_some() {
            return Err(Diagnostic::error(
                "ZRYNA-C2001",
                None,
                "standalone projects currently require the manifest-declared i32-v1 profile",
                "omit --profile when using --project-root",
            ));
        }
        return Ok(ProfileBuildRequest::Project(zryna_driver::ProjectBuildRequest {
            compiler_root: root,
            project_root,
            entrypoint: options.entrypoint,
            artifact_stem: stem,
            targets,
            node_runtime: node,
        }));
    }
    Ok(match options.profile {
        Some(CliProfile::BrowserComponentV1) | None => ProfileBuildRequest::M1(BuildRequest {
            workspace_root: root,
            entrypoint: options.entrypoint,
            artifact_stem: stem,
            targets,
            node_runtime: node,
        }),
        Some(CliProfile::DataOwnershipV1) => {
            ProfileBuildRequest::DataOwnershipV1(DataOwnershipBuildRequest {
                workspace_root: root,
                entrypoint: options.entrypoint,
                artifact_stem: stem,
                targets,
                node_runtime: node,
            })
        }
        Some(CliProfile::ControlFlowV1) => {
            ProfileBuildRequest::ControlFlowV1(ControlFlowBuildRequest {
                workspace_root: root,
                entrypoint: options.entrypoint,
                artifact_stem: stem,
                targets,
                node_runtime: node,
            })
        }
    })
}

fn absolute_workspace_path(path: &PathBuf) -> Result<PathBuf, Diagnostic> {
    if path.is_absolute() {
        Ok(path.clone())
    } else {
        Ok(std::env::current_dir().map_err(|_| cli_path_error())?.join(path))
    }
}

fn cli_path_error() -> Diagnostic {
    profile::cli_path_error()
}

#[cfg(test)]
mod tests {
    use super::{CliProfile, Command, parse_cli_from, profile};
    use zryna_abi::ScalarValue;

    #[test]
    fn target_and_node_are_required() {
        let error = parse_cli_from(["zryna", "build", "src/main.zry"])
            .expect_err("target and node must be required");
        let rendered = error.to_string();
        assert!(rendered.contains("--target"));
        assert!(rendered.contains("--node"));
    }

    #[test]
    fn package_resolution_requires_an_explicit_mode() {
        let error = parse_cli_from(["zryna", "package", "resolve", "packages/app"])
            .expect_err("package lock mode must be explicit");
        assert!(error.to_string().contains("--mode"));
        assert!(matches!(
            parse_cli_from(["zryna", "package", "resolve", "packages/app", "--mode", "frozen"])
                .expect("package command")
                .command,
            Command::Package { .. }
        ));
    }

    #[test]
    fn scalar_arguments_are_canonical() {
        assert_eq!(
            profile::parse_scalar_argument("i32:-2147483648"),
            Ok(ScalarValue::I32(i32::MIN))
        );
        assert_eq!(
            profile::parse_scalar_argument("i32:2147483647"),
            Ok(ScalarValue::I32(i32::MAX))
        );
        for rejected in ["i32:+1", "i32:01", "i32:-0", "i32: 1", "bool:true"] {
            assert!(profile::parse_scalar_argument(rejected).is_err(), "{rejected}");
        }
    }

    #[test]
    fn control_flow_profile_and_boolean_arguments_are_explicit() {
        let cli = parse_cli_from([
            "zryna",
            "run",
            "src/main.zry",
            "--target",
            "javascript",
            "--profile",
            "control-flow-v1",
            "--node",
            "/node",
            "--export",
            "main",
            "--arg=bool:true",
        ])
        .expect("exact control-flow profile must parse");
        let Command::Run(options) = cli.command else {
            panic!("run command must parse");
        };
        assert_eq!(options.compile.profile, Some(CliProfile::ControlFlowV1));
        assert_eq!(options.arguments, [ScalarValue::Bool(true)]);
        assert_eq!(profile::parse_control_flow_argument("bool:true"), Ok(ScalarValue::Bool(true)));
        assert_eq!(
            profile::parse_control_flow_argument("bool:false"),
            Ok(ScalarValue::Bool(false))
        );
        assert_eq!(profile::parse_control_flow_argument("i32:-1"), Ok(ScalarValue::I32(-1)));
        for rejected in ["bool:True", "bool:1", "bool:false ", "bool:"] {
            assert!(profile::parse_control_flow_argument(rejected).is_err(), "{rejected}");
        }
    }

    #[test]
    fn omitted_profile_remains_m1_and_unknown_profiles_fail() {
        let cli = parse_cli_from([
            "zryna",
            "build",
            "src/main.zry",
            "--target",
            "javascript",
            "--node",
            "/node",
        ])
        .expect("legacy M1 command must still parse");
        let Command::Build(options) = cli.command else {
            panic!("build command must parse");
        };
        assert_eq!(options.profile, None);

        let error = parse_cli_from([
            "zryna",
            "build",
            "src/main.zry",
            "--target",
            "javascript",
            "--profile",
            "m2",
            "--node",
            "/node",
        ])
        .expect_err("profile aliases must fail");
        assert!(error.to_string().contains("control-flow-v1"));
    }

    #[test]
    fn component_build_target_is_explicit() {
        let cli = parse_cli_from([
            "zryna",
            "build",
            "src/main.zry",
            "--target",
            "component",
            "--node",
            "/node",
        ])
        .expect("component build target");
        let Command::Build(options) = cli.command else {
            panic!("build command must parse");
        };
        assert_eq!(options.target, super::CliTarget::Component);
    }
}
