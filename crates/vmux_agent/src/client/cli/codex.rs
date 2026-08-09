use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::client::cli::strategy::{
    CliAgentStrategy, ResumableSession, lines_skipping_invalid_utf8,
};
use crate::strategy::AgentStrategy;
use crate::{AgentKind, AgentVariant, AssistantBlock, McpServerConfig, Message};

const DISABLED_FEATURES: &[&str] = &["shell_tool", "unified_exec"];
pub(crate) const DIRECT_ONLY_NAMESPACE: &str = "mcp__vmux";
pub(crate) const RUN_STEER_PROMPT: &str = "The native shell and web search tools are disabled. Run ALL shell \
commands via the mcp__vmux__run tool (a visible terminal the user can watch and take over). Use the \
output returned by run directly; call read_terminal only when run says the command is still running. To READ \
a file, use the mcp__vmux__read_file tool (it shows the file in a pane beside you and returns its \
text) - do NOT cat/sed/head/tail a file via run. To SEARCH code, use the mcp__vmux__grep tool (it \
opens each matching file in a pane and returns the matches) - do NOT run rg/grep/ag via run. OpenAI's bundled browser skill is disabled inside vmux. \
Never use browser:control-in-app-browser, a Node REPL, agent.browsers, or connector discovery. Do ALL web access via the vmux browser tools in the \
user's visible browser. If the user refers to a page already visible beside you, first call mcp__vmux__browser_snapshot without a pane argument. \
For a new URL, call mcp__vmux__browser_navigate, then mcp__vmux__browser_scroll to read more. Omitting the pane targets the visible browser pane associated with you. \
Do not look for a built-in web search. Read-only inspection may use the current directory or a known \
path directly; never call mcp__vmux__select_project or mcp__vmux__create_worktree for requests \
that only read, show, search, or explain existing files. Before the first mutation in an existing \
project without a selected project, call mcp__vmux__select_project, passing its known path or omitting it to \
choose under ~/.vmux/workspace. For a new project, first use mcp__vmux__request_user_choice to offer \
a concrete suggested path and Choose existing project. Use \
~/.vmux/workspace/<remote-host>/<organization>/<repository> when a remote is known and \
~/.vmux/workspace/local/<project> otherwise. If creation is selected, use run to create the \
empty directory, then select that path. vmux will offer Git initialization and use the new project \
root directly; never call create_worktree for that new project. Do not ask the user to invent a \
folder location. In a previously existing Git project, immediately before any edit, write, test, \
build, or other mutation, call mcp__vmux__create_worktree. If it reports ambiguous existing \
worktrees, ask whether to create or choose an existing path, then call mcp__vmux__create_worktree with \
create=true or the selected path. Never \
run git worktree add yourself. After project or worktree setup succeeds, continue the original \
request immediately. Never enumerate tool registries or wait for optional tools. If a skill requires \
an unavailable tool, continue with the available tools; for website visuals, use code-native design \
or available project assets.";
const FILE_TOUCH_MATCHER: &str = "apply_patch|Edit|Write";

pub struct CodexStrategy;

impl AgentStrategy for CodexStrategy {
    fn kind(&self) -> AgentKind {
        AgentKind::Codex
    }

    fn variant(&self) -> AgentVariant {
        AgentVariant::Cli
    }
}

