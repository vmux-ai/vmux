use super::*;
use std::time::Duration;

fn unique_tmp(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("vmux-agent-{label}-{pid}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn project_dir_name_replaces_slashes_and_dots_with_dashes() {
    assert_eq!(
        project_dir_name(Path::new("/Users/junichi.sugiura/.config/nvim")),
        "-Users-junichi-sugiura--config-nvim"
    );
    assert_eq!(project_dir_name(Path::new("/tmp/a")), "-tmp-a");
}

#[test]
fn discover_picks_jsonl_under_project_dir_after_spawn_time() {
    let tmp = unique_tmp("claude-discover");
    let dir = tmp.join("project");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("session-old.jsonl"), b"x").unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let spawn = SystemTime::now();
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(dir.join("session-new.jsonl"), b"x").unwrap();

    let claimed = HashSet::new();
    let id = discover_claude_session_id(&dir, spawn, &claimed);
    assert_eq!(id.as_deref(), Some("session-new"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn discover_skips_claimed() {
    let tmp = unique_tmp("claude-claimed");
    let dir = tmp.join("project");
    std::fs::create_dir_all(&dir).unwrap();
    let spawn = SystemTime::now();
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(dir.join("session-a.jsonl"), b"x").unwrap();
    std::fs::write(dir.join("session-b.jsonl"), b"x").unwrap();

    let mut claimed = HashSet::new();
    claimed.insert("session-a".to_string());
    let id = discover_claude_session_id(&dir, spawn, &claimed);
    assert_eq!(id.as_deref(), Some("session-b"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn effort_args_pass_claude_effort_flag() {
    assert_eq!(ClaudeStrategy.effort_args("high"), ["--effort", "high"]);
}

#[test]
fn build_args_includes_mcp_config() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into()],
        cwd: None,
    };
    let args = ClaudeStrategy.build_args(&mcp, None);
    assert!(args.iter().any(|a| a == "--mcp-config"));
    assert!(!args.iter().any(|a| a == "--strict-mcp-config"));
    assert!(!args.iter().any(|a| a == "--permission-mode"));
    assert!(!args.iter().any(|a| a == "bypassPermissions"));
}

#[test]
fn build_args_resume_appends_resume_flag() {
    let mcp = McpServerConfig {
        command: "x".into(),
        args: vec![],
        cwd: None,
    };
    let args = ClaudeStrategy.build_args(&mcp, Some("abc-123"));
    let resume_idx = args.iter().position(|a| a == "--resume").unwrap();
    assert_eq!(args[resume_idx + 1], "abc-123");
    assert_eq!(
        args.last().map(String::as_str),
        Some("abc-123"),
        "--resume must stay last so the tool flags don't swallow it"
    );
}

#[test]
fn build_args_disables_native_bash_and_steers_to_run() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into()],
        cwd: None,
    };
    let args = ClaudeStrategy.build_args(&mcp, None);

    let disallowed = args.iter().position(|a| a == "--disallowedTools").unwrap();
    assert_eq!(args[disallowed + 1], "Bash,Monitor,WebSearch,WebFetch");

    let allowed = args.iter().position(|a| a == "--allowedTools").unwrap();
    assert!(args[allowed + 1].contains("mcp__vmux__run"));
    assert!(args[allowed + 1].contains("mcp__vmux__read_terminal"));
    assert!(args[allowed + 1].contains("mcp__vmux__request_user_choice"));
    assert!(args[allowed + 1].contains("mcp__vmux__select_project"));
    assert!(args[allowed + 1].contains("mcp__vmux__create_worktree"));

    let steer = args
        .iter()
        .position(|a| a == "--append-system-prompt")
        .unwrap();
    assert!(args[steer + 1].contains("mcp__vmux__run"));
    assert!(args[steer + 1].contains("browser_navigate"));
    let workspace = args[steer + 1].find("mcp__vmux__select_project").unwrap();
    let worktree = args[steer + 1].find("mcp__vmux__create_worktree").unwrap();
    assert!(workspace < worktree);
}

#[test]
fn build_args_injects_notification_bell_hook() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into()],
        cwd: None,
    };
    let args = ClaudeStrategy.build_args(&mcp, None);
    let settings = args.iter().position(|a| a == "--settings").unwrap();
    let json = &args[settings + 1];
    assert!(json.contains("Notification"));
    assert!(json.contains("/dev/tty"));
    let parsed: Value = serde_json::from_str(json).unwrap();
    let cmd = parsed["hooks"]["Notification"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert_eq!(cmd, "printf '\\a' > /dev/tty");
}

#[test]
fn build_args_injects_file_touch_hook() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into(), "--anchor".into(), "42".into()],
        cwd: None,
    };
    let args = ClaudeStrategy.build_args(&mcp, None);
    let settings = args.iter().position(|a| a == "--settings").unwrap();
    let json = &args[settings + 1];
    assert!(json.contains("PostToolUse"), "json: {json}");
    assert!(json.contains("Read|Edit|Write|MultiEdit"));
    assert!(json.contains("notify-file-touch"));
    assert!(json.contains("\"--anchor\""));
    assert!(json.contains("\"42\""));
}

