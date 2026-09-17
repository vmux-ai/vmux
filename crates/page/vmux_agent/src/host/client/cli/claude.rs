use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Map, Value};

use crate::client::cli::strategy::{
    CliAgentStrategy, CliModelCatalog, PromptHistory, ResumableSession, SameProject,
    lines_skipping_invalid_utf8,
};
use crate::strategy::AgentStrategy;
use crate::{AgentKind, AgentVariant, AssistantBlock, McpServerConfig, Message};

const DISALLOWED_TOOLS: &str = "Bash,Monitor,WebSearch,WebFetch";
const HOST_ALLOWED_TOOLS: &str = "mcp__vmux__run,mcp__vmux__read_terminal,\
mcp__vmux__browser_navigate,mcp__vmux__browser_snapshot,mcp__vmux__browser_scroll,\
mcp__vmux__request_user_choice,mcp__vmux__set_conversation_title,\
mcp__vmux__select_project,mcp__vmux__create_worktree";
const RUN_STEER_PROMPT: &str = "The native Bash, WebSearch, and WebFetch tools are disabled. Run \
ALL shell commands via the mcp__vmux__run tool (a visible terminal the user can watch and take \
over). Use the output returned by run directly; call read_terminal only when run says the command \
is still running. Do ALL web access via the vmux browser tools in the user's visible browser: \
mcp__vmux__browser_navigate (it returns the page snapshot on load), then mcp__vmux__browser_scroll \
to read more. Omit the pane argument - it targets your own browser pane. Do not look for a \
built-in web search. An unbound tab starts in ~/.vmux/projects. Before accessing project files or \
running project commands, call mcp__vmux__select_project with the known project path or omit it to \
open the picker. Paths inside ~/.vmux/projects are selected immediately; paths outside it require \
explicit user approval in the native picker. For a \
new project, first use mcp__vmux__request_user_choice to offer a concrete suggested path and \
Choose existing project. Use ~/.vmux/projects/<remote-host>/<organization>/<repository> when a \
remote is known and ~/.vmux/projects/local/<project> otherwise. If creation is selected, use run \
only to create the empty directory, then select that path. vmux will offer Git initialization and \
use the new project root directly; never call create_worktree for that new project. Do not ask the \
user to invent a folder location. In a previously existing Git project, immediately before any \
edit, write, test, build, or other mutation, call mcp__vmux__create_worktree. If it reports ambiguous existing \
worktrees, ask whether to create or choose an existing path, then call create_worktree with \
create=true or the selected path. Never \
run git worktree add yourself. After project or worktree setup succeeds, continue the original \
request immediately. Never enumerate tool registries or wait for optional tools. If a skill requires \
an unavailable tool, continue with the available tools.";
const FILE_TOUCH_MATCHER: &str = "Read|Edit|Write|MultiEdit";

pub struct ClaudeStrategy;

impl AgentStrategy for ClaudeStrategy {
    fn kind(&self) -> AgentKind {
        AgentKind::Claude
    }

    fn variant(&self) -> AgentVariant {
        AgentVariant::Cli
    }
}