impl CliAgentStrategy for CodexStrategy {
    fn sessions_root(&self) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".codex").join("sessions")
    }

    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "-c".into(),
            format!("mcp_servers.vmux.command={}", quote_toml(&mcp.command)),
            "-c".into(),
            format!("mcp_servers.vmux.args={}", toml_array(&mcp.args)),
            "-c".into(),
            format!(
                "mcp_servers.vmux.tool_timeout_sec={}",
                crate::mcp::LONG_MCP_TOOL_TIMEOUT_SECS
            ),
        ];
        if let Some(cwd) = &mcp.cwd {
            args.push("-c".into());
            args.push(format!(
                "mcp_servers.vmux.cwd={}",
                quote_toml(&cwd.to_string_lossy())
            ));
        }
        append_managed_mcp_args(&mut args);
        args.push("-c".into());
        args.push(format!(
            "features.code_mode.direct_only_tool_namespaces=[{}]",
            quote_toml(DIRECT_ONLY_NAMESPACE)
        ));
        args.push("-c".into());
        args.push("tools.web_search=false".to_string());
        if let Some(skills) = build_skills_config_override(&codex_disabled_skill_files()) {
            args.push("-c".into());
            args.push(skills);
        }
        args.push("-c".into());
        args.push(format!(
            "developer_instructions={}",
            quote_toml(&vmux_core::knowledge::append_agent_context(
                RUN_STEER_PROMPT
            ))
        ));
        args.push("-c".into());
        args.push("features.hooks=true".into());
        args.push("-c".into());
        args.push(build_file_touch_hook_override(mcp));
        args.push("-c".into());
        args.push(build_turn_end_hook_override(mcp));
        for feature in DISABLED_FEATURES {
            args.push("--disable".into());
            args.push((*feature).to_string());
        }
        if let Some(sid) = session_id {
            args.push("resume".into());
            args.push(sid.to_string());
        }
        args
    }

    fn effort_args(&self, level: &str) -> Vec<String> {
        vec!["-c".to_string(), format!("model_reasoning_effort={level}")]
    }

    fn build_env(&self, _mcp: &McpServerConfig) -> Vec<(String, String)> {
        vec![]
    }

    fn discover_session(
        &self,
        cwd: &Path,
        spawn_time: SystemTime,
        claimed: &HashSet<String>,
    ) -> Option<String> {
        discover_codex_session_id(&self.sessions_root(), cwd, spawn_time, claimed)
    }

    fn detect_end_time(&self, _session_id: &str) -> bool {
        false
    }

    fn list_sessions(&self) -> Vec<ResumableSession> {
        list_codex_sessions(&self.sessions_root())
    }

    fn load_transcript(&self, session_id: &str) -> Result<Vec<Message>, String> {
        load_codex_transcript(&self.sessions_root(), session_id)
    }
}

pub(crate) fn quote_toml(s: &str) -> String {
    let escaped: String = s
        .chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            c => vec![c],
        })
        .collect();
    format!("\"{escaped}\"")
}

fn append_managed_mcp_args(args: &mut Vec<String>) {
    for (name, server) in crate::managed_mcp::load() {
        let prefix = format!("mcp_servers.{}", quote_toml(&name));
        match server.transport {
            vmux_core::profile::tools::McpTransport::Stdio => {
                if let Some(command) = server.command {
                    push_config_override(
                        args,
                        format!("{prefix}.command={}", quote_toml(&command)),
                    );
                }
                if !server.args.is_empty() {
                    push_config_override(
                        args,
                        format!("{prefix}.args={}", toml_array(&server.args)),
                    );
                }
                if !server.env.is_empty() {
                    push_config_override(
                        args,
                        format!("{prefix}.env={}", toml_inline_table(&server.env)),
                    );
                }
                if let Some(cwd) = server.cwd {
                    push_config_override(args, format!("{prefix}.cwd={}", quote_toml(&cwd)));
                }
            }
            vmux_core::profile::tools::McpTransport::Http
            | vmux_core::profile::tools::McpTransport::Sse => {
                if let Some(url) = server.url {
                    push_config_override(args, format!("{prefix}.url={}", quote_toml(&url)));
                }
                if !server.headers.is_empty() {
                    push_config_override(
                        args,
                        format!(
                            "{prefix}.http_headers={}",
                            toml_inline_table(&server.headers)
                        ),
                    );
                }
                if !server.header_env.is_empty() {
                    push_config_override(
                        args,
                        format!(
                            "{prefix}.env_http_headers={}",
                            toml_inline_table(&server.header_env)
                        ),
                    );
                }
                if let Some(variable) = server.bearer_token_env_var {
                    push_config_override(
                        args,
                        format!("{prefix}.bearer_token_env_var={}", quote_toml(&variable)),
                    );
                }
            }
        }
    }
}

fn push_config_override(args: &mut Vec<String>, value: String) {
    args.push("-c".to_string());
    args.push(value);
}

