use super::*;

fn options() -> RunOptions {
    let node = std::env::current_exe().expect("absolute test executable");
    let cli = crate::parse_cli_from(vec![
        "zryna".into(),
        "run".into(),
        "src/main.zry".into(),
        "--target".into(),
        "wasi-command".into(),
        "--profile".into(),
        "command-h1-v1".into(),
        "--export".into(),
        "main".into(),
        "--node".into(),
        node.into_os_string(),
    ])
    .expect("exact command grammar");
    let crate::Command::Run(options) = cli.command else { panic!("run") };
    options
}

#[test]
fn command_grammar_and_explicit_file_preserve_exact_request() {
    let mut options = options();
    assert!(selected(&options.compile));
    let empty = request(&options).expect("pure request");
    assert_eq!(empty.entrypoint, "src/main.zry");
    assert!(empty.grant_file.is_none());
    let file = std::env::temp_dir().join("private-command-request.json");
    options.grant_file = Some(file.clone());
    assert_eq!(request(&options).expect("explicit file").grant_file, Some(file));
}

#[test]
fn command_cannot_fall_through_to_scalar_or_installed_profiles() {
    assert!(CliTarget::WasiCommand.ordinary().is_err());
    let base = options();
    let mut wrong = base.clone();
    wrong.compile.profile = None;
    assert!(request(&wrong).is_err());
    wrong = base.clone();
    wrong.compile.target = CliTarget::Component;
    assert!(request(&wrong).is_err());
    wrong = base.clone();
    wrong.export = "other".into();
    assert!(request(&wrong).is_err());
    wrong = base.clone();
    wrong.arguments.push(zryna_abi::ScalarValue::I32(1));
    assert!(request(&wrong).is_err());
    wrong = base.clone();
    wrong.compile.project_root = Some(std::env::temp_dir());
    assert!(request(&wrong).is_err());
    wrong = base;
    wrong.grant_file = Some(PathBuf::from("private.json"));
    assert!(request(&wrong).is_err());
}

#[test]
fn grant_file_is_a_run_only_closed_flag() {
    assert!(
        crate::parse_cli_from([
            "zryna",
            "build",
            "src/main.zry",
            "--target",
            "wasi-command",
            "--profile",
            "command-h1-v1",
            "--node",
            "/node",
            "--grant-file",
            "/private.json",
        ])
        .is_err()
    );
}
