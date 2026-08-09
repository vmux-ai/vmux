use super::*;

#[test]
fn terminal_launch_plain_construction() {
    let launch = TerminalLaunch {
        command: "/bin/zsh".to_string(),
        args: vec![],
        cwd: "/tmp".to_string(),
        env: vec![],
        kind: TerminalKind::Plain,
    };
    assert_eq!(launch.kind, TerminalKind::Plain);
    assert!(launch.args.is_empty());
}

#[test]
fn terminal_launch_vibe_with_resume_args() {
    let launch = TerminalLaunch {
        command: "/usr/local/bin/vibe".to_string(),
        args: vec!["--trust".into(), "--resume".into(), "abc-123".into()],
        cwd: "/work".to_string(),
        env: vec![("VIBE_MCP_SERVERS".into(), "[]".into())],
        kind: TerminalKind::Vibe,
    };
    assert_eq!(launch.kind, TerminalKind::Vibe);
    assert_eq!(launch.args.len(), 3);
    assert!(launch.env.iter().any(|(k, _)| k == "VIBE_MCP_SERVERS"));
}
