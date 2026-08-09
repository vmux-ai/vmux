use super::*;

#[test]
fn claude_projection_resolves_remote_headers() {
    let server = McpServerManifest {
        transport: McpTransport::Http,
        command: None,
        args: Vec::new(),
        env: BTreeMap::new(),
        cwd: None,
        url: Some("https://example.com/mcp".to_string()),
        headers: BTreeMap::from([("X-Key".to_string(), "value".to_string())]),
        header_env: BTreeMap::new(),
        bearer_token_env_var: None,
    };

    assert_eq!(
        claude_value(&server),
        serde_json::json!({
            "type": "http",
            "url": "https://example.com/mcp",
            "headers": {"X-Key": "value"}
        })
    );
}

#[test]
fn acp_projection_preserves_stdio_launch_configuration() {
    let server = McpServerManifest {
        transport: McpTransport::Stdio,
        command: Some("npx".to_string()),
        args: vec!["-y".to_string(), "server".to_string()],
        env: BTreeMap::from([("MODE".to_string(), "local".to_string())]),
        cwd: Some("/tmp/project".to_string()),
        url: None,
        headers: BTreeMap::new(),
        header_env: BTreeMap::new(),
        bearer_token_env_var: None,
    };

    assert_eq!(
        acp_server("local".to_string(), server),
        ManagedMcpServer {
            name: "local".to_string(),
            transport: ManagedMcpTransport::Stdio,
            command: Some("npx".to_string()),
            args: vec!["-y".to_string(), "server".to_string()],
            env: vec![("MODE".to_string(), "local".to_string())],
            cwd: Some("/tmp/project".to_string()),
            url: None,
            headers: Vec::new(),
        }
    );
}
