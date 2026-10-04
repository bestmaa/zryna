//! Explicit, feature-gated source-workspace build selection.

use std::{path::PathBuf, process::ExitCode};

use clap::{Arg, ArgAction, ArgMatches, FromArgMatches};
use zryna_diagnostics::Diagnostic;
use zryna_driver::{
    CommandKind,
    distribution::InstalledCompiler,
    native_frontend::{NativeBuildProfile, NativeBuildRequest, NativeBuildSuccess},
};

use super::{Cli, CliProfile, CliTarget, CompileOptions, absolute_workspace_path, profile};

pub(super) fn command(command: clap::Command) -> clap::Command {
    command.mut_subcommand("build", |build| {
        let build = build.arg(
            Arg::new("native_frontend")
                .long("native-frontend")
                .hide(true)
                .action(ArgAction::SetTrue),
        );
        if InstalledCompiler::is_distribution_build() {
            build
        } else {
            build.mut_arg("node", |node| {
                node.required(false).required_unless_present("native_frontend")
            })
        }
    })
}

pub(super) fn from_matches(matches: &ArgMatches) -> Result<Cli, clap::Error> {
    let mut cli = Cli::from_arg_matches(matches)?;
    cli.native_frontend = matches
        .subcommand()
        .is_some_and(|(name, build)| name == "build" && build.get_flag("native_frontend"));
    Ok(cli)
}

pub(super) fn run(options: CompileOptions) -> ExitCode {
    let json_mode = options.json;
    let request = match request(options, InstalledCompiler::is_distribution_build()) {
        Ok(request) => request,
        Err(diagnostic) => {
            return super::render_cli_failure(CommandKind::Build, json_mode, 2, &[diagnostic]);
        }
    };
    match zryna_driver::native_frontend::build_workspace(&request) {
        Ok(NativeBuildSuccess::Scalar(success) | NativeBuildSuccess::ControlFlow(success)) => {
            super::render_success(&success, json_mode)
        }
        Ok(NativeBuildSuccess::Ownership(bundle)) => {
            super::ownership::render(Ok(bundle), CommandKind::Build, json_mode)
        }
        Err(failure) => super::render_failure(CommandKind::Build, json_mode, &failure),
    }
}

fn request(options: CompileOptions, installed: bool) -> Result<NativeBuildRequest, Diagnostic> {
    if installed || options.project_root.is_some() {
        return Err(Diagnostic::error(
            "ZRYNA-C2001",
            None,
            "the private native frontend build requires a source-checkout workspace",
            "omit --project-root and use a source compiler built with native-provider-internal",
        ));
    }
    if options.profile == Some(CliProfile::BrowserComponentV1)
        || options.target == CliTarget::Component
    {
        return Err(Diagnostic::error(
            "ZRYNA-C1013",
            None,
            "the private native frontend build does not admit browser components",
            "select javascript, webassembly, native or all with the default, control-flow-v1 or data-ownership-v1 profile",
        ));
    }
    Ok(NativeBuildRequest {
        workspace_root: absolute_workspace_path(
            &options.root.unwrap_or_else(|| PathBuf::from(".")),
        )?,
        artifact_stem: options.name.unwrap_or_else(|| profile::default_stem(&options.entrypoint)),
        entrypoint: options.entrypoint,
        targets: options.target.into(),
        profile: match options.profile {
            None => NativeBuildProfile::I32,
            Some(CliProfile::ControlFlowV1) => NativeBuildProfile::ControlFlow,
            Some(CliProfile::DataOwnershipV1) => NativeBuildProfile::DataOwnership,
            Some(CliProfile::BrowserComponentV1) => unreachable!("browser profile rejected above"),
        },
    })
}

#[cfg(test)]
#[path = "native_frontend_tests.rs"]
mod tests;
