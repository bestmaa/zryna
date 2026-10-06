//! Test-only native installation consumer: no CLI compiler or runtime dispatch.

mod mutations;

use serde_json::{Value, json};
use std::path::PathBuf;
use zryna_driver::{
    ModuleClosureError, PackageLockMode, PackageResolutionRequest,
    distribution::native_installation::NativeInstallation, resolve_package,
};
use zryna_source::NormalizedSourcePath;

struct Failure {
    phase: &'static str,
    diagnostics: Value,
    mutation: Option<Value>,
    setup: bool,
}

fn failure(phase: &'static str, diagnostics: Value, mutation: &Option<Value>) -> Failure {
    Failure { phase, diagnostics, mutation: mutation.clone(), setup: false }
}

fn syntax_failure(
    phase: &'static str,
    error: &ModuleClosureError,
    mutation: &Option<Value>,
) -> Failure {
    failure(phase, json!(error.diagnostics()), mutation)
}

fn execute() -> Result<Value, Failure> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err(Failure {
            phase: "probe-setup",
            diagnostics: json!("four exact test arguments required"),
            mutation: None,
            setup: true,
        });
    }
    let source_root = PathBuf::from(&args[0]);
    let outside = PathBuf::from(&args[1]);
    let scenario = args[2].as_str();
    let protocol = args[3].parse::<u8>().map_err(|_| Failure {
        phase: "probe-setup",
        diagnostics: json!("invalid protocol"),
        mutation: None,
        setup: true,
    })?;
    let mut mutation = None;
    let installation = NativeInstallation::capture_current()
        .map_err(|error| failure("installation-capture", json!([error]), &mutation))?;
    let current = std::env::current_exe()
        .map_err(|error| failure("probe-setup", json!(error.to_string()), &mutation))?;
    let root = current
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or_else(|| failure("probe-setup", json!("missing installation root"), &mutation))?;
    let mut request = PackageResolutionRequest {
        source_root,
        package: "packages/app".to_owned(),
        git_cache: None,
        mode: PackageLockMode::Update,
    };
    let resolved = resolve_package(&request).map_err(|error| {
        failure(
            "package-setup",
            json!({"code": error.code(), "message": error.to_string()}),
            &mutation,
        )
    })?;
    request.mode = PackageLockMode::Frozen;
    if scenario == "non-frozen-request" {
        request.mode = PackageLockMode::Update;
    }
    let package_id =
        if scenario == "wrong-package-identity" { "unadmitted" } else { resolved.graph().root() };
    if scenario.starts_with("descriptor-replaced")
        || scenario.starts_with("license-replaced")
        || scenario.starts_with("executable-path-replaced")
        || scenario.starts_with("installation-parent-replaced")
        || scenario == "extra-file-after-capture"
    {
        mutation =
            Some(mutations::perform(scenario, root, &request.source_root, &outside).map_err(
                |error| Failure {
                    phase: "mutation-setup",
                    diagnostics: json!(error.to_string()),
                    mutation: None,
                    setup: true,
                },
            )?);
    }
    let entry = NormalizedSourcePath::new("main.zry")
        .map_err(|error| failure("probe-setup", json!(error.to_string()), &mutation))?;
    let sources = installation
        .capture_package_sources(&request, package_id, entry)
        .map_err(|error| syntax_failure("source-capture", &error, &mutation))?;
    if scenario.starts_with("source-")
        || scenario.starts_with("package-manifest-")
        || scenario.starts_with("package-lock-")
    {
        mutation =
            Some(mutations::perform(scenario, root, &request.source_root, &outside).map_err(
                |error| Failure {
                    phase: "mutation-setup",
                    diagnostics: json!(error.to_string()),
                    mutation: None,
                    setup: true,
                },
            )?);
    }
    let syntax = sources
        .verify(protocol)
        .map_err(|error| syntax_failure("syntax-verification", &error, &mutation))?;
    if scenario == "callback-license-change" || scenario == "callback-error-license-change" {
        let mut setup_error = None;
        let result = syntax.with_verified(|_| {
            mutation =
                Some(mutations::perform(scenario, root, &request.source_root, &outside).map_err(
                    |error| {
                        setup_error = Some(error.to_string());
                        ModuleClosureError::Rejected(vec![zryna_diagnostics::Diagnostic::error(
                            "PROBE-SETUP",
                            None,
                            error.to_string(),
                            "retain the exclusively owned test fixture",
                        )])
                    },
                )?);
            if scenario == "callback-error-license-change" {
                return Err(ModuleClosureError::Rejected(vec![
                    zryna_diagnostics::Diagnostic::error(
                        "PROBE-CONSUMER",
                        None,
                        "deliberate callback error",
                        "test-only negative control",
                    ),
                ]));
            }
            Ok(())
        });
        if let Some(error) = setup_error {
            return Err(Failure {
                phase: "mutation-setup",
                diagnostics: json!(error),
                mutation,
                setup: true,
            });
        }
        result.map_err(|error| syntax_failure("consumer-postcheck", &error, &mutation))?;
    }
    let summary = syntax
        .with_verified(|syntax| syntax.summary())
        .map_err(|error| syntax_failure("verified-summary", &error, &mutation))?;
    installation
        .revalidate()
        .map_err(|error| failure("final-installation-check", json!([error]), &mutation))?;
    Ok(json!({"status": "verified", "summary": summary, "mutation": mutation,
        "lock_path": resolved.lock_path(), "public_default_acceptance": false,
        "runtime_or_backend_execution": false}))
}

fn main() {
    match execute() {
        Ok(value) => println!("{value}"),
        Err(error) => {
            println!(
                "{}",
                json!({"status": if error.setup { "setup-failed" } else { "rejected" },
                "phase": error.phase, "diagnostics": error.diagnostics, "mutation": error.mutation})
            );
            std::process::exit(if error.setup { 3 } else { 2 });
        }
    }
}