impl CliAgentStrategy for ClaudeStrategy {
    fn sessions_root(&self) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".claude").join("projects")
    }

    fn prompt_history(&self, cwd: &Path) -> Vec<String> {
        let home = std::env::var("HOME").unwrap_or_default();
        let path = PathBuf::from(home).join(".claude").join("history.jsonl");
        let mut spoken = Vec::new();
        for line in PromptHistory::lines_of(&path) {
            let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let Some(project) = entry.get("project").and_then(|v| v.as_str()) else {
                continue;
            };
            if !SameProject::covers(project, cwd) {
                continue;
            }
            let Some(text) = entry.get("display").and_then(|v| v.as_str()) else {
                continue;
            };
            spoken.push(text.to_string());
        }
        PromptHistory::recent(spoken)
    }

    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String> {
        let mut args = vec![
            "--mcp-config".to_string(),
            build_mcp_config_json(mcp),
            "--settings".to_string(),
            build_settings_json(mcp),
            "--disallowedTools".to_string(),
            DISALLOWED_TOOLS.to_string(),
            "--allowedTools".to_string(),
            allowed_tools(crate::managed_mcp::load().into_keys()),
            "--append-system-prompt".to_string(),
            vmux_core::knowledge::AgentPrompt::of(RUN_STEER_PROMPT).into_string(),
        ];
        if let Some(sid) = session_id {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        }
        args
    }

    fn model_catalog(&self) -> CliModelCatalog {
        ClaudeModels::load()
    }

    fn model_args(&self, model: &str) -> Vec<String> {
        vec!["--model".to_string(), model.to_string()]
    }

    fn effort_args(&self, level: &str) -> Vec<String> {
        vec!["--effort".to_string(), level.to_string()]
    }

    fn build_env(&self, _mcp: &McpServerConfig) -> Vec<(String, String)> {
        let mut env = vec![(
            "MCP_TOOL_TIMEOUT".to_string(),
            (crate::mcp::LONG_MCP_TOOL_TIMEOUT_SECS * 1_000).to_string(),
        )];
        env.extend(crate::managed_mcp::McpAuthorization::environment());
        env
    }

    fn discover_session(
        &self,
        cwd: &Path,
        spawn_time: SystemTime,
        claimed: &HashSet<String>,
    ) -> Option<String> {
        let dir = self.sessions_root().join(project_dir_name(cwd));
        discover_claude_session_id(&dir, spawn_time, claimed)
    }

    fn detect_end_time(&self, _session_id: &str) -> bool {
        false
    }

    fn list_sessions(&self) -> Vec<ResumableSession> {
        list_claude_sessions(&self.sessions_root())
    }

    fn latest_message(&self, transcript: &Path) -> String {
        claude_latest_message(transcript)
    }

    fn load_transcript(&self, session_id: &str) -> Result<Vec<Message>, String> {
        load_claude_transcript(&self.sessions_root(), session_id)
    }
}

#[derive(Default, serde::Deserialize)]
struct ClaudeSettings {
    model: Option<String>,
}

struct ClaudeModels;

impl ClaudeModels {
    fn load() -> CliModelCatalog {
        let home = std::env::var("HOME").unwrap_or_default();
        Self::from_settings(&PathBuf::from(home).join(".claude").join("settings.json"))
    }

    fn from_settings(path: &Path) -> CliModelCatalog {
        let configured = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ClaudeSettings>(&bytes).ok())
            .and_then(|settings| settings.model)
            .unwrap_or_default();
        let selected = Self::alias_of(&configured).unwrap_or_else(|| configured.clone());
        let mut models = ["fable", "opus", "sonnet"]
            .into_iter()
            .map(|id| vmux_wire::room::ModelOptionEntry {
                id: id.to_string(),
                name: Self::display_name(id),
                description: String::new(),
            })
            .collect::<Vec<_>>();
        if !selected.is_empty() && !models.iter().any(|model| model.id == selected) {
            models.insert(
                0,
                vmux_wire::room::ModelOptionEntry {
                    id: selected.clone(),
                    name: selected.clone(),
                    description: String::new(),
                },
            );
        }
        CliModelCatalog {
            selected: if selected.is_empty() {
                "sonnet".to_string()
            } else {
                selected
            },
            models,
        }
    }

    fn alias_of(model: &str) -> Option<String> {
        ["fable", "opus", "sonnet"]
            .into_iter()
            .find(|alias| model == *alias || model.contains(&format!("-{alias}-")))
            .map(str::to_string)
    }

    fn display_name(id: &str) -> String {
        let mut chars = id.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        }
    }
}

fn allowed_tools(servers: impl IntoIterator<Item = String>) -> String {
    let mut tools = HOST_ALLOWED_TOOLS.to_string();
    for server in servers {
        tools.push(',');
        tools.push_str("mcp__");
        tools.push_str(&server);
        tools.push_str("__*");
    }
    tools
}

