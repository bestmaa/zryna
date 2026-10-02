//! Exact command run grammar and typed execution-record rendering.

use super::{CliProfile, CliTarget, CompileOptions, RunOptions, absolute_workspace_path};
use std::{path::PathBuf, process::ExitCode};
use zryna_diagnostics::Diagnostic;
use zryna_driver::{
    CommandH1Outcome, CommandH1RunRequest, CommandH1RunReturn, CommandH1Teardown, CommandKind,
    distribution::InstalledCompiler, run_command_h1_workspace,
};

pub(super) fn selected(options: &CompileOptions) -> bool {
    options.profile == Some(CliProfile::CommandH1V1) || options.target == CliTarget::WasiCommand
}

pub(super) fn configuration_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-C4103",
        None,
        "Command execution requires the exact source-checkout command profile and sole main export.",
        "Use run ENTRY --target wasi-command --profile command-h1-v1 --export main --node PINNED; omit arguments and project-root.",
    )
}

fn request(options: &RunOptions) -> Result<CommandH1RunRequest, Diagnostic> {
    let compile = &options.compile;
    if compile.profile != Some(CliProfile::CommandH1V1)
        || compile.target != CliTarget::WasiCommand
        || compile.project_root.is_some()
        || options.export != "main"
        || !options.arguments.is_empty()
        || InstalledCompiler::is_distribution_build()
    {
        return Err(configuration_error());
    }
    let root =
        absolute_workspace_path(&compile.root.clone().unwrap_or_else(|| PathBuf::from(".")))?;
    let node = compile.node.clone().ok_or_else(configuration_error)?;
    if !node.is_absolute() || options.grant_file.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err(configuration_error());
    }
    Ok(CommandH1RunRequest {
        workspace_root: root,
        entrypoint: compile.entrypoint.clone(),
        artifact_stem: compile
            .name
            .clone()
            .unwrap_or_else(|| super::profile::default_stem(&compile.entrypoint)),
        node_runtime: node,
        grant_file: options.grant_file.clone(),
    })
}

pub(super) fn run(options: &RunOptions) -> ExitCode {
    let request = match request(options) {
        Ok(request) => request,
        Err(error) => {
            return super::render_cli_failure(CommandKind::Run, options.compile.json, 2, &[error]);
        }
    };
    let result = match run_command_h1_workspace(&request) {
        Ok(result) => result,
        Err(error) => return super::render_failure(CommandKind::Run, options.compile.json, &error),
    };
    if options.compile.json {
        let manifest = format!(
            ".zryna/out/{}.wasi-command-run/{}",
            request.artifact_stem,
            zryna_driver::COMMAND_H1_MANIFEST_NAME,
        );
        let response = serde_json::json!({
            "version": 1, "command": "run", "profile": "command-h1-v1", "target": "wasi-command",
            "ok": result.record().succeeded(), "manifest": manifest,
            "execution": result.record(), "diagnostics": result.diagnostics(),
        });
        match serde_json::to_string_pretty(&response) {
            Ok(response) => println!("{response}"),
            Err(_) => return ExitCode::from(70),
        }
    } else {
        match result.record().outcome() {
            CommandH1Outcome::RunReturned { result: CommandH1RunReturn::Ok } => {
                println!("wasi-command: run ok");
            }
            CommandH1Outcome::RunReturned { result: CommandH1RunReturn::Err } => {
                println!("wasi-command: run err");
            }
            CommandH1Outcome::HostDenial { interface, operation } => {
                println!("wasi-command: permission denied at {interface}/{operation}");
            }
            CommandH1Outcome::RuntimeTrap { category, identity } => {
                println!("wasi-command: trapped {category:?}");
                if let Some(identity) = identity {
                    println!("trap identity: {identity}");
                }
            }
        }
        println!("{}", result.manifest_path().display());
        for diagnostic in result.diagnostics() {
            eprintln!("{diagnostic}");
        }
    }
    if result.record().teardown() == CommandH1Teardown::Unconfirmed {
        ExitCode::from(6)
    } else if result.record().succeeded() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(5)
    }
}

#[cfg(test)]
mod tests;
