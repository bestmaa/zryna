use std::path::PathBuf;

use zryna_driver::{TargetSelection, native_frontend::NativeBuildProfile};

use super::super::{CliProfile, Command, parse_cli_from};
use super::request;

fn build_options(extra: &[&str]) -> super::CompileOptions {
    let mut args = vec!["zryna", "build", "src/main.zry", "--target", "javascript"];
    args.extend_from_slice(extra);
    let cli = parse_cli_from(args).expect("explicit private build should parse");
    assert!(cli.native_frontend);
    let Command::Build(options) = cli.command else { panic!("build command") };
    options
}

#[test]
fn native_frontend_explicit_build_does_not_require_node() {
    let options = build_options(&["--native-frontend"]);
    assert!(options.node.is_none());
    let request = request(options, false).expect("source build request");
    assert!(request.workspace_root.is_absolute());
    assert_eq!(request.entrypoint, "src/main.zry");
    assert_eq!(request.artifact_stem, "main");
    assert!(matches!(request.profile, NativeBuildProfile::I32));
    assert_eq!(request.targets, TargetSelection::JavaScript);
}

#[test]
fn native_frontend_selection_is_build_only_and_hidden() {
    let error = parse_cli_from([
        "zryna",
        "run",
        "src/main.zry",
        "--target",
        "javascript",
        "--export",
        "main",
        "--node",
        "/node",
        "--native-frontend",
    ])
    .expect_err("the private build flag cannot select a run route");
    assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    let mut command = super::command(<super::super::Cli as clap::CommandFactory>::command());
    let help = command.find_subcommand_mut("build").expect("build").render_long_help().to_string();
    assert!(!help.contains("--native-frontend"));
}

#[test]
fn ordinary_build_still_requires_node_and_keeps_bootstrap_selection() {
    let error = parse_cli_from(["zryna", "build", "src/main.zry", "--target", "javascript"])
        .expect_err("ordinary source builds require Node");
    assert!(error.to_string().contains("--node"));
    let cli = parse_cli_from([
        "zryna",
        "build",
        "src/main.zry",
        "--target",
        "javascript",
        "--node",
        "/node",
    ])
    .expect("ordinary bootstrap build");
    assert!(!cli.native_frontend);
    let Command::Build(options) = cli.command else { panic!("build command") };
    assert_eq!(options.node, Some(PathBuf::from("/node")));
}

#[test]
fn native_frontend_profiles_and_explicit_paths_preserve_request_values() {
    for (profile, expected) in [
        ("control-flow-v1", NativeBuildProfile::ControlFlow),
        ("data-ownership-v1", NativeBuildProfile::DataOwnership),
    ] {
        let options = build_options(&[
            "--native-frontend",
            "--profile",
            profile,
            "--root",
            ".",
            "--name",
            "private",
            "--node",
            "/node",
        ]);
        let request = request(options, false).expect("supported profile");
        assert_eq!(request.profile, expected);
        assert_eq!(request.artifact_stem, "private");
        assert!(request.workspace_root.is_absolute());
    }
}

#[test]
fn native_frontend_rejects_installed_and_standalone_project_routes() {
    let options = build_options(&["--native-frontend"]);
    let diagnostic = request(options, true).expect_err("installed native overrides denied");
    assert_eq!(diagnostic.code(), "ZRYNA-C2001");
    let options = build_options(&["--native-frontend", "--project-root", "project"]);
    let diagnostic = request(options, false).expect_err("standalone native route not admitted");
    assert_eq!(diagnostic.code(), "ZRYNA-C2001");
}

#[test]
fn native_frontend_rejects_component_profile_and_target() {
    let mut options = build_options(&["--native-frontend"]);
    options.profile = Some(CliProfile::BrowserComponentV1);
    assert_eq!(request(options, false).expect_err("browser profile").code(), "ZRYNA-C1013");
    let mut options = build_options(&["--native-frontend"]);
    options.target = super::super::CliTarget::Component;
    assert_eq!(request(options, false).expect_err("component target").code(), "ZRYNA-C1013");
}