fn project_dir_name(cwd: &Path) -> String {
    let s = cwd.to_string_lossy();
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn build_settings_json(mcp: &McpServerConfig) -> String {
    let anchor = anchor_from_mcp(mcp);
    let args_for = |subcommand: &str| {
        let mut a = vec![Value::String(subcommand.into())];
        if let Some(anchor) = anchor {
            a.push(Value::String("--anchor".into()));
            a.push(Value::String(anchor.into()));
        }
        a
    };
    let value = serde_json::json!({
        "hooks": {
            "Notification": [
                { "hooks": [ { "type": "command", "command": "printf '\\a' > /dev/tty" } ] }
            ],
            "PostToolUse": [
                {
                    "matcher": FILE_TOUCH_MATCHER,
                    "hooks": [
                        { "type": "command", "command": mcp.command, "args": args_for("notify-file-touch"), "async": true }
                    ]
                }
            ],
            "Stop": [
                { "hooks": [ { "type": "command", "command": mcp.command, "args": args_for("notify-turn-end"), "async": true } ] }
            ]
        }
    });
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".into())
}

fn build_mcp_config_json(mcp: &McpServerConfig) -> String {
    let mut server = Map::new();
    server.insert("command".into(), Value::String(mcp.command.clone()));
    server.insert(
        "args".into(),
        Value::Array(mcp.args.iter().map(|s| Value::String(s.clone())).collect()),
    );
    if let Some(cwd) = &mcp.cwd {
        server.insert("cwd".into(), Value::String(cwd.to_string_lossy().into()));
    }
    let mut servers = Map::new();
    servers.insert("vmux".into(), Value::Object(server));
    for (name, server) in crate::managed_mcp::load() {
        servers.insert(
            name.clone(),
            crate::managed_mcp::claude_value(&name, &server),
        );
    }
    let mut root = Map::new();
    root.insert("mcpServers".into(), Value::Object(servers));
    serde_json::to_string(&Value::Object(root)).unwrap_or_else(|_| "{}".into())
}

fn anchor_from_mcp(mcp: &McpServerConfig) -> Option<&str> {
    let i = mcp.args.iter().position(|a| a == "--anchor")?;
    mcp.args.get(i + 1).map(|s| s.as_str())
}

fn discover_claude_session_id(
    project_dir: &Path,
    spawn_time: SystemTime,
    claimed: &HashSet<String>,
) -> Option<String> {
    let entries = std::fs::read_dir(project_dir).ok()?;
    let mut best: Option<(SystemTime, String)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if claimed.contains(stem) {
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
        match &best {
            None => best = Some((created, stem.to_string())),
            Some((cur, _)) if created < *cur => best = Some((created, stem.to_string())),
            _ => {}
        }
    }
    best.map(|(_, id)| id)
}

fn list_claude_sessions(root: &Path) -> Vec<ResumableSession> {
    let mut out = Vec::new();
    let Ok(projects) = std::fs::read_dir(root) else {
        return out;
    };
    for proj in projects.flatten() {
        let Ok(files) = std::fs::read_dir(proj.path()) else {
            continue;
        };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if stem.starts_with("agent-") {
                continue;
            }
            let mtime = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let Some(head) = ClaudeHead::of(&path, stem) else {
                continue;
            };
            out.push(ResumableSession {
                kind: AgentKind::Claude,
                sid: stem.to_string(),
                cwd: head.cwd,
                transcript: path,
                mtime,
                title: head.title,
                cross_runtime: true,
            });
        }
    }
    out
}

struct ClaudeHead {
    cwd: PathBuf,
    title: String,
}

impl ClaudeHead {
    fn of(path: &Path, stem: &str) -> Option<Self> {
        use std::io::{BufRead, BufReader};
        let mut cwd: Option<PathBuf> = None;
        let mut title: Option<String> = None;
        if let Ok(file) = std::fs::File::open(path) {
            for line in BufReader::new(file).lines().take(40).filter_map(Result::ok) {
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if cwd.is_none()
                    && let Some(c) = v.get("cwd").and_then(|c| c.as_str())
                {
                    cwd = Some(PathBuf::from(c));
                }
                if v.get("type").and_then(|t| t.as_str()) != Some("user") {
                    continue;
                }
                if title.is_none() {
                    if Self::is_driven_by_sdk(&v) {
                        return None;
                    }
                    title = ClaudePromptPreview::of(&v);
                }
                if cwd.is_some() && title.is_some() {
                    break;
                }
            }
        }
        let cwd = cwd.unwrap_or_else(|| path.parent().map(Path::to_path_buf).unwrap_or_default());
        let title = title.unwrap_or_else(|| stem.split('-').next().unwrap_or(stem).to_string());
        Some(Self { cwd, title })
    }

    fn is_driven_by_sdk(v: &Value) -> bool {
        let source = v.get("promptSource").and_then(Value::as_str).unwrap_or("");
        let entry = v.get("entrypoint").and_then(Value::as_str).unwrap_or("");
        source == "sdk" || entry.starts_with("sdk")
    }
}

fn claude_latest_message(path: &Path) -> String {
    for line in crate::client::cli::strategy::SessionTail::lines_of(path)
        .iter()
        .rev()
    {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }
        let Some(text) = ClaudePromptPreview::of(&v) else {
            continue;
        };
        return text;
    }
    String::new()
}

struct ClaudePromptPreview;

