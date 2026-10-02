use super::*;

#[test]
fn stdio_mcp_server_with_working_directory_is_rejected() {
    assert!(
        AcpMcpServers::from_managed(ManagedMcpServer {
            name: "local".into(),
            transport: ManagedMcpTransport::Stdio,
            command: Some("server".into()),
            args: Vec::new(),
            env: Vec::new(),
            cwd: Some("/tmp/project".into()),
            url: None,
            headers: Vec::new(),
        })
        .is_none()
    );
}
