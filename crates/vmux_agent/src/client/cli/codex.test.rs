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

fn write_session(root: &Path, ymd: &str, file: &str, id: &str, cwd: &str) {
    let dir = root.join(ymd);
    std::fs::create_dir_all(&dir).unwrap();
    let line = format!(
        r#"{{"timestamp":"2026-04-30T11:41:00.170Z","type":"session_meta","payload":{{"id":"{id}","timestamp":"2026-04-30T09:56:21.846Z","cwd":"{cwd}"}}}}"#
    );
    std::fs::write(dir.join(file), format!("{line}\n")).unwrap();
}

#[test]
fn effort_args_pass_codex_reasoning_override() {
    assert_eq!(
        CodexStrategy.effort_args("high"),
        ["-c", "model_reasoning_effort=high"]
    );
}

#[test]
fn quote_toml_escapes_quotes_and_backslashes() {
    assert_eq!(quote_toml("a"), "\"a\"");
    assert_eq!(quote_toml(r#"a"b"#), "\"a\\\"b\"");
    assert_eq!(quote_toml(r"a\b"), "\"a\\\\b\"");
}

#[test]
fn toml_array_emits_quoted_csv() {
    assert_eq!(toml_array(&[]), "[]");
    assert_eq!(toml_array(&["mcp".into(), "x".into()]), "[\"mcp\",\"x\"]");
}

#[test]
fn build_args_uses_dash_c_overrides_for_mcp() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into()],
        cwd: None,
    };
    let args = CodexStrategy.build_args(&mcp, None);
    assert!(!args.iter().any(|a| a == "-s"));
    assert!(!args.iter().any(|a| a == "-a"));
    assert!(
        args.iter()
            .any(|a| a == "mcp_servers.vmux.command=\"/bin/vmux\"")
    );
    assert!(args.iter().any(|a| a == "mcp_servers.vmux.args=[\"mcp\"]"));
    assert!(
        args.iter()
            .any(|a| a == "mcp_servers.vmux.tool_timeout_sec=660")
    );
}

#[test]
fn build_args_injects_file_touch_hook() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into(), "--anchor".into(), "42".into()],
        cwd: None,
    };
    let args = CodexStrategy.build_args(&mcp, None);
    assert!(args.iter().any(|a| a == "features.hooks=true"));
    let hook = args
        .iter()
        .find(|a| a.starts_with("hooks.PostToolUse="))
        .expect("hook override present");
    assert!(hook.contains("apply_patch|Edit|Write"), "hook: {hook}");
    assert!(hook.contains("notify-file-touch"));
    assert!(hook.contains("--anchor"));
    assert!(hook.contains("\"42\""));
}

#[test]
fn build_args_injects_turn_end_stop_hook() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into(), "--anchor".into(), "42".into()],
        cwd: None,
    };
    let args = CodexStrategy.build_args(&mcp, None);
    let hook = args
        .iter()
        .find(|a| a.starts_with("hooks.Stop="))
        .expect("Stop hook override present");
    assert!(hook.contains("notify-turn-end"), "hook: {hook}");
    assert!(hook.contains("--anchor"));
    assert!(hook.contains("\"42\""));
    assert!(
        !hook.contains("matcher"),
        "Stop hook takes no matcher: {hook}"
    );
}

#[test]
fn build_args_resume_uses_resume_subcommand() {
    let mcp = McpServerConfig {
        command: "x".into(),
        args: vec![],
        cwd: None,
    };
    let args = CodexStrategy.build_args(&mcp, Some("abc-123"));
    let resume_idx = args.iter().position(|a| a == "resume").unwrap();
    assert_eq!(args[resume_idx + 1], "abc-123");
    let last_dash_c = args.iter().rposition(|a| a == "-c").unwrap();
    assert!(resume_idx > last_dash_c);
    let last_disable = args.iter().rposition(|a| a == "--disable").unwrap();
    assert!(
        resume_idx > last_disable,
        "the resume subcommand must follow the global --disable options"
    );
}

#[test]
fn build_args_disables_native_shell_features() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into()],
        cwd: None,
    };
    let args = CodexStrategy.build_args(&mcp, None);
    let disabled: Vec<&str> = args
        .windows(2)
        .filter(|w| w[0] == "--disable")
        .map(|w| w[1].as_str())
        .collect();
    assert!(disabled.contains(&"shell_tool"));
    assert!(disabled.contains(&"unified_exec"));
}

#[test]
fn build_args_disables_native_web_search() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into()],
        cwd: None,
    };
    let args = CodexStrategy.build_args(&mcp, None);
    assert!(args.iter().any(|a| a == "tools.web_search=false"));
}

#[test]
fn skill_config_override_disables_embedded_vmux_skills() {
    let override_value = build_skills_config_override(&[
        PathBuf::from("/tmp/knowledge/alpha/SKILL.md"),
        PathBuf::from("/tmp/knowledge/beta/SKILL.md"),
    ])
    .unwrap();
    assert_eq!(
        override_value,
        "skills.config=[{path=\"/tmp/knowledge/alpha/SKILL.md\",enabled=false},{path=\"/tmp/knowledge/beta/SKILL.md\",enabled=false}]"
    );
}