impl ClaudePromptPreview {
    fn of(v: &Value) -> Option<String> {
        if v.get("isMeta").and_then(Value::as_bool) == Some(true) {
            return None;
        }
        let text = message_text(v)?;
        let mut visible = Vec::new();
        let mut hidden_until = None;
        for line in text.lines() {
            let line = line.trim();
            if let Some(tag) = hidden_until {
                if line.contains(tag) {
                    hidden_until = None;
                }
                continue;
            }
            if line.starts_with("<task-notification>") {
                if !line.contains("</task-notification>") {
                    hidden_until = Some("</task-notification>");
                }
                continue;
            }
            if line.starts_with("<system-reminder>") {
                if !line.contains("</system-reminder>") {
                    hidden_until = Some("</system-reminder>");
                }
                continue;
            }
            if line.starts_with("[Image: source:") && line.ends_with(']') {
                continue;
            }
            let line = line
                .strip_prefix("[Request interrupted by user for tool use]")
                .or_else(|| line.strip_prefix("[Request interrupted by user]"))
                .unwrap_or(line)
                .trim();
            if !line.is_empty() {
                visible.push(line);
            }
        }
        let text = visible.join(" ");
        (!text.is_empty()).then(|| text.chars().take(80).collect())
    }
}

fn message_text(v: &Value) -> Option<String> {
    let content = v.get("message")?.get("content")?;
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn load_claude_transcript(root: &Path, session_id: &str) -> Result<Vec<Message>, String> {
    use std::io::BufReader;

    let mut path = None;
    let projects = std::fs::read_dir(root)
        .map_err(|err| format!("read Claude session root {}: {err}", root.display()))?;
    for project in projects.flatten() {
        let candidate = project.path().join(format!("{session_id}.jsonl"));
        if candidate.is_file() {
            path = Some(candidate);
            break;
        }
    }
    let path = path.ok_or_else(|| format!("Claude session '{session_id}' not found"))?;
    let file = std::fs::File::open(&path)
        .map_err(|err| format!("open Claude session {}: {err}", path.display()))?;
    let mut messages = Vec::new();
    for line in lines_skipping_invalid_utf8(BufReader::new(file)) {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("isMeta").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(text) = message_text(&value) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("user") => messages.push(Message::user(text)),
            Some("assistant") => messages.push(Message::Assistant {
                blocks: vec![AssistantBlock::Text(text)],
            }),
            _ => {}
        }
    }
    if messages.is_empty() {
        return Err(format!(
            "Claude session '{session_id}' has no usable conversation"
        ));
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
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
    fn model_catalog_maps_the_configured_claude_model_to_an_alias() {
        let tmp = unique_tmp("claude-models");
        let settings = tmp.join("settings.json");
        std::fs::write(&settings, r#"{"model":"claude-opus-5"}"#).unwrap();

        let models = ClaudeModels::from_settings(&settings);

        assert_eq!(models.selected, "opus");
        assert!(models.models.iter().any(|model| model.id == "sonnet"));
        assert_eq!(ClaudeStrategy.model_args("opus"), ["--model", "opus"]);
        let _ = std::fs::remove_dir_all(&tmp);
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
    fn managed_mcp_tools_are_allowed_without_an_interactive_terminal_prompt() {
        let allowed = allowed_tools(["linear".to_string(), "notion".to_string()]);

        assert!(allowed.contains("mcp__linear__*"));
        assert!(allowed.contains("mcp__notion__*"));
        assert!(allowed.contains("mcp__vmux__run"));
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
            b"{\"type\":\"user\",\"cwd\":\"/Users/me/proj\",\"message\":{\"role\":\"user\",\"content\":\"fix the auth bug\\n[Image: source: /tmp/auth.png]\"}}\n",
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
    fn latest_message_ignores_generated_user_metadata() {
        let tmp = unique_tmp("claude-latest-preview");
        let path = tmp.join("session.jsonl");
        std::fs::write(
            &path,
            concat!(
                "{\"type\":\"user\",\"message\":{\"content\":\"fix the layout\\n[Image: source: /tmp/layout.png]\"}}\n",
                "{\"type\":\"user\",\"message\":{\"content\":\"<task-notification>\\n<task-id>build</task-id>\\n</task-notification>\"}}\n",
                "{\"type\":\"user\",\"message\":{\"content\":\"[Request interrupted by user]\"}}\n"
            ),
        )
        .unwrap();

        assert_eq!(claude_latest_message(&path), "fix the layout");
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
        transcript
            .extend_from_slice(b"{\"type\":\"assistant\",\"message\":{\"content\":\"after\"}}\n");
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
}
