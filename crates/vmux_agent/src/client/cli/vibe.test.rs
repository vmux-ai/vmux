use super::*;
use std::time::Duration;

#[test]
fn build_args_trust_resume_and_test_session_auto_approve() {
    let mcp = McpServerConfig {
        command: "vmux".to_string(),
        args: vec![],
        cwd: None,
    };
    let prev = std::env::var("VMUX_TEST").ok();
    unsafe { std::env::remove_var("VMUX_TEST") };
    assert_eq!(
        VibeStrategy.build_args(&mcp, None),
        vec![
            "--trust",
            "--disabled-tools",
            "web_search",
            "--disabled-tools",
            "web_fetch"
        ]
    );
    assert_eq!(
        VibeStrategy.build_args(&mcp, Some("sid-1")),
        vec![
            "--trust",
            "--disabled-tools",
            "web_search",
            "--disabled-tools",
            "web_fetch",
            "--resume",
            "sid-1"
        ]
    );
    unsafe { std::env::set_var("VMUX_TEST", "1") };
    assert!(
        VibeStrategy
            .build_args(&mcp, None)
            .iter()
            .any(|a| a == "--auto-approve")
    );
    unsafe { std::env::remove_var("VMUX_TEST") };
    if let Some(p) = prev {
        unsafe { std::env::set_var("VMUX_TEST", p) };
    }
}

#[test]
fn build_env_does_not_override_disabled_tools() {
    let mcp = McpServerConfig {
        command: "vmux".to_string(),
        args: vec![],
        cwd: None,
    };
    let env = VibeStrategy.build_env(&mcp);
    assert!(env.iter().all(|(key, _)| key != "VIBE_DISABLED_TOOLS"));
}

#[test]
fn build_env_enables_experimental_hooks() {
    let mcp = McpServerConfig {
        command: "vmux".to_string(),
        args: vec![],
        cwd: None,
    };
    let env = VibeStrategy.build_env(&mcp);
    assert!(
        env.iter()
            .any(|(k, v)| k == "VIBE_ENABLE_EXPERIMENTAL_HOOKS" && v == "true")
    );
}

#[test]
fn knowledge_skills_extend_existing_vibe_paths() {
    let merged = merged_skill_paths(Some("[\"/existing\"]"), Path::new("/knowledge"));
    let paths: Vec<String> = serde_json::from_str(&merged).unwrap();
    assert_eq!(paths, vec!["/existing", "/knowledge"]);
}

