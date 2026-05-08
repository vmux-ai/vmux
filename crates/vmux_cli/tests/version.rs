use assert_cmd::Command;

#[test]
fn version_flag_prints_workspace_version() {
    let mut cmd = Command::cargo_bin("vmux").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains(env!("CARGO_PKG_VERSION")));
}
