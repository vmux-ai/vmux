use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

use crate::client::cli::strategy::{
    CliAgentStrategy, ResumableSession, lines_skipping_invalid_utf8,
};
use crate::strategy::AgentStrategy;
use crate::{AgentKind, AgentVariant, AssistantBlock, McpServerConfig, Message};

pub struct VibeStrategy;

impl AgentStrategy for VibeStrategy {
    fn kind(&self) -> AgentKind {
        AgentKind::Vibe
    }

    fn variant(&self) -> AgentVariant {
        AgentVariant::Cli
    }
}

impl CliAgentStrategy for VibeStrategy {
    fn sessions_root(&self) -> PathBuf {
        std::env::var("VIBE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(home).join(".vibe")
            })
            .join("logs")
            .join("session")
    }

    fn build_args(&self, _mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String> {
        // vmux launches vibe non-interactively, so the folder-trust prompt can't
        // be answered. Without trust, vibe runs restricted and ignores the user
        // config (falling back to default models). `--trust` trusts the working
        // directory for this invocation (vibe's documented automation flag).
        let mut args = vec!["--trust".to_string()];
        for tool in VIBE_WEB_TOOLS {
            args.push("--disabled-tools".to_string());
            args.push(tool.to_string());
        }
        if vmux_core::profile::is_test_session() {
            args.push("--auto-approve".to_string());
        }
        if let Some(sid) = session_id {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        }
        args
    }

    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)> {
        let mcp_json = serialize_vibe_mcp_env(mcp);
        vec![
            ("VIBE_MCP_SERVERS".to_string(), mcp_json),
            (
                "VIBE_ENABLE_EXPERIMENTAL_HOOKS".to_string(),
                "true".to_string(),
            ),
        ]
    }

    fn prepare_launch(&self, mcp: &McpServerConfig) {
        ensure_vibe_hooks(&mcp.command);
    }

    fn discover_session(
        &self,
        cwd: &Path,
        spawn_time: SystemTime,
        claimed: &HashSet<String>,
    ) -> Option<String> {
        discover_vibe_session_id(&self.sessions_root(), cwd, spawn_time, claimed)
    }

    fn detect_end_time(&self, session_id: &str) -> bool {
        let root = self.sessions_root();
        let Ok(entries) = std::fs::read_dir(&root) else {
            return false;
        };
        for entry in entries.flatten() {
            let meta_path = entry.path().join("meta.json");
            let Ok(text) = std::fs::read_to_string(&meta_path) else {
                continue;
            };
            let Ok(head) = serde_json::from_str::<MetaJsonHead>(&text) else {
                continue;
            };
            if head.session_id != session_id {
                continue;
            }
            let Ok(exit) = serde_json::from_str::<MetaJsonExit>(&text) else {
                continue;
            };
            return exit.end_time.is_some();
        }
        false
    }

    fn list_sessions(&self) -> Vec<ResumableSession> {
        list_vibe_sessions(&self.sessions_root())
    }

    fn load_transcript(&self, session_id: &str) -> Result<Vec<Message>, String> {
        load_vibe_transcript(&self.sessions_root(), session_id)
    }
}

#[derive(Serialize)]
struct VibeMcpServerEnv {
    name: &'static str,
    transport: &'static str,
    command: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
}

fn serialize_vibe_mcp_env(mcp: &McpServerConfig) -> String {
    let server = VibeMcpServerEnv {
        name: "vmux",
        transport: "stdio",
        command: mcp.command.clone(),
        args: mcp.args.clone(),
        cwd: mcp.cwd.as_ref().map(|c| c.to_string_lossy().to_string()),
    };
    serde_json::to_string(&[server]).unwrap_or_else(|_| "[]".to_string())
}

const VIBE_WEB_TOOLS: [&str; 2] = ["web_search", "web_fetch"];

const VMUX_HOOK_NAME: &str = "vmux-file-follow";
const VMUX_TURN_END_HOOK_NAME: &str = "vmux-turn-end";

fn vibe_hooks_path() -> PathBuf {
    std::env::var("VIBE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".vibe")
        })
        .join("hooks.toml")
}

/// Idempotently register vmux-managed hooks in `~/.vibe/hooks.toml`: an
/// `after_tool` hook that pings vmux on file read/edit, and a `post_agent_turn`
/// hook that pings vmux at turn-end (drives follow-pane auto-tidy + the
/// done-dot). Both commands no-op without `VMUX_ANCHOR`, so manual vibe use is
/// unaffected. Adds each named hook if absent and reconciles its command in
/// place when stale (e.g. after the vmux binary moves) — never clobbers
/// user-authored hooks.
fn ensure_vibe_hooks(vmux_command: &str) {
    write_vmux_hooks(&vibe_hooks_path(), vmux_command);
}

fn write_vmux_hooks(path: &Path, vmux_command: &str) {
    let mut doc: toml::Table = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or_default();
    let entry = doc
        .entry("hooks".to_string())
        .or_insert_with(|| toml::Value::Array(Vec::new()));
    let toml::Value::Array(hooks) = entry else {
        return;
    };
    upsert_vmux_hook(
        hooks,
        VMUX_HOOK_NAME,
        "after_tool",
        Some("re:^(read|edit|write)$"),
        &format!("{vmux_command} notify-file-touch"),
    );
    // `post_agent_turn` is not a tool hook, so vibe rejects `match`/`strict` on it.
    upsert_vmux_hook(
        hooks,
        VMUX_TURN_END_HOOK_NAME,
        "post_agent_turn",
        None,
        &format!("{vmux_command} notify-turn-end"),
    );
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = toml::to_string(&doc) {
        let _ = std::fs::write(path, text);
    }
}

