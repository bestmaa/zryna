//! Standard-input entry point for the bounded Zryna language server.

use std::{env, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 1 && args[0] == "--version" {
        println!(
            "zryna-language-server {} portable-setup-v1 {}",
            env!("CARGO_PKG_VERSION"),
            option_env!("ZRYNA_TOOLING_SOURCE_COMMIT").unwrap_or("source-build")
        );
        return ExitCode::SUCCESS;
    }
    let result = arguments().and_then(|arguments| match arguments {
        ServerArguments::Installed { root, workspace: None } => {
            zryna_language_server::run_installed_stdio(&root)
        }
        ServerArguments::Installed { root, workspace: Some(workspace) } => {
            zryna_language_server::run_installed_stdio_with_workspace_root(&root, &workspace)
        }
        ServerArguments::Source { root, node, workspace: None } => {
            zryna_language_server::run_stdio(&root, &node)
        }
        ServerArguments::Source { root, node, workspace: Some(workspace) } => {
            zryna_language_server::run_stdio_with_workspace_root(&root, &node, &workspace)
        }
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("zryna-language-server: {error}");
            ExitCode::from(1)
        }
    }
}

enum ServerArguments {
    Installed { root: PathBuf, workspace: Option<PathBuf> },
    Source { root: PathBuf, node: PathBuf, workspace: Option<PathBuf> },
}

fn arguments() -> Result<ServerArguments, String> {
    let mut root = None;
    let mut node = None;
    let mut installed = None;
    let mut workspace = None;
    let mut arguments = env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        let value = arguments.next().ok_or_else(|| "every option requires one value".to_owned())?;
        match argument.to_str() {
            Some("--compiler-root") if root.is_none() => root = Some(PathBuf::from(value)),
            Some("--node") if node.is_none() => node = Some(PathBuf::from(value)),
            Some("--installed-root") if installed.is_none() => {
                installed = Some(PathBuf::from(value));
            }
            Some("--workspace-root") if workspace.is_none() => {
                workspace = Some(PathBuf::from(value));
            }
            _ => return Err("invalid or duplicate server option".to_owned()),
        }
    }
    if workspace.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err("workspace root must be absolute".to_owned());
    }
    match (installed, root, node) {
        (Some(root), None, None) if root.is_absolute() => {
            Ok(ServerArguments::Installed { root, workspace })
        }
        (None, Some(root), Some(node)) if root.is_absolute() && node.is_absolute() => {
            Ok(ServerArguments::Source { root, node, workspace })
        }
        _ => Err("expected --installed-root or --compiler-root with --node, using absolute paths"
            .to_owned()),
    }
}
