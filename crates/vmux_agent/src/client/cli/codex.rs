use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::client::cli::strategy::CliAgentStrategy;
use crate::strategy::AgentStrategy;
use crate::{AgentKind, AgentVariant, McpServerConfig};

const DISABLED_FEATURES: &[&str] = &["shell_tool", "unified_exec"];
const DIRECT_ONLY_NAMESPACE: &str = "mcp__vmux";
const RUN_STEER_PROMPT: &str = "The native shell and web search tools are disabled. Run ALL shell \
commands via the mcp__vmux__run tool (a visible terminal the user can watch and take over). To READ \
a file, use the mcp__vmux__read_file tool (it shows the file in a pane beside you and returns its \
text) - do NOT cat/sed/head/tail a file via run. To SEARCH code, use the mcp__vmux__grep tool (it \
opens each matching file in a pane and returns the matches) - do NOT run rg/grep/ag via run. Do ALL web access via the vmux browser tools in the \
user's visible browser: mcp__vmux__browser_navigate (it returns the page snapshot on load), then \
mcp__vmux__browser_scroll to read more. Omit the pane argument - it targets your own browser pane. \
Do not look for a built-in web search.";
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
        ];
        if let Some(cwd) = &mcp.cwd {
            args.push("-c".into());
            args.push(format!(
                "mcp_servers.vmux.cwd={}",
                quote_toml(&cwd.to_string_lossy())
            ));
        }
        args.push("-c".into());
        args.push(format!(
            "features.code_mode.direct_only_tool_namespaces=[{}]",
            quote_toml(DIRECT_ONLY_NAMESPACE)
        ));
        args.push("-c".into());
        args.push("tools.web_search=false".to_string());
        args.push("-c".into());
        args.push(format!(
            "developer_instructions={}",
            quote_toml(RUN_STEER_PROMPT)
        ));
        args.push("-c".into());
        args.push("features.hooks=true".into());
        args.push("-c".into());
        args.push(build_file_touch_hook_override(mcp));
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

pub(crate) fn toml_array(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| quote_toml(s)).collect();
    format!("[{}]", inner.join(","))
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

    fn write_session(root: &Path, ymd: &str, file: &str, id: &str, cwd: &str) {
        let dir = root.join(ymd);
        std::fs::create_dir_all(&dir).unwrap();
        let line = format!(
            r#"{{"timestamp":"2026-04-30T11:41:00.170Z","type":"session_meta","payload":{{"id":"{id}","timestamp":"2026-04-30T09:56:21.846Z","cwd":"{cwd}"}}}}"#
        );
        std::fs::write(dir.join(file), format!("{line}\n")).unwrap();
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
}
