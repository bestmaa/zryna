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
    assert_eq!(profile::parse_scalar_argument("i32:-2147483648"), Ok(ScalarValue::I32(i32::MIN)));
    assert_eq!(profile::parse_scalar_argument("i32:2147483647"), Ok(ScalarValue::I32(i32::MAX)));
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
    assert_eq!(profile::parse_control_flow_argument("bool:false"), Ok(ScalarValue::Bool(false)));
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
