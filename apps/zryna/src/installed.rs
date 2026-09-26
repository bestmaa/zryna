//! Installed command routing always constructs the running compiler's retained proof.

use super::{
    CliProfile, CompileOptions, absolute_workspace_path, ownership, profile, render_cli_failure,
    render_failure, render_success,
};
use std::{path::PathBuf, process::ExitCode};
use zryna_abi::ScalarValue;
use zryna_diagnostics::Diagnostic;
use zryna_driver::{
    CommandKind,
    distribution::{
        InstalledBuildRequest, InstalledCommandSuccess, InstalledCompiler, InstalledProfile,
    },
};

pub(super) fn execute(
    options: CompileOptions,
    run: Option<(String, Vec<ScalarValue>)>,
) -> ExitCode {
    let command = if run.is_some() { CommandKind::Run } else { CommandKind::Build };
    let json = options.json;
    let installation = match InstalledCompiler::capture_current() {
        Ok(installation) => installation,
        Err(error) => return render_cli_failure(command, json, 2, &[error]),
    };
    let request = match request(options) {
        Ok(request) => request,
        Err(error) => return render_cli_failure(command, json, 2, &[error]),
    };
    let result = match run {
        None => installation.build(&request),
        Some((export, arguments)) => installation.run(&request, export, arguments),
    };
    match result {
        Ok(InstalledCommandSuccess::Scalar(success)) => render_success(&success, json),
        Ok(InstalledCommandSuccess::Ownership(success)) => {
            ownership::render(Ok(success), command, json)
        }
        Err(error) => render_failure(command, json, &error),
    }
}

fn request(options: CompileOptions) -> Result<InstalledBuildRequest, Diagnostic> {
    if options.profile == Some(CliProfile::BrowserComponentV1) {
        return Err(Diagnostic::error(
            "ZRYNA-C2001",
            None,
            "installed package compatibility does not include browser components",
            "use the repository-local source build until package compatibility is specified",
        ));
    }
    if options.root.is_some() || options.node.is_some() {
        return Err(Diagnostic::error(
            "ZRYNA-C2001",
            None,
            "installed commands do not accept compiler-root or Node.js overrides",
            "select the project with --project-root; the installation supplies its runtime",
        ));
    }
    Ok(InstalledBuildRequest {
        project_root: absolute_workspace_path(
            &options.project_root.unwrap_or_else(|| PathBuf::from(".")),
        )?,
        artifact_stem: options.name.unwrap_or_else(|| profile::default_stem(&options.entrypoint)),
        entrypoint: options.entrypoint,
        targets: options.target.into(),
        profile: match options.profile {
            None => InstalledProfile::I32,
            Some(CliProfile::ControlFlowV1) => InstalledProfile::ControlFlow,
            Some(CliProfile::DataOwnershipV1) => InstalledProfile::DataOwnership,
            Some(CliProfile::BrowserComponentV1) => {
                unreachable!("rejected before request construction")
            }
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> CompileOptions {
        CompileOptions {
            entrypoint: "src/main.zry".to_owned(),
            target: crate::CliTarget::JavaScript,
            profile: None,
            root: None,
            project_root: Some(std::env::temp_dir().join("installed-project")),
            name: None,
            node: None,
            json: false,
        }
    }

    #[test]
    fn installed_request_rejects_explicit_runtime_and_compiler_root_selectors() {
        let mut runtime = options();
        runtime.node = Some(std::env::temp_dir().join("node"));
        assert!(request(runtime).is_err());
        let mut root = options();
        root.root = Some(PathBuf::from("."));
        assert!(request(root).is_err());
    }

    #[test]
    fn project_selection_does_not_select_the_runtime_or_compiler_root() {
        let request = request(options()).expect("project-only request");
        assert_eq!(request.project_root, std::env::temp_dir().join("installed-project"));
        assert_eq!(request.profile, InstalledProfile::I32);
        assert_eq!(request.entrypoint, "src/main.zry");
    }
}