#[test]
fn vmux_hook_written_idempotently() {
    let tmp = unique_tmp("vibe-hooks");
    let path = tmp.join("hooks.toml");
    write_vmux_hooks(&path, "/bin/vmux");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("vmux-file-follow"), "text: {text}");
    assert!(text.contains("after_tool"));
    assert!(text.contains("notify-file-touch"));

    write_vmux_hooks(&path, "/bin/vmux");
    let doc: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    let count = doc
        .get("hooks")
        .and_then(|h| h.as_array())
        .unwrap()
        .iter()
        .filter(|h| h.get("name").and_then(|n| n.as_str()) == Some("vmux-file-follow"))
        .count();
    assert_eq!(count, 1, "idempotent: no duplicate");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn vmux_turn_end_hook_written_without_match_or_strict() {
    let tmp = unique_tmp("vibe-hooks-turn");
    let path = tmp.join("hooks.toml");
    write_vmux_hooks(&path, "/bin/vmux");
    let doc: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    let hooks = doc.get("hooks").and_then(|h| h.as_array()).unwrap();
    let turn = hooks
        .iter()
        .find(|h| h.get("name").and_then(|n| n.as_str()) == Some("vmux-turn-end"))
        .expect("turn-end hook present");
    assert_eq!(
        turn.get("type").and_then(|t| t.as_str()),
        Some("post_agent_turn")
    );
    assert_eq!(
        turn.get("command").and_then(|c| c.as_str()),
        Some("/bin/vmux notify-turn-end")
    );
    assert!(
        turn.get("match").is_none(),
        "post_agent_turn must not carry match"
    );
    assert!(
        turn.get("strict").is_none(),
        "post_agent_turn must not carry strict"
    );

    write_vmux_hooks(&path, "/bin/vmux");
    let doc: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    let count = doc
        .get("hooks")
        .and_then(|h| h.as_array())
        .unwrap()
        .iter()
        .filter(|h| h.get("name").and_then(|n| n.as_str()) == Some("vmux-turn-end"))
        .count();
    assert_eq!(count, 1, "idempotent: no duplicate turn-end hook");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn vmux_hook_reconciles_stale_command() {
    let tmp = unique_tmp("vibe-hooks-stale");
    let path = tmp.join("hooks.toml");
    write_vmux_hooks(&path, "/old/path/vmux");
    write_vmux_hooks(&path, "/new/path/vmux");
    let doc: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    let hooks = doc.get("hooks").and_then(|h| h.as_array()).unwrap();
    let ours: Vec<_> = hooks
        .iter()
        .filter(|h| h.get("name").and_then(|n| n.as_str()) == Some("vmux-file-follow"))
        .collect();
    assert_eq!(ours.len(), 1, "no duplicate after reconcile");
    assert_eq!(
        ours[0].get("command").and_then(|c| c.as_str()),
        Some("/new/path/vmux notify-file-touch"),
        "stale command updated"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn vmux_hook_preserves_user_hooks() {
    let tmp = unique_tmp("vibe-hooks-user");
    let path = tmp.join("hooks.toml");
    std::fs::write(
            &path,
            "[[hooks]]\nname = \"mine\"\ntype = \"before_tool\"\nmatch = \"bash\"\ncommand = \"echo hi\"\n",
        )
        .unwrap();
    write_vmux_hooks(&path, "/bin/vmux");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("mine"), "user hook preserved: {text}");
    assert!(text.contains("vmux-file-follow"));
    let _ = std::fs::remove_dir_all(&tmp);
}

fn write_meta(
    dir: &Path,
    session_id: &str,
    working_dir: &str,
    start_time: &str,
    end_time: Option<&str>,
) {
    std::fs::create_dir_all(dir).unwrap();
    let end_field = end_time
        .map(|e| format!(r#","end_time":"{e}""#))
        .unwrap_or_default();
    std::fs::write(
            dir.join("meta.json"),
            format!(
                r#"{{"session_id":"{session_id}","start_time":"{start_time}"{end_field},"environment":{{"working_directory":"{working_dir}"}}}}"#
            ),
        )
        .unwrap();
}

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
fn discover_returns_short_uuid_from_session_dir_name() {
    let tmp = unique_tmp("vibe-discover-shortid");
    let sessions = tmp.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let spawn = SystemTime::now();
    std::thread::sleep(Duration::from_millis(20));
    std::fs::create_dir_all(sessions.join("session_20260515_214210_3d4fcbe1")).unwrap();
    let claimed = HashSet::new();
    let result = discover_vibe_session_id(&sessions, Path::new("/tmp/anything"), spawn, &claimed);
    assert_eq!(result.as_deref(), Some("3d4fcbe1"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn discover_skips_dirs_created_before_spawn_time() {
    let tmp = unique_tmp("vibe-discover-old");
    let sessions = tmp.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::create_dir_all(sessions.join("session_20260101_000000_oldsess1")).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let spawn = SystemTime::now();
    let claimed = HashSet::new();
    let result = discover_vibe_session_id(&sessions, Path::new("/tmp/x"), spawn, &claimed);
    assert!(result.is_none());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn discover_skips_claimed_short_ids() {
    let tmp = unique_tmp("vibe-discover-claimed");
    let sessions = tmp.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let spawn = SystemTime::now();
    std::thread::sleep(Duration::from_millis(20));
    std::fs::create_dir_all(sessions.join("session_20260515_214210_aaaaaaaa")).unwrap();
    std::fs::create_dir_all(sessions.join("session_20260515_214300_bbbbbbbb")).unwrap();
    let mut claimed = HashSet::new();
    claimed.insert("aaaaaaaa".to_string());
    let result = discover_vibe_session_id(&sessions, Path::new("/tmp/x"), spawn, &claimed);
    assert_eq!(result.as_deref(), Some("bbbbbbbb"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn discover_filters_by_meta_cwd_when_meta_present() {
    let tmp = unique_tmp("vibe-discover-meta-cwd");
    let sessions = tmp.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let spawn = SystemTime::now();
    std::thread::sleep(Duration::from_millis(20));
    write_meta(
        &sessions.join("session_20260515_214210_xxxxxxxx"),
        "full-uuid-x",
        "/tmp/work-X",
        "2026-05-15T21:42:10+00:00",
        None,
    );
    write_meta(
        &sessions.join("session_20260515_214300_yyyyyyyy"),
        "full-uuid-y",
        "/tmp/work-Y",
        "2026-05-15T21:43:00+00:00",
        None,
    );
    let claimed = HashSet::new();
    let result = discover_vibe_session_id(&sessions, Path::new("/tmp/work-Y"), spawn, &claimed);
    assert_eq!(result.as_deref(), Some("yyyyyyyy"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn discover_uses_dirname_when_meta_json_absent() {
    let tmp = unique_tmp("vibe-discover-nometa");
    let sessions = tmp.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let spawn = SystemTime::now();
    std::thread::sleep(Duration::from_millis(20));
    std::fs::create_dir_all(sessions.join("session_20260515_214210_freshone")).unwrap();
    let claimed = HashSet::new();
    let result = discover_vibe_session_id(&sessions, Path::new("/tmp/anywhere"), spawn, &claimed);
    assert_eq!(result.as_deref(), Some("freshone"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn detect_end_time_returns_true_when_meta_has_end_time() {
    let tmp = unique_tmp("vibe-end");
    let sessions = tmp.join("sessions");
    let cwd = "/tmp/work";
    write_meta(
        &sessions.join("a"),
        "ended-id",
        cwd,
        "2026-05-11T12:00:00+00:00",
        Some("2026-05-11T13:00:00+00:00"),
    );
    write_meta(
        &sessions.join("b"),
        "live-id",
        cwd,
        "2026-05-11T12:00:00+00:00",
        None,
    );

    let read_end = |id: &str| -> bool {
        let entries = std::fs::read_dir(&sessions).unwrap();
        for entry in entries.flatten() {
            let path = entry.path().join("meta.json");
            let text = std::fs::read_to_string(&path).unwrap();
            let head: MetaJsonHead = serde_json::from_str(&text).unwrap();
            if head.session_id != id {
                continue;
            }
            let exit: MetaJsonExit = serde_json::from_str(&text).unwrap();
            return exit.end_time.is_some();
        }
        false
    };
    assert!(read_end("ended-id"));
    assert!(!read_end("live-id"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn list_sessions_reads_meta_json() {
    let tmp = unique_tmp("vibe-list");
    let sdir = tmp.join("session_vb-1");
    std::fs::create_dir_all(&sdir).unwrap();
    std::fs::write(
        sdir.join("meta.json"),
        b"{\"environment\":{\"working_directory\":\"/w/y\"}}",
    )
    .unwrap();
    let out = list_vibe_sessions(&tmp);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].sid, "vb-1");
    assert_eq!(out[0].cwd, PathBuf::from("/w/y"));
    assert_eq!(out[0].title, "vb-1");
    assert!(out[0].cross_runtime);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn list_sessions_uses_first_user_prompt_as_title() {
    let tmp = unique_tmp("vibe-list-title");
    let sdir = tmp.join("session_vb-1");
    std::fs::create_dir_all(&sdir).unwrap();
    std::fs::write(
        sdir.join("meta.json"),
        b"{\"environment\":{\"working_directory\":\"/w/y\"}}",
    )
    .unwrap();
    std::fs::write(
        sdir.join("messages.jsonl"),
        concat!(
            "{\"role\":\"user\",\"content\":\"hidden\",\"injected\":true}\n",
            "{\"role\":\"assistant\",\"content\":\"hello\",\"injected\":false}\n",
            "{\"role\":\"user\",\"content\":\"fix the\\napproval flow\",\"injected\":false}\n",
            "{\"role\":\"user\",\"content\":\"second prompt\",\"injected\":false}\n"
        ),
    )
    .unwrap();

    let out = list_vibe_sessions(&tmp);

    assert_eq!(out[0].title, "fix the approval flow");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn list_sessions_uses_meta_modified_time() {
    let tmp = unique_tmp("vibe-list-mtime");
    let sdir = tmp.join("session_vb-1");
    let meta = sdir.join("meta.json");
    std::fs::create_dir_all(&sdir).unwrap();
    std::fs::write(&meta, b"{\"environment\":{\"working_directory\":\"/w/y\"}}").unwrap();
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(&meta, b"{\"environment\":{\"working_directory\":\"/w/y\"}}").unwrap();
    let expected = std::fs::metadata(&meta).unwrap().modified().unwrap();

    let out = list_vibe_sessions(&tmp);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].mtime, expected);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn list_sessions_skips_entries_without_valid_cwd_metadata() {
    let tmp = unique_tmp("vibe-list-invalid-meta");
    std::fs::create_dir_all(tmp.join("session_vb-1")).unwrap();

    let out = list_vibe_sessions(&tmp);

    assert!(out.is_empty());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn vibe_transcript_extracts_non_injected_user_and_assistant_text() {
    use crate::{AssistantBlock, Message};

    let tmp = unique_tmp("vibe-transcript");
    let session = tmp.join("session_20260713_120000_vb1");
    std::fs::create_dir_all(&session).unwrap();
    std::fs::write(
            session.join("messages.jsonl"),
            concat!(
                "{bad}\n",
                "{\"role\":\"user\",\"content\":\"fix auth\",\"injected\":false}\n",
                "{\"role\":\"assistant\",\"content\":\"working\",\"reasoning_content\":\"secret\",\"injected\":false}\n",
                "{\"role\":\"user\",\"content\":\"injected\",\"injected\":true}\n",
                "{\"role\":\"tool\",\"content\":\"tool output\",\"injected\":false}\n"
            ),
        )
        .unwrap();

    let messages = load_vibe_transcript(&tmp, "vb1").unwrap();

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
fn vibe_transcript_skips_invalid_utf8_line() {
    use crate::{AssistantBlock, Message};

    let tmp = unique_tmp("vibe-transcript-invalid-utf8");
    let session = tmp.join("session_20260713_120000_vb1");
    std::fs::create_dir_all(&session).unwrap();
    let mut transcript =
        b"{\"role\":\"user\",\"content\":\"before\",\"injected\":false}\n".to_vec();
    transcript.extend_from_slice(b"\xff\n");
    transcript
        .extend_from_slice(b"{\"role\":\"assistant\",\"content\":\"after\",\"injected\":false}\n");
    std::fs::write(session.join("messages.jsonl"), transcript).unwrap();

    let messages = load_vibe_transcript(&tmp, "vb1").unwrap();

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
fn vibe_transcript_rejects_unknown_or_empty_session() {
    let tmp = unique_tmp("vibe-transcript-empty");
    let session = tmp.join("session_20260713_120000_vb1");
    std::fs::create_dir_all(&session).unwrap();
    std::fs::write(session.join("messages.jsonl"), "{\"role\":\"tool\"}\n").unwrap();

    assert!(load_vibe_transcript(&tmp, "missing").is_err());
    assert!(load_vibe_transcript(&tmp, "vb1").is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}
