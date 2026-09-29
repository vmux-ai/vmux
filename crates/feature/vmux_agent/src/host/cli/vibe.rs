use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy::prelude::*;

use super::{
    CliModelCatalog, CliSessionRoot, CliSessionSource, ResumableSession, VibeCli,
    lines_skipping_invalid_utf8,
};
use crate::session::{AgentSession, AgentSessionExited, PendingAgentSession, SessionId};
use crate::{AgentKind, AssistantBlock, McpServerConfig, Message};

use super::super::launch::CliLaunchProvider;
use super::super::session::{DiscoverAgentSessions, DiscoverAgentSessionsSet};

pub(super) struct VibeCliPlugin;

impl Plugin for VibeCliPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_vibe_cli).add_systems(
            Update,
            (discover_sessions, detect_ended_sessions).in_set(DiscoverAgentSessionsSet),
        );
    }
}

fn spawn_vibe_cli(mut commands: Commands) {
    commands.spawn((
        Name::new("Vibe CLI session source"),
        VibeCli,
        SESSIONS,
        CliSessionRoot(sessions_root()),
        VibeModels::load(),
    ));
}

fn discover_sessions(
    mut requests: MessageReader<DiscoverAgentSessions>,
    root: Single<&CliSessionRoot, With<VibeCli>>,
    pending_sessions: Query<(Entity, &PendingAgentSession)>,
    sessions: Query<(&AgentSession, &SessionId)>,
    mut commands: Commands,
) {
    if requests.read().next().is_none() {
        return;
    }
    for (entity, pending) in &pending_sessions {
        if pending.kind != AgentKind::Vibe {
            continue;
        }
        let claimed = sessions
            .iter()
            .filter_map(|(session, id)| {
                if session.kind == pending.kind {
                    Some(id.0.clone())
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>();
        if let Some(id) =
            discover_vibe_session_id(&root.0, &pending.cwd, pending.spawn_time, &claimed)
        {
            commands
                .entity(entity)
                .insert(SessionId(id))
                .remove::<PendingAgentSession>();
        }
    }
}

fn detect_ended_sessions(
    mut requests: MessageReader<DiscoverAgentSessions>,
    root: Single<&CliSessionRoot, With<VibeCli>>,
    sessions: Query<(Entity, &AgentSession, &SessionId)>,
    mut exited: MessageWriter<AgentSessionExited>,
    mut commands: Commands,
) {
    if requests.read().next().is_none() {
        return;
    }
    for (entity, session, sid) in &sessions {
        if session.kind != AgentKind::Vibe || !vibe_session_ended(&root.0, &sid.0) {
            continue;
        }
        commands
            .entity(entity)
            .remove::<AgentSession>()
            .remove::<SessionId>()
            .remove::<PendingAgentSession>();
        exited.write(AgentSessionExited { entity });
    }
}

fn vibe_home() -> PathBuf {
    std::env::var("VIBE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".vibe")
        })
}

pub(super) const SESSIONS: CliSessionSource = CliSessionSource {
    kind: AgentKind::Vibe,
    list_sessions,
    load_transcript,
};

pub(crate) struct VibeLaunch;

impl CliLaunchProvider for VibeLaunch {
    const KIND: AgentKind = AgentKind::Vibe;

    fn arguments(_mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String> {
        build_args(session_id)
    }

    fn model_environment(model: &str) -> Vec<(String, String)> {
        vec![("VIBE_ACTIVE_MODEL".to_string(), model.to_string())]
    }

    fn environment(mcp: &McpServerConfig) -> Vec<(String, String)> {
        build_env(mcp)
    }

    fn prepare(mcp: &McpServerConfig) {
        ensure_vibe_hooks(&mcp.command);
    }
}

fn sessions_root() -> PathBuf {
    vibe_home().join("logs").join("session")
}

fn build_args(session_id: Option<&str>) -> Vec<String> {
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

fn build_env(mcp: &McpServerConfig) -> Vec<(String, String)> {
    let mcp_json = serialize_vibe_mcp_env(mcp);
    let mut env = vec![
        ("VIBE_MCP_SERVERS".to_string(), mcp_json),
        (
            "VIBE_ENABLE_EXPERIMENTAL_HOOKS".to_string(),
            "true".to_string(),
        ),
        (
            "VIBE_SKILL_PATHS".to_string(),
            merged_skill_paths(
                std::env::var("VIBE_SKILL_PATHS").ok().as_deref(),
                &vmux_core::knowledge::KnowledgeVault::user()
                    .skills()
                    .into_path(),
            ),
        ),
    ];
    env.extend(crate::managed_mcp::McpAuthorization::environment());
    env
}

fn list_sessions() -> Vec<ResumableSession> {
    list_vibe_sessions(&sessions_root())
}

fn load_transcript(session_id: &str) -> Result<Vec<Message>, String> {
    load_vibe_transcript(&sessions_root(), session_id)
}

#[derive(Default, serde::Deserialize)]
struct VibeConfig {
    #[serde(default)]
    active_model: String,
    #[serde(default)]
    models: Vec<VibeModel>,
}

#[derive(serde::Deserialize)]
struct VibeModel {
    name: String,
    #[serde(default)]
    alias: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    provider: String,
}

struct VibeModels;

impl VibeModels {
    fn load() -> CliModelCatalog {
        Self::from_config(&vibe_home().join("config.toml"))
    }

    fn from_config(path: &Path) -> CliModelCatalog {
        let config = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str::<VibeConfig>(&text).ok())
            .unwrap_or_default();
        let mut models = Vec::new();
        for model in config.models {
            let id = if model.alias.is_empty() {
                model.name.clone()
            } else {
                model.alias
            };
            let name = if model.display_name.is_empty() {
                id.clone()
            } else {
                model.display_name
            };
            models.push(vmux_api::room::ModelOptionEntry {
                id,
                name,
                description: model.provider,
            });
        }
        if !config.active_model.is_empty()
            && !models.iter().any(|model| model.id == config.active_model)
        {
            models.insert(
                0,
                vmux_api::room::ModelOptionEntry {
                    id: config.active_model.clone(),
                    name: config.active_model.clone(),
                    description: String::new(),
                },
            );
        }
        let selected = if config.active_model.is_empty() {
            models
                .first()
                .map(|model| model.id.clone())
                .unwrap_or_default()
        } else {
            config.active_model
        };
        CliModelCatalog { selected, models }
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
        crate::managed_mcp::ManagedMcpServers::current()
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

fn discover_vibe_session_id(
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

fn vibe_session_ended(sessions_root: &Path, session_id: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(sessions_root) else {
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

fn list_vibe_sessions(root: &Path) -> Vec<ResumableSession> {
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
        let transcript = path.join("messages.jsonl");
        out.push(ResumableSession {
            kind: AgentKind::Vibe,
            sid: short_id.to_string(),
            cwd,
            latest: vibe_latest_message(&transcript),
            transcript,
            mtime,
            title,
            cross_runtime: true,
        });
    }
    out
}

fn load_vibe_transcript(root: &Path, session_id: &str) -> Result<Vec<Message>, String> {
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

fn vibe_latest_message(path: &Path) -> String {
    for line in super::SessionTail::lines_of(path).iter().rev() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("injected").and_then(serde_json::Value::as_bool) == Some(true) {
            continue;
        }
        if value.get("role").and_then(serde_json::Value::as_str) != Some("user") {
            continue;
        }
        let Some(text) = value.get("content").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        return text.lines().collect::<Vec<_>>().join(" ");
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn model_catalog_reads_vibe_aliases_and_active_model() {
        let tmp = unique_tmp("vibe-models");
        let config = tmp.join("config.toml");
        std::fs::write(
            &config,
            concat!(
                "active_model = \"opus\"\n",
                "[[models]]\n",
                "name = \"claude-opus-5\"\n",
                "alias = \"opus\"\n",
                "provider = \"anthropic\"\n"
            ),
        )
        .unwrap();

        let models = VibeModels::from_config(&config);

        assert_eq!(models.selected, "opus");
        assert_eq!(models.models[0].id, "opus");
        assert_eq!(models.models[0].description, "anthropic");
        assert_eq!(
            VibeLaunch::model_environment("opus"),
            [("VIBE_ACTIVE_MODEL".to_string(), "opus".to_string())]
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

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
            VibeLaunch::arguments(&mcp, None),
            vec![
                "--trust",
                "--disabled-tools",
                "web_search",
                "--disabled-tools",
                "web_fetch"
            ]
        );
        assert_eq!(
            VibeLaunch::arguments(&mcp, Some("sid-1")),
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
            VibeLaunch::arguments(&mcp, None)
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
        let env = VibeLaunch::environment(&mcp);
        assert!(env.iter().all(|(key, _)| key != "VIBE_DISABLED_TOOLS"));
    }

    #[test]
    fn build_env_enables_experimental_hooks() {
        let mcp = McpServerConfig {
            command: "vmux".to_string(),
            args: vec![],
            cwd: None,
        };
        let env = VibeLaunch::environment(&mcp);
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

    fn vibe_session_app(root: PathBuf) -> App {
        let mut app = App::new();
        app.add_message::<DiscoverAgentSessions>()
            .add_message::<AgentSessionExited>()
            .add_systems(Update, (discover_sessions, detect_ended_sessions));
        app.world_mut().spawn((VibeCli, CliSessionRoot(root)));
        app
    }

    #[test]
    fn discovery_message_claims_matching_pending_session() {
        let tmp = unique_tmp("vibe-ecs-discover");
        let cwd = tmp.join("workspace");
        std::fs::create_dir_all(&cwd).unwrap();
        let spawn_time = SystemTime::now();
        std::thread::sleep(Duration::from_millis(20));
        write_meta(
            &tmp.join("session_20260929_120000_vb1"),
            "full-vb1",
            &cwd.to_string_lossy(),
            "2026-09-29T12:00:00+00:00",
            None,
        );
        let mut app = vibe_session_app(tmp.clone());
        let entity = app
            .world_mut()
            .spawn(PendingAgentSession {
                kind: AgentKind::Vibe,
                spawn_time,
                cwd,
            })
            .id();

        app.world_mut().write_message(DiscoverAgentSessions);
        app.update();

        assert_eq!(
            app.world().get::<SessionId>(entity).map(|id| id.0.as_str()),
            Some("vb1")
        );
        assert!(app.world().get::<PendingAgentSession>(entity).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn discovery_message_keeps_unmatched_pending_session() {
        let tmp = unique_tmp("vibe-ecs-unmatched");
        let mut app = vibe_session_app(tmp.clone());
        let entity = app
            .world_mut()
            .spawn(PendingAgentSession {
                kind: AgentKind::Vibe,
                spawn_time: SystemTime::UNIX_EPOCH,
                cwd: tmp.join("missing"),
            })
            .id();

        app.world_mut().write_message(DiscoverAgentSessions);
        app.update();

        assert!(app.world().get::<PendingAgentSession>(entity).is_some());
        assert!(app.world().get::<SessionId>(entity).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
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
    fn discovery_message_removes_ended_session_and_emits_exit() {
        let tmp = unique_tmp("vibe-end");
        let cwd = "/tmp/work";
        write_meta(
            &tmp.join("a"),
            "ended-id",
            cwd,
            "2026-05-11T12:00:00+00:00",
            Some("2026-05-11T13:00:00+00:00"),
        );
        let mut app = vibe_session_app(tmp.clone());
        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Vibe,
                },
                SessionId("ended-id".to_string()),
            ))
            .id();

        app.world_mut().write_message(DiscoverAgentSessions);
        app.update();

        assert!(app.world().get::<AgentSession>(entity).is_none());
        assert!(app.world().get::<SessionId>(entity).is_none());
        assert_eq!(
            app.world().resource::<Messages<AgentSessionExited>>().len(),
            1
        );
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
