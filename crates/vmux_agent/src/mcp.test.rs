use super::*;

#[test]
fn mcp_args_always_append_profile() {
    let anchor = ProcessId::new();
    for profile in ["personal", "gregor"] {
        let args = mcp_subcommand_args(anchor, profile, false, false, DEFAULT_RUN_TIMEOUT_SECS);
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--profile" && w[1] == profile)
        );
    }
}

#[test]
fn acp_args_append_acp_terminals_flag() {
    let anchor = ProcessId::new();
    let plain = mcp_subcommand_args(anchor, "personal", false, false, DEFAULT_RUN_TIMEOUT_SECS);
    let acp = mcp_subcommand_args(anchor, "personal", true, true, DEFAULT_RUN_TIMEOUT_SECS);
    assert!(!plain.iter().any(|a| a == "--acp-session"));
    assert!(acp.iter().any(|a| a == "--acp-session"));
    assert!(!plain.iter().any(|a| a == "--acp-terminals"));
    assert!(acp.iter().any(|a| a == "--acp-terminals"));
}

#[test]
fn compatibility_acp_agents_keep_vmux_terminal_tools() {
    assert!(!acp_uses_native_terminals("codex"));
    assert!(!acp_uses_native_terminals("codex-acp"));
    assert!(!acp_uses_native_terminals("claude"));
    assert!(!acp_uses_native_terminals("claude-acp"));
    assert!(!acp_uses_native_terminals("mistral-vibe"));
    assert!(!acp_uses_native_terminals("vibe"));
    assert!(acp_uses_native_terminals("vibe-acp"));
}

#[test]
fn codex_and_claude_use_long_run_timeout() {
    assert_eq!(
        run_timeout_secs_for_kind(AgentKind::Codex),
        LONG_RUN_TIMEOUT_SECS
    );
    assert_eq!(
        run_timeout_secs_for_kind(AgentKind::Claude),
        LONG_RUN_TIMEOUT_SECS
    );
    assert_eq!(
        run_timeout_secs_for_agent_id("codex"),
        LONG_RUN_TIMEOUT_SECS
    );
    assert_eq!(
        run_timeout_secs_for_agent_id("claude"),
        LONG_RUN_TIMEOUT_SECS
    );
}

#[test]
fn vibe_keeps_default_run_timeout() {
    assert_eq!(
        run_timeout_secs_for_kind(AgentKind::Vibe),
        DEFAULT_RUN_TIMEOUT_SECS
    );
    assert_eq!(
        run_timeout_secs_for_agent_id("mistral-vibe"),
        DEFAULT_RUN_TIMEOUT_SECS
    );
}

#[test]
fn falls_back_to_cargo_run_when_sidecar_is_missing() {
    let temp = std::env::temp_dir().join(format!("vmux-agent-mcp-{}", std::process::id()));
    let workspace = temp.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(workspace.join("Cargo.toml"), b"[workspace]\n").unwrap();

    let anchor = ProcessId::new();
    let config = resolve_with_sidecar(
        &temp.join("missing-vmux"),
        &workspace,
        anchor,
        "personal",
        false,
        false,
        DEFAULT_RUN_TIMEOUT_SECS,
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&temp);

    assert_eq!(config.command, "cargo");
    assert_eq!(
        config.args,
        vec![
            "run",
            "--quiet",
            "-p",
            "vmux_cli",
            "--bin",
            "vmux",
            "--",
            "mcp",
            "--anchor",
            &anchor.to_string(),
            "--profile",
            "personal",
            "--run-timeout-secs",
            "50"
        ]
    );
    assert_eq!(config.cwd, Some(workspace));
}

#[test]
fn resolve_appends_anchor_to_args() {
    let temp = std::env::temp_dir().join(format!("vmux-anchor-{}", std::process::id()));
    let workspace = temp.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(workspace.join("Cargo.toml"), b"[workspace]\n").unwrap();

    let anchor = ProcessId::new();
    let config = resolve_with_sidecar(
        &temp.join("missing-vmux"),
        &workspace,
        anchor,
        "personal",
        false,
        false,
        DEFAULT_RUN_TIMEOUT_SECS,
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&temp);

    assert!(config.args.windows(2).any(|w| w[0] == "--anchor"));
    assert!(config.args.iter().any(|a| a == &anchor.to_string()));
}
