use super::*;

#[test]
fn from_url_segment_recognizes_known_kinds() {
    assert_eq!(AgentKind::from_url_segment("vibe"), Some(AgentKind::Vibe));
    assert_eq!(
        AgentKind::from_url_segment("claude"),
        Some(AgentKind::Claude)
    );
    assert_eq!(AgentKind::from_url_segment("codex"), Some(AgentKind::Codex));
    assert_eq!(AgentKind::from_url_segment("nope"), None);
}

#[test]
fn executable_returns_cli_binary_name() {
    assert_eq!(AgentKind::Vibe.executable(), "vibe");
    assert_eq!(AgentKind::Claude.executable(), "claude");
    assert_eq!(AgentKind::Codex.executable(), "codex");
}

#[test]
fn cli_url_prefix_returns_three_segment_form() {
    assert_eq!(AgentKind::Vibe.cli_url_prefix(), "vmux://agent/vibe/");
    assert_eq!(AgentKind::Claude.cli_url_prefix(), "vmux://agent/claude/");
}

#[test]
fn agent_kind_into_terminal_kind() {
    assert_eq!(TerminalKind::from(AgentKind::Vibe), TerminalKind::Vibe);
    assert_eq!(TerminalKind::from(AgentKind::Claude), TerminalKind::Claude);
    assert_eq!(TerminalKind::from(AgentKind::Codex), TerminalKind::Codex);
}

#[test]
fn parse_page_agent_url_provider_model_only() {
    let (provider, model, sid) = parse_page_agent_url("vmux://agent/openai/gpt-5.5").unwrap();
    assert_eq!(provider, "openai");
    assert_eq!(model, "gpt-5.5");
    assert!(sid.is_none());
}

#[test]
fn parse_page_agent_url_with_sid() {
    let (provider, model, sid) =
        parse_page_agent_url("vmux://agent/anthropic/claude-opus-4.7/xHigh").unwrap();
    assert_eq!(provider, "anthropic");
    assert_eq!(model, "claude-opus-4.7");
    assert_eq!(sid.as_deref(), Some("xHigh"));
}

#[test]
fn parse_page_agent_url_rejects_single_segment() {
    assert!(parse_page_agent_url("vmux://agent/vibe").is_none());
}

#[test]
fn parse_acp_agent_url_single_segment() {
    assert_eq!(
        parse_acp_agent_url("vmux://agent/vibe-acp"),
        Some("vibe-acp".to_string())
    );
    assert!(parse_acp_agent_url("vmux://agent/openai/gpt-5.5").is_none());
    assert!(parse_acp_agent_url("https://google.com").is_none());
}

#[test]
fn parse_page_agent_url_rejects_too_many_segments() {
    assert!(parse_page_agent_url("vmux://agent/openai/gpt/sid/extra").is_none());
}

#[test]
fn parse_page_agent_url_rejects_non_agent_host() {
    assert!(parse_page_agent_url("https://google.com").is_none());
}

#[test]
fn effort_levels_exposed_only_for_wired_agents() {
    assert_eq!(effort_levels("claude"), ["low", "medium", "high", "max"]);
    assert_eq!(
        effort_levels("cli:claude"),
        ["low", "medium", "high", "max"]
    );
    assert_eq!(
        effort_levels("cli:codex"),
        ["minimal", "low", "medium", "high"]
    );
    // ACP codex/gemini and unknown agents have no vmux-driven effort control yet.
    assert!(effort_levels("codex").is_empty());
    assert!(effort_levels("gemini").is_empty());
    assert!(effort_levels("vibe").is_empty());
    assert!(effort_levels("cli:vibe").is_empty());
}