fn toml_inline_table(values: &std::collections::BTreeMap<String, String>) -> String {
    format!(
        "{{{}}}",
        values
            .iter()
            .map(|(key, value)| format!("{}={}", quote_toml(key), quote_toml(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub(crate) fn toml_array(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| quote_toml(s)).collect();
    format!("[{}]", inner.join(","))
}

fn build_skills_config_override(skill_files: &[PathBuf]) -> Option<String> {
    if skill_files.is_empty() {
        return None;
    }
    let entries = skill_files
        .iter()
        .map(|path| {
            format!(
                "{{path={},enabled=false}}",
                quote_toml(&path.to_string_lossy())
            )
        })
        .collect::<Vec<_>>();
    Some(format!("skills.config=[{}]", entries.join(",")))
}

pub(crate) fn codex_disabled_skill_files() -> Vec<PathBuf> {
    let mut files = vmux_core::knowledge::configured_skill_files();
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    collect_skill_files(
        &codex_home.join("plugins/cache/openai-bundled/browser"),
        &mut files,
    );
    files.sort();
    files.dedup();
    files
}

fn collect_skill_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_skill_files(&path, files);
        } else if file_type.is_file()
            && path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
        {
            files.push(path);
        }
    }
}

/// `-c` override registering a PostToolUse hook that pings vmux on file edits.
/// Codex has no structured read tool (reads go via shell), so this is edits
/// only (`apply_patch`/`Edit`/`Write`). Inline TOML array-of-tables.
fn build_file_touch_hook_override(mcp: &McpServerConfig) -> String {
    let mut hook_args = vec![quote_toml("notify-file-touch")];
    if let Some(i) = mcp.args.iter().position(|a| a == "--anchor")
        && let Some(anchor) = mcp.args.get(i + 1)
    {
        hook_args.push(quote_toml("--anchor"));
        hook_args.push(quote_toml(anchor));
    }
    format!(
        "hooks.PostToolUse=[{{matcher={},hooks=[{{type={},command={},args=[{}]}}]}}]",
        quote_toml(FILE_TOUCH_MATCHER),
        quote_toml("command"),
        quote_toml(&mcp.command),
        hook_args.join(","),
    )
}

/// `-c` override registering a Stop hook that pings vmux at turn-end (drives
/// follow-pane auto-tidy + the done-dot). Codex's `Stop` fires when the agent
/// finishes a turn; it takes no tool matcher. Inline TOML array-of-tables.
fn build_turn_end_hook_override(mcp: &McpServerConfig) -> String {
    let mut hook_args = vec![quote_toml("notify-turn-end")];
    if let Some(i) = mcp.args.iter().position(|a| a == "--anchor")
        && let Some(anchor) = mcp.args.get(i + 1)
    {
        hook_args.push(quote_toml("--anchor"));
        hook_args.push(quote_toml(anchor));
    }
    format!(
        "hooks.Stop=[{{hooks=[{{type={},command={},args=[{}]}}]}}]",
        quote_toml("command"),
        quote_toml(&mcp.command),
        hook_args.join(","),
    )
}

fn normalize_cwd(path: &Path) -> String {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canon.to_string_lossy().trim_end_matches('/').to_string()
}

#[derive(serde::Deserialize)]
struct CodexHead {
    #[serde(rename = "type")]
    kind: String,
    payload: CodexHeadPayload,
}

#[derive(serde::Deserialize)]
struct CodexHeadPayload {
    id: String,
    cwd: String,
}

pub(crate) fn discover_codex_session_id(
    sessions_root: &Path,
    cwd: &Path,
    spawn_time: SystemTime,
    claimed: &HashSet<String>,
) -> Option<String> {
    let cwd_norm = normalize_cwd(cwd);
    let mut best: Option<(SystemTime, String)> = None;
    walk_jsonl(sessions_root, &mut |path: &Path| {
        let Ok(meta) = std::fs::metadata(path) else {
            return;
        };
        let Ok(modified) = meta.modified() else {
            return;
        };
        if modified < spawn_time {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let Some(line) = text.lines().next() else {
            return;
        };
        let Ok(head) = serde_json::from_str::<CodexHead>(line) else {
            return;
        };
        if head.kind != "session_meta" {
            return;
        }
        if claimed.contains(&head.payload.id) {
            return;
        }
        let head_cwd = normalize_cwd(Path::new(&head.payload.cwd));
        if head_cwd != cwd_norm {
            return;
        }
        match &best {
            None => best = Some((modified, head.payload.id.clone())),
            Some((cur, _)) if modified < *cur => {
                best = Some((modified, head.payload.id.clone()));
            }
            _ => {}
        }
    });
    best.map(|(_, id)| id)
}

fn walk_jsonl(root: &Path, visit: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_jsonl(&path, visit);
        } else if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            visit(&path);
        }
    }
}

