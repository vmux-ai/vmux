use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::client::cli::strategy::{
    CliAgentStrategy, ResumableSession, lines_skipping_invalid_utf8,
};
use crate::strategy::AgentStrategy;
use crate::{AgentKind, AgentVariant, AssistantBlock, McpServerConfig, Message};

pub struct VibeStrategy;

fn vibe_home() -> PathBuf {
    std::env::var("VIBE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".vibe")
        })
}

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
        vibe_home().join("logs").join("session")
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
            (
                "VIBE_SKILL_PATHS".to_string(),
                merged_skill_paths(
                    std::env::var("VIBE_SKILL_PATHS").ok().as_deref(),
                    &vmux_core::knowledge::skills_dir(),
                ),
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

fn merged_skill_paths(existing: Option<&str>, knowledge: &Path) -> String {
    let mut paths = existing
        .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
        .unwrap_or_default();
    let knowledge = knowledge.to_string_lossy().into_owned();
    if !paths.contains(&knowledge) {
        paths.push(knowledge);
    }
    serde_json::to_string(&paths).unwrap_or_else(|_| "[]".to_string())
}

fn serialize_vibe_mcp_env(mcp: &McpServerConfig) -> String {
    let mut vmux = serde_json::Map::from_iter([
        ("name".to_string(), serde_json::json!("vmux")),
        ("transport".to_string(), serde_json::json!("stdio")),
        (
            "command".to_string(),
            serde_json::json!(mcp.command.clone()),
        ),
        ("args".to_string(), serde_json::json!(mcp.args.clone())),
    ]);
    if let Some(cwd) = &mcp.cwd {
        vmux.insert("cwd".to_string(), serde_json::json!(cwd.to_string_lossy()));
    }
    let mut servers = vec![serde_json::Value::Object(vmux)];
    servers.extend(
        crate::managed_mcp::load()
            .iter()
            .map(|(name, server)| crate::managed_mcp::vibe_value(name, server)),
    );
    serde_json::to_string(&servers).unwrap_or_else(|_| "[]".to_string())
}

const VIBE_WEB_TOOLS: [&str; 2] = ["web_search", "web_fetch"];

const VMUX_HOOK_NAME: &str = "vmux-file-follow";
const VMUX_TURN_END_HOOK_NAME: &str = "vmux-turn-end";

fn vibe_hooks_path() -> PathBuf {
    vibe_home().join("hooks.toml")
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
    use std::io::BufReader;

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
        let title = std::fs::File::open(path.join("messages.jsonl"))
            .ok()
            .and_then(|file| {
                lines_skipping_invalid_utf8(BufReader::new(file))
                    .filter_map(|line| serde_json::from_str::<serde_json::Value>(&line).ok())
                    .filter(|value| {
                        value.get("injected").and_then(|value| value.as_bool()) != Some(true)
                    })
                    .find_map(|value| {
                        (value.get("role").and_then(|value| value.as_str()) == Some("user"))
                            .then(|| value.get("content"))
                            .flatten()
                            .and_then(|content| content.as_str())
                            .map(str::trim)
                            .filter(|content| !content.is_empty())
                            .map(|content| content.lines().collect::<Vec<_>>().join(" "))
                            .map(|content| content.chars().take(80).collect())
                    })
            })
            .unwrap_or_else(|| short_id.to_string());
        out.push(ResumableSession {
            kind: AgentKind::Vibe,
            sid: short_id.to_string(),
            cwd,
            mtime,
            title,
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
#[path = "vibe.test.rs"]
mod tests;
