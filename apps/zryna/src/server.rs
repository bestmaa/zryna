//! Explicit source-checkout server route and bounded newline-delimited public observations.

use clap::Args;
use serde_json::json;
use std::{
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};
use zryna_diagnostics::Diagnostic;
use zryna_driver::{
    ClockServerRunRequest, ServerReadiness, ServerRunRequest, serve_clock_workspace,
    serve_workspace,
};

#[derive(Clone, Debug, Args)]
pub(super) struct Options {
    /// One portable workspace-relative .zry source with a sole no-argument i32 export.
    entrypoint: String,
    /// Exact bounded server arrangement.
    #[arg(long, value_parser = ["server-status-v1", "server-clock-status-v1"])]
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
    /// Absolute caller-private one-read request for the clock profile.
    #[arg(long)]
    guest_request: Option<PathBuf>,
    /// Separate caller-private root approval for that exact guest request.
    #[arg(long)]
    guest_approval: Option<PathBuf>,
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
            "Installed distributions do not advertise bounded source-checkout server profiles.",
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
        let ready = |ready| publish_readiness(options, &ready);
        match (options.profile.as_str(), &options.guest_request, &options.guest_approval) {
            ("server-status-v1", None, None) => serve_workspace(&request, ready),
            ("server-clock-status-v1", Some(guest), Some(approval)) => serve_clock_workspace(
                &ClockServerRunRequest {
                    server: request,
                    guest_request: guest.clone(),
                    guest_approval: approval.clone(),
                },
                ready,
            ),
            _ => {
                return rejected(
                    options,
                    &Diagnostic::error(
                        "ZRYNA-C4201",
                        None,
                        "Guest input selection does not match the exact server profile.",
                        "Use both private guest files only with server-clock-status-v1.",
                    ),
                );
            }
        }
    };
    match result {
        Ok(bundle) => {
            let execution = &bundle.manifest()["execution"];
            let ok = execution["outcome"] == "attempts_exhausted";
            let manifest =
                format!(".zryna/out/{}.wasi-server-run/{}", options.name, bundle.manifest_name());
            if options.json {
                println!(
                    "{}",
                    json!({"version":1,"command":"serve","kind":"result","profile":options.profile,
                    "ok":ok,"manifest":manifest,"execution":execution})
                );
            } else {
                println!(
                    "{}: {}",
                    options.profile,
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

fn publish_readiness(options: &Options, ready: &ServerReadiness) -> Result<(), Diagnostic> {
    let message = if options.json {
        json!({"version":1,"command":"serve","kind":"ready","profile":options.profile,
            "endpoint":ready.address().to_string()})
        .to_string()
    } else {
        format!("{} ready at {}", options.profile, ready.address())
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
