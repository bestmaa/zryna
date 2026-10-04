use super::parse_cli_from;

#[test]
fn native_frontend_flag_is_unavailable_in_default_builds() {
    let error = parse_cli_from([
        "zryna",
        "build",
        "src/main.zry",
        "--target",
        "javascript",
        "--node",
        "/node",
        "--native-frontend",
    ])
    .expect_err("default builds cannot select the private frontend");
    assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
}