pub(crate) fn list_codex_sessions(root: &Path) -> Vec<ResumableSession> {
    use std::io::{BufRead, BufReader};

    let mut out = Vec::new();
    walk_jsonl(root, &mut |path: &Path| {
        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let Ok(file) = std::fs::File::open(path) else {
            return;
        };
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        let Ok(read) = reader.read_line(&mut line) else {
            return;
        };
        if read == 0 {
            return;
        }
        let Ok(head) = serde_json::from_str::<CodexHead>(line.trim_end()) else {
            return;
        };
        if head.kind != "session_meta" {
            return;
        }
        let fallback = head
            .payload
            .id
            .split('-')
            .next()
            .unwrap_or(&head.payload.id)
            .to_string();
        let title = lines_skipping_invalid_utf8(reader)
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(&line).ok())
            .find_map(|value| {
                (value.get("type").and_then(|value| value.as_str()) == Some("event_msg"))
                    .then(|| value.get("payload"))
                    .flatten()
                    .filter(|payload| {
                        payload.get("type").and_then(|value| value.as_str()) == Some("user_message")
                    })
                    .and_then(|payload| payload.get("message"))
                    .and_then(|message| message.as_str())
                    .map(str::trim)
                    .filter(|message| !message.is_empty())
                    .map(|message| message.lines().collect::<Vec<_>>().join(" "))
                    .map(|message| message.chars().take(80).collect())
            })
            .unwrap_or(fallback);
        out.push(ResumableSession {
            kind: AgentKind::Codex,
            sid: head.payload.id.clone(),
            cwd: PathBuf::from(&head.payload.cwd),
            mtime,
            title,
            cross_runtime: true,
        });
    });
    out
}

pub(crate) fn load_codex_transcript(root: &Path, session_id: &str) -> Result<Vec<Message>, String> {
    use std::io::{BufRead, BufReader};

    let mut session_path = None;
    walk_jsonl(root, &mut |path| {
        if session_path.is_some() {
            return;
        }
        let Ok(file) = std::fs::File::open(path) else {
            return;
        };
        let mut line = String::new();
        let Ok(read) = BufReader::new(file).read_line(&mut line) else {
            return;
        };
        if read == 0 {
            return;
        }
        let Ok(head) = serde_json::from_str::<CodexHead>(line.trim_end()) else {
            return;
        };
        if head.kind == "session_meta" && head.payload.id == session_id {
            session_path = Some(path.to_path_buf());
        }
    });
    let path = session_path.ok_or_else(|| format!("Codex session '{session_id}' not found"))?;
    let file = std::fs::File::open(&path)
        .map_err(|err| format!("open Codex session {}: {err}", path.display()))?;
    let mut messages = Vec::new();
    for line in lines_skipping_invalid_utf8(BufReader::new(file)) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|v| v.as_str()) != Some("event_msg") {
            continue;
        }
        let Some(payload) = value.get("payload") else {
            continue;
        };
        let Some(text) = payload
            .get("message")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
        else {
            continue;
        };
        match payload.get("type").and_then(|v| v.as_str()) {
            Some("user_message") => messages.push(Message::user(text)),
            Some("agent_message") => messages.push(Message::Assistant {
                blocks: vec![AssistantBlock::Text(text.to_string())],
            }),
            _ => {}
        }
    }
    if messages.is_empty() {
        return Err(format!(
            "Codex session '{session_id}' has no usable conversation"
        ));
    }
    Ok(messages)
}

#[cfg(test)]
#[path = "codex.test.rs"]
mod tests;
