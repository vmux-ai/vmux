use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn mcp_initialize_returns_server_info() {
    let stdin = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\"}}\n";
    let mut cmd = Command::cargo_bin("vmux").unwrap();
    cmd.arg("mcp")
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(contains("\"name\":\"vmux\""))
        .stdout(contains("\"protocolVersion\":\"2025-11-25\""));
}

#[test]
fn mcp_tools_list_returns_tool_definitions() {
    let stdin = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n";
    let mut cmd = Command::cargo_bin("vmux").unwrap();
    cmd.arg("mcp")
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(contains("\"tools\""));
}
