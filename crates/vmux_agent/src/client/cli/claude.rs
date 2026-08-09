use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Map, Value};

use crate::client::cli::strategy::{
    CliAgentStrategy, ResumableSession, lines_skipping_invalid_utf8,
};
use crate::strategy::AgentStrategy;
use crate::{AgentKind, AgentVariant, AssistantBlock, McpServerConfig, Message};

const DISALLOWED_TOOLS: &str = "Bash,Monitor,WebSearch,WebFetch";
const ALLOWED_TOOLS: &str = "mcp__vmux__run,mcp__vmux__read_terminal,\
mcp__vmux__browser_navigate,mcp__vmux__browser_snapshot,mcp__vmux__browser_scroll,\
mcp__vmux__request_user_choice,mcp__vmux__set_conversation_title,\
mcp__vmux__select_project,mcp__vmux__create_worktree";
const RUN_STEER_PROMPT: &str = "The native Bash, WebSearch, and WebFetch tools are disabled. Run \
ALL shell commands via the mcp__vmux__run tool (a visible terminal the user can watch and take \
over). Use the output returned by run directly; call read_terminal only when run says the command \
is still running. Do ALL web access via the vmux browser tools in the user's visible browser: \
mcp__vmux__browser_navigate (it returns the page snapshot on load), then mcp__vmux__browser_scroll \
to read more. Omit the pane argument - it targets your own browser pane. Do not look for a \
built-in web search. Read-only inspection may use the current directory or a known path directly; \
never call mcp__vmux__select_project or mcp__vmux__create_worktree for requests that only read, \
show, search, or explain existing files. Before the first mutation in an existing project without a selected project, call \
mcp__vmux__select_project with its known path or omit it to choose under ~/.vmux/workspace. For a \
new project, first use mcp__vmux__request_user_choice to offer a concrete suggested path and \
Choose existing project. Use ~/.vmux/workspace/<remote-host>/<organization>/<repository> when a \
remote is known and ~/.vmux/workspace/local/<project> otherwise. If creation is selected, use run \
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

    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String> {
        let mut args = vec![
            "--mcp-config".to_string(),
            build_mcp_config_json(mcp),
            "--settings".to_string(),
            build_settings_json(mcp),
            "--disallowedTools".to_string(),
            DISALLOWED_TOOLS.to_string(),
            "--allowedTools".to_string(),
            ALLOWED_TOOLS.to_string(),
            "--append-system-prompt".to_string(),
            vmux_core::knowledge::append_agent_context(RUN_STEER_PROMPT),
        ];
        if let Some(sid) = session_id {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        }
        args
    }

    fn effort_args(&self, level: &str) -> Vec<String> {
        vec!["--effort".to_string(), level.to_string()]
    }

    fn build_env(&self, _mcp: &McpServerConfig) -> Vec<(String, String)> {
        vec![(
            "MCP_TOOL_TIMEOUT".to_string(),
            (crate::mcp::LONG_MCP_TOOL_TIMEOUT_SECS * 1_000).to_string(),
        )]
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

    fn load_transcript(&self, session_id: &str) -> Result<Vec<Message>, String> {
        load_claude_transcript(&self.sessions_root(), session_id)
    }
}

pub(crate) fn project_dir_name(cwd: &Path) -> String {
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

/// Inline `--settings` JSON merging three vmux hooks (merges with the user's
/// `~/.claude/settings.json`, does not modify it): a Notification bell; a
/// PostToolUse hook that pings vmux on every file read/edit; and a Stop hook
/// that pings vmux at turn-end (drives follow-pane auto-tidy + the done-dot).
/// Both vmux pings are `async` so they never block the agent.
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
        servers.insert(name, crate::managed_mcp::claude_value(&server));
    }
    let mut root = Map::new();
    root.insert("mcpServers".into(), Value::Object(servers));
    serde_json::to_string(&Value::Object(root)).unwrap_or_else(|_| "{}".into())
}

fn anchor_from_mcp(mcp: &McpServerConfig) -> Option<&str> {
    let i = mcp.args.iter().position(|a| a == "--anchor")?;
    mcp.args.get(i + 1).map(|s| s.as_str())
}

pub(crate) fn discover_claude_session_id(
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

pub(crate) fn list_claude_sessions(root: &Path) -> Vec<ResumableSession> {
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
            let (cwd, title) = claude_cwd_and_title(&path, stem);
            out.push(ResumableSession {
                kind: AgentKind::Claude,
                sid: stem.to_string(),
                cwd,
                mtime,
                title,
                cross_runtime: true,
            });
        }
    }
    out
}

/// Read the first lines of a claude `.jsonl` to recover the working dir and a title.
/// `cwd` is taken from the first line carrying a string `cwd`; `title` from the first user
/// message text. Both fall back gracefully (cwd → the file's parent, title → short sid).
fn claude_cwd_and_title(path: &Path, stem: &str) -> (PathBuf, String) {
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
            if title.is_none()
                && v.get("type").and_then(|t| t.as_str()) == Some("user")
                && let Some(text) = user_message_text(&v)
            {
                title = Some(text);
            }
            if cwd.is_some() && title.is_some() {
                break;
            }
        }
    }
    let cwd = cwd.unwrap_or_else(|| path.parent().map(Path::to_path_buf).unwrap_or_default());
    let title = title.unwrap_or_else(|| stem.split('-').next().unwrap_or(stem).to_string());
    (cwd, title)
}

/// Extract plain text from a claude `message.content` (string, or an array of `{type,text}`).
fn user_message_text(v: &Value) -> Option<String> {
    message_text(v).map(|text| text.chars().take(80).collect())
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

pub(crate) fn load_claude_transcript(
    root: &Path,
    session_id: &str,
) -> Result<Vec<Message>, String> {
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
#[path = "claude.test.rs"]
mod tests;
