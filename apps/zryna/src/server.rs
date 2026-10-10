//! Explicit source-checkout server route and bounded newline-delimited public observations.

use clap::Args;
use serde_json::json;
use std::{
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};
use zryna_diagnostics::Diagnostic;
use zryna_driver::{ServerRunRequest, serve_workspace};

#[derive(Clone, Debug, Args)]
pub(super) struct Options {
    /// One portable workspace-relative .zry source with a sole no-argument i32 export.
    entrypoint: String,
    /// Exact bounded pure-source server selection.
    #[arg(long, value_parser = ["server-status-v1"])]
    profile: String,
    /// Sole status export.
    #[arg(long)]
    export: String,
    /// Real compiler workspace root.
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Exact absolute pinned Node executable used only for syntax authentication.
    #[arg(long)]
    node: PathBuf,
    /// Absolute caller-private loopback configuration.
    #[arg(long)]
    server_config: PathBuf,
    /// Absolute caller-private root approval bound to exact configuration bytes.
    #[arg(long)]
    listener_approval: PathBuf,
    /// Fresh portable output stem.
    #[arg(long)]
    name: String,
    /// Emit one readiness line and one final result line as JSON.
    #[arg(long)]
    json: bool,
}

pub(super) fn run(options: &Options) -> ExitCode {
    if zryna_driver::distribution::InstalledCompiler::is_distribution_build() {
        let error = Diagnostic::error(
            "ZRYNA-C4201",
            None,
            "Installed distributions do not advertise server-status-v1.",
            "Use the source-checkout workflow with its explicit root and pinned Node.",
        );
        return rejected(options, &error);
    }
    let root = match super::absolute_workspace_path(&options.root) {
        Ok(root) => root,
        Err(error) => return rejected(options, &error),
    };
    let result = {
        let request = ServerRunRequest {
            workspace_root: root,
            entrypoint: options.entrypoint.clone(),
            export: options.export.clone(),
            artifact_stem: options.name.clone(),
            node_runtime: options.node.clone(),
            configuration: options.server_config.clone(),
            listener_approval: options.listener_approval.clone(),
        };
        serve_workspace(&request, |ready| {
            let message = if options.json {
                json!({"version":1,"command":"serve","kind":"ready","profile":options.profile,
                    "endpoint":ready.address().to_string()})
                .to_string()
            } else {
                format!("server-status-v1 ready at {}", ready.address())
            };
            let mut output = io::stdout().lock();
            writeln!(output, "{message}").and_then(|()| output.flush()).map_err(|_| {
                Diagnostic::error(
                    "ZRYNA-C4204",
                    None,
                    "Server readiness output failed.",
                    "Keep the bounded output channel open.",
                )
            })
        })
    };
    match result {
        Ok(bundle) => {
            let execution = &bundle.manifest()["execution"];
            let ok = execution["outcome"] == "attempts_exhausted";
            let manifest = format!(
                ".zryna/out/{}.wasi-server-run/zryna-wasi-server-manifest-v1.json",
                options.name
            );
            if options.json {
                println!(
                    "{}",
                    json!({"version":1,"command":"serve","kind":"result","profile":options.profile,
                    "ok":ok,"manifest":manifest,"execution":execution})
                );
            } else {
                println!(
                    "server-status-v1: {}",
                    execution["outcome"].as_str().unwrap_or("host_failure")
                );
                println!("{manifest}");
            }
            if ok { ExitCode::SUCCESS } else { ExitCode::from(5) }
        }
        Err(error) => {
            let exit = error.kind().exit_code();
            if options.json {
                println!(
                    "{}",
                    json!({"version":1,"command":"serve","kind":"result","ok":false,
                "profile":options.profile,"diagnostics":error.diagnostics()})
                );
            } else {
                for diagnostic in error.diagnostics() {
                    eprintln!("{diagnostic}");
                }
            }
            ExitCode::from(exit)
        }
    }
}

fn rejected(options: &Options, error: &Diagnostic) -> ExitCode {
    if options.json {
        println!(
            "{}",
            json!({"version":1,"command":"serve","kind":"result","ok":false,"diagnostics":[error]})
        );
    } else {
        eprintln!("{error}");
    }
    ExitCode::from(2)
}
