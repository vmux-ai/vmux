use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Map, Value};

use crate::client::cli::strategy::CliAgentStrategy;
use crate::strategy::AgentStrategy;
use crate::{AgentKind, AgentVariant, McpServerConfig};

const DISALLOWED_TOOLS: &str = "Bash,Monitor,WebSearch,WebFetch";
const ALLOWED_TOOLS: &str = "mcp__vmux__run,mcp__vmux__read_terminal,\
mcp__vmux__browser_navigate,mcp__vmux__browser_snapshot,mcp__vmux__browser_scroll";
const RUN_STEER_PROMPT: &str = "The native Bash, WebSearch, and WebFetch tools are disabled. Run \
ALL shell commands via the mcp__vmux__run tool (a visible terminal the user can watch and take \
over). Do ALL web access via the vmux browser tools in the user's visible browser: \
mcp__vmux__browser_navigate (it returns the page snapshot on load), then mcp__vmux__browser_scroll \
to read more. Omit the pane argument - it targets your own browser pane. Do not look for a \
built-in web search.";
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
            RUN_STEER_PROMPT.to_string(),
        ];
        if let Some(sid) = session_id {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        }
        args
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
        let dir = self.sessions_root().join(project_dir_name(cwd));
        discover_claude_session_id(&dir, spawn_time, claimed)
    }

    fn detect_end_time(&self, _session_id: &str) -> bool {
        false
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

        let steer = args
            .iter()
            .position(|a| a == "--append-system-prompt")
            .unwrap();
        assert!(args[steer + 1].contains("mcp__vmux__run"));
        assert!(args[steer + 1].contains("browser_navigate"));
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
}