#[test]
fn build_args_injects_turn_end_stop_hook() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into(), "--anchor".into(), "42".into()],
        cwd: None,
    };
    let args = ClaudeStrategy.build_args(&mcp, None);
    let settings = args.iter().position(|a| a == "--settings").unwrap();
    let json = &args[settings + 1];
    let parsed: Value = serde_json::from_str(json).unwrap();
    let stop = &parsed["hooks"]["Stop"][0]["hooks"][0];
    assert_eq!(stop["command"].as_str().unwrap(), "/bin/vmux");
    let stop_args: Vec<&str> = stop["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(stop_args, vec!["notify-turn-end", "--anchor", "42"]);
    assert_eq!(stop["async"].as_bool(), Some(true));
}

#[test]
fn detect_end_time_always_false() {
    assert!(!ClaudeStrategy.detect_end_time("anything"));
}

#[test]
fn build_mcp_config_json_includes_vmux_server_with_command_and_args() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into()],
        cwd: Some(PathBuf::from("/work")),
    };
    let json = build_mcp_config_json(&mcp);
    assert!(json.contains("\"command\":\"/bin/vmux\""));
    assert!(json.contains("\"args\":[\"mcp\"]"));
    assert!(json.contains("\"cwd\":\"/work\""));
    assert!(json.contains("\"vmux\""));
    assert!(json.contains("\"mcpServers\""));
}

#[test]
fn build_env_extends_mcp_tool_timeout() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into()],
        cwd: None,
    };

    assert_eq!(
        ClaudeStrategy.build_env(&mcp),
        vec![("MCP_TOOL_TIMEOUT".into(), "660000".into())]
    );
}

#[test]
fn list_sessions_reads_sid_cwd_and_title_from_jsonl() {
    let tmp = unique_tmp("claude-list");
    let proj = tmp.join("-Users-me-proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
            proj.join("11111111-2222.jsonl"),
            b"{\"type\":\"user\",\"cwd\":\"/Users/me/proj\",\"message\":{\"role\":\"user\",\"content\":\"fix the auth bug\"}}\n",
        )
        .unwrap();
    std::fs::write(proj.join("agent-log.jsonl"), b"{}\n").unwrap();

    let out = list_claude_sessions(&tmp);
    assert_eq!(out.len(), 1, "agent-* excluded, one real session");
    let s = &out[0];
    assert_eq!(s.sid, "11111111-2222");
    assert_eq!(s.cwd, PathBuf::from("/Users/me/proj"));
    assert_eq!(s.title, "fix the auth bug");
    assert!(s.cross_runtime);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn list_sessions_title_falls_back_to_short_sid() {
    let tmp = unique_tmp("claude-list-fallback");
    let proj = tmp.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join("abcdef01-9999.jsonl"),
        b"{\"type\":\"summary\"}\n",
    )
    .unwrap();
    let out = list_claude_sessions(&tmp);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].title, "abcdef01");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn list_sessions_skips_unreadable_lines_before_metadata() {
    let tmp = unique_tmp("claude-list-invalid-line");
    let proj = tmp.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    let mut transcript = b"{\"type\":\"summary\"}\n".to_vec();
    transcript.extend_from_slice(b"\xff\n");
    transcript.extend_from_slice(
            b"{\"type\":\"user\",\"cwd\":\"/work/after-bad-line\",\"message\":{\"content\":\"still readable\"}}\n",
        );
    std::fs::write(proj.join("abcdef01-9999.jsonl"), transcript).unwrap();

    let out = list_claude_sessions(&tmp);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].cwd, PathBuf::from("/work/after-bad-line"));
    assert_eq!(out[0].title, "still readable");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn claude_transcript_extracts_non_meta_user_and_assistant_text() {
    use crate::{AssistantBlock, Message};

    let tmp = unique_tmp("claude-transcript");
    let proj = tmp.join("project");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
            proj.join("cl-1.jsonl"),
            concat!(
                "{bad}\n",
                "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"fix auth\"}}\n",
                "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"secret\"},{\"type\":\"text\",\"text\":\"working\"},{\"type\":\"tool_use\",\"name\":\"run\"}]}}\n",
                "{\"type\":\"user\",\"isMeta\":true,\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"injected\"}]}}\n",
                "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"content\":\"tool output\"}]}}\n"
            ),
        )
        .unwrap();

    let messages = load_claude_transcript(&tmp, "cl-1").unwrap();

    assert_eq!(
        messages,
        vec![
            Message::user("fix auth"),
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("working".into())]
            }
        ]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn claude_transcript_skips_invalid_utf8_line() {
    use crate::{AssistantBlock, Message};

    let tmp = unique_tmp("claude-transcript-invalid-utf8");
    let proj = tmp.join("project");
    std::fs::create_dir_all(&proj).unwrap();
    let mut transcript = b"{\"type\":\"user\",\"message\":{\"content\":\"before\"}}\n".to_vec();
    transcript.extend_from_slice(b"\xff\n");
    transcript.extend_from_slice(b"{\"type\":\"assistant\",\"message\":{\"content\":\"after\"}}\n");
    std::fs::write(proj.join("cl-1.jsonl"), transcript).unwrap();

    let messages = load_claude_transcript(&tmp, "cl-1").unwrap();

    assert_eq!(
        messages,
        vec![
            Message::user("before"),
            Message::Assistant {
                blocks: vec![AssistantBlock::Text("after".into())]
            }
        ]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn claude_transcript_rejects_unknown_or_empty_session() {
    let tmp = unique_tmp("claude-transcript-empty");
    let proj = tmp.join("project");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("cl-1.jsonl"), "{\"type\":\"summary\"}\n").unwrap();

    assert!(load_claude_transcript(&tmp, "missing").is_err());
    assert!(load_claude_transcript(&tmp, "cl-1").is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}
