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

#[test]
fn mcp_tools_list_includes_layout_tools() {
    let stdin = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n";
    let mut cmd = Command::cargo_bin("vmux").unwrap();
    cmd.arg("mcp")
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(contains("\"read_layout\""))
        .stdout(contains("\"update_layout\""));
}

#[test]
fn mcp_tools_list_excludes_legacy_layout_tools() {
    let stdin = "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/list\"}\n";
    let mut cmd = Command::cargo_bin("vmux").unwrap();
    let assert = cmd.arg("mcp").write_stdin(stdin).assert().success();
    let out = assert.get_output().stdout.clone();
    let s = String::from_utf8_lossy(&out);
    for legacy in [
        "\"name\":\"split_v\"",
        "\"name\":\"split_h\"",
        "\"name\":\"close_pane\"",
        "\"name\":\"new_tab\"",
        "\"name\":\"tab_select_1\"",
        "\"name\":\"stack_new\"",
        "\"name\":\"get_state\"",
        "\"name\":\"list_tabs\"",
        "\"name\":\"list_terminals\"",
    ] {
        assert!(!s.contains(legacy), "tool list still contains {legacy}");
    }
}