fn upsert_vmux_hook(
    hooks: &mut Vec<toml::Value>,
    name: &str,
    hook_type: &str,
    match_re: Option<&str>,
    command: &str,
) {
    let table = match hooks
        .iter_mut()
        .find(|h| h.get("name").and_then(|n| n.as_str()) == Some(name))
    {
        Some(toml::Value::Table(table)) => table,
        Some(_) => return,
        None => {
            let mut hook = toml::Table::new();
            hook.insert("name".into(), name.into());
            hooks.push(toml::Value::Table(hook));
            let toml::Value::Table(table) = hooks.last_mut().expect("just pushed") else {
                return;
            };
            table
        }
    };
    table.insert("type".into(), hook_type.into());
    table.insert("command".into(), command.into());
    match match_re {
        Some(re) => {
            table.insert("match".into(), re.into());
            table.insert("strict".into(), false.into());
        }
        None => {
            table.remove("match");
            table.remove("strict");
        }
    }
}

#[derive(serde::Deserialize)]
struct MetaJson {
    environment: MetaEnvironment,
}
#[derive(serde::Deserialize)]
struct MetaEnvironment {
    working_directory: String,
}
#[derive(serde::Deserialize)]
struct MetaJsonHead {
    session_id: String,
}
#[derive(serde::Deserialize)]
struct MetaJsonExit {
    end_time: Option<String>,
}

fn normalize_cwd(path: &Path) -> String {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canon.to_string_lossy().trim_end_matches('/').to_string()
}

pub(crate) fn discover_vibe_session_id(
    sessions_root: &Path,
    cwd: &Path,
    spawn_time: SystemTime,
    claimed: &HashSet<String>,
) -> Option<String> {
    let cwd_norm = normalize_cwd(cwd);
    let entries = std::fs::read_dir(sessions_root).ok()?;
    let mut best: Option<(SystemTime, String)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(dirname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !dirname.starts_with("session_") {
            continue;
        }
        let Some(short_id) = dirname.rsplit('_').next() else {
            continue;
        };
        if short_id.is_empty() || claimed.contains(short_id) {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        let Ok(created) = meta.created().or_else(|_| meta.modified()) else {
            continue;
        };
        if created < spawn_time {
            continue;
        }
        let meta_path = path.join("meta.json");
        if let Ok(text) = std::fs::read_to_string(&meta_path)
            && let Ok(parsed) = serde_json::from_str::<MetaJson>(&text)
        {
            let meta_cwd = normalize_cwd(Path::new(&parsed.environment.working_directory));
            if meta_cwd != cwd_norm {
                continue;
            }
        }
        match &best {
            None => best = Some((created, short_id.to_string())),
            Some((cur, _)) if created < *cur => best = Some((created, short_id.to_string())),
            _ => {}
        }
    }
    best.map(|(_, id)| id)
}

pub(crate) fn list_vibe_sessions(root: &Path) -> Vec<ResumableSession> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(dirname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !dirname.starts_with("session_") {
            continue;
        }
        let Some(short_id) = dirname.rsplit('_').next() else {
            continue;
        };
        if short_id.is_empty() {
            continue;
        }
        let meta_path = path.join("meta.json");
        let mtime = std::fs::metadata(&meta_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let Some(meta) = std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|text| serde_json::from_str::<MetaJson>(&text).ok())
        else {
            continue;
        };
        let cwd = PathBuf::from(meta.environment.working_directory);
        if cwd.as_os_str().is_empty() {
            continue;
        }
        out.push(ResumableSession {
            kind: AgentKind::Vibe,
            sid: short_id.to_string(),
            cwd,
            mtime,
            title: short_id.to_string(),
            cross_runtime: true,
        });
    }
    out
}

pub(crate) fn load_vibe_transcript(root: &Path, session_id: &str) -> Result<Vec<Message>, String> {
    use std::io::BufReader;

    let entries = std::fs::read_dir(root)
        .map_err(|err| format!("read Vibe session root {}: {err}", root.display()))?;
    let mut path = None;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let Some(dirname) = entry_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if dirname.starts_with("session_") && dirname.rsplit('_').next() == Some(session_id) {
            path = Some(entry_path.join("messages.jsonl"));
            break;
        }
    }
    let path = path.ok_or_else(|| format!("Vibe session '{session_id}' not found"))?;
    let file = std::fs::File::open(&path)
        .map_err(|err| format!("open Vibe session {}: {err}", path.display()))?;
    let mut messages = Vec::new();
    for line in lines_skipping_invalid_utf8(BufReader::new(file)) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value.get("injected").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }
        let Some(text) = value
            .get("content")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        match value.get("role").and_then(|v| v.as_str()) {
            Some("user") => messages.push(Message::user(text)),
            Some("assistant") => messages.push(Message::Assistant {
                blocks: vec![AssistantBlock::Text(text)],
            }),
            _ => {}
        }
    }
    if messages.is_empty() {
        return Err(format!(
            "Vibe session '{session_id}' has no usable conversation"
        ));
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
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
        let result =
            discover_vibe_session_id(&sessions, Path::new("/tmp/anything"), spawn, &claimed);
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
        let result =
            discover_vibe_session_id(&sessions, Path::new("/tmp/anywhere"), spawn, &claimed);
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
        assert!(out[0].cross_runtime);
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
        transcript.extend_from_slice(
            b"{\"role\":\"assistant\",\"content\":\"after\",\"injected\":false}\n",
        );
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
}