#[test]
fn bundled_browser_skill_discovery_finds_versioned_skill_files() {
    let temp = tempfile::tempdir().unwrap();
    let skill = temp
        .path()
        .join("26.1/skills/control-in-app-browser/SKILL.md");
    std::fs::create_dir_all(skill.parent().unwrap()).unwrap();
    std::fs::write(&skill, "browser").unwrap();
    std::fs::write(temp.path().join("ignored.md"), "ignored").unwrap();

    let mut files = Vec::new();
    collect_skill_files(temp.path(), &mut files);
    assert_eq!(files, vec![skill]);
}

#[test]
fn build_args_steers_web_access_to_vmux_browser() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into()],
        cwd: None,
    };
    let args = CodexStrategy.build_args(&mcp, None);
    let steer = args
        .iter()
        .find(|a| a.starts_with("developer_instructions="))
        .expect("developer_instructions override present");
    assert!(steer.contains("mcp__vmux__run"));
    assert!(steer.contains("browser_navigate"));
    assert!(steer.contains("browser_snapshot"));
    assert!(steer.contains("page already visible beside you"));
    assert!(steer.contains("Never use browser:control-in-app-browser"));
}

#[test]
fn build_args_forces_vmux_tools_direct_to_bypass_deferral() {
    let mcp = McpServerConfig {
        command: "/bin/vmux".into(),
        args: vec!["mcp".into()],
        cwd: None,
    };
    let args = CodexStrategy.build_args(&mcp, None);
    assert!(
        args.iter()
            .any(|a| a == "features.code_mode.direct_only_tool_namespaces=[\"mcp__vmux\"]"),
        "vmux tools must be pinned direct so codex does not defer run behind tool_search"
    );
}

#[test]
fn discover_walks_yyyy_mm_dd_dirs() {
    let tmp = unique_tmp("codex-walk");
    let sessions = tmp.join("sessions");
    let cwd = "/tmp/work";
    let spawn = SystemTime::now() - Duration::from_secs(60);
    write_session(&sessions, "2026/05/14", "rollout-a.jsonl", "id-a", cwd);
    write_session(
        &sessions,
        "2026/05/14",
        "rollout-b.jsonl",
        "id-b",
        "/tmp/other",
    );

    let claimed = HashSet::new();
    let result = discover_codex_session_id(&sessions, Path::new(cwd), spawn, &claimed);
    assert_eq!(result.as_deref(), Some("id-a"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn detect_end_time_always_false() {
    assert!(!CodexStrategy.detect_end_time("anything"));
}

#[test]
fn list_sessions_reads_session_meta() {
    let tmp = unique_tmp("codex-list");
    let day = tmp.join("2026/07");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(
        day.join("sess.jsonl"),
        b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"cx-1\",\"cwd\":\"/w/x\"}}\n",
    )
    .unwrap();
    let out = list_codex_sessions(&tmp);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].sid, "cx-1");
    assert_eq!(out[0].cwd, PathBuf::from("/w/x"));
    assert_eq!(out[0].title, "cx");
    assert!(out[0].cross_runtime);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn list_sessions_uses_first_user_prompt_as_title() {
    let tmp = unique_tmp("codex-list-title");
    let day = tmp.join("2026/07");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(
            day.join("sess.jsonl"),
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"cx-1\",\"cwd\":\"/w/x\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"hello\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"fix the\\napproval flow\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"second prompt\"}}\n"
            ),
        )
        .unwrap();

    let out = list_codex_sessions(&tmp);

    assert_eq!(out[0].title, "fix the approval flow");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn list_sessions_reads_valid_head_when_later_bytes_are_invalid_utf8() {
    let tmp = unique_tmp("codex-list-invalid-tail");
    let day = tmp.join("2026/07");
    std::fs::create_dir_all(&day).unwrap();
    let mut transcript =
        b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"cx-1\",\"cwd\":\"/w/x\"}}\n".to_vec();
    transcript.extend_from_slice(b"\xff\n");
    std::fs::write(day.join("sess.jsonl"), transcript).unwrap();

    let out = list_codex_sessions(&tmp);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].sid, "cx-1");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn codex_transcript_extracts_user_and_agent_messages() {
    use crate::{AssistantBlock, Message};

    let tmp = unique_tmp("codex-transcript");
    let day = tmp.join("2026/07");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(
            day.join("sess.jsonl"),
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"cx-1\",\"cwd\":\"/w/x\"}}\n",
                "{not-json}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"fix auth\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\",\"content\":\"secret\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"working\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"output\":\"tool output\"}}\n"
            ),
        )
        .unwrap();

    let messages = load_codex_transcript(&tmp, "cx-1").unwrap();

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
fn codex_transcript_skips_invalid_utf8_line() {
    use crate::{AssistantBlock, Message};

    let tmp = unique_tmp("codex-transcript-invalid-utf8");
    let day = tmp.join("2026/07");
    std::fs::create_dir_all(&day).unwrap();
    let mut transcript =
        b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"cx-1\",\"cwd\":\"/w/x\"}}\n".to_vec();
    transcript.extend_from_slice(
            b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"before\"}}\n",
        );
    transcript.extend_from_slice(b"\xff\n");
    transcript.extend_from_slice(
            b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"after\"}}\n",
        );
    std::fs::write(day.join("sess.jsonl"), transcript).unwrap();

    let messages = load_codex_transcript(&tmp, "cx-1").unwrap();

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
fn codex_transcript_rejects_unknown_or_empty_session() {
    let tmp = unique_tmp("codex-transcript-empty");
    write_session(&tmp, "2026/07", "sess.jsonl", "cx-1", "/w");

    assert!(load_codex_transcript(&tmp, "missing").is_err());
    assert!(load_codex_transcript(&tmp, "cx-1").is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}
