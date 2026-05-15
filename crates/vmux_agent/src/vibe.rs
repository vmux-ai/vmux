use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

use crate::cli_trait::CliAgentStrategy;
use crate::strategy::AgentStrategy;
use crate::{AgentKind, AgentVariant, McpServerConfig};

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
        let mut args = Vec::new();
        if let Some(sid) = session_id {
            args.push("--resume".to_string());
            args.push(sid.to_string());
        }
        args
    }

    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)> {
        let json = serialize_vibe_mcp_env(mcp);
        vec![("VIBE_MCP_SERVERS".to_string(), json)]
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

#[derive(serde::Deserialize)]
struct MetaJson {
    session_id: String,
    start_time: String,
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
    let spawn_secs = spawn_time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let entries = std::fs::read_dir(sessions_root).ok()?;
    let mut best: Option<(i64, String)> = None;
    for entry in entries.flatten() {
        let meta_path = entry.path().join("meta.json");
        let Ok(text) = std::fs::read_to_string(&meta_path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<MetaJson>(&text) else {
            continue;
        };
        let meta_cwd = normalize_cwd(Path::new(&meta.environment.working_directory));
        if meta_cwd != cwd_norm {
            continue;
        }
        if claimed.contains(&meta.session_id) {
            continue;
        }
        let Ok(start_dt) = chrono::DateTime::parse_from_rfc3339(&meta.start_time) else {
            continue;
        };
        let start_secs = start_dt.timestamp();
        if start_secs < spawn_secs {
            continue;
        }
        match &best {
            None => best = Some((start_secs, meta.session_id)),
            Some((cur, _)) if start_secs < *cur => best = Some((start_secs, meta.session_id)),
            _ => {}
        }
    }
    best.map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
    fn discover_picks_session_matching_cwd_and_after_spawn_time() {
        let tmp = unique_tmp("vibe-discover");
        let sessions = tmp.join("sessions");
        let cwd = "/tmp/work-A";
        write_meta(
            &sessions.join("a"),
            "older",
            cwd,
            "2025-12-31T23:00:00+00:00",
            None,
        );
        write_meta(
            &sessions.join("b"),
            "this",
            cwd,
            "2026-05-11T12:00:00+00:00",
            None,
        );
        write_meta(
            &sessions.join("c"),
            "other",
            "/tmp/work-B",
            "2026-05-11T12:00:00+00:00",
            None,
        );

        let spawn = SystemTime::UNIX_EPOCH + Duration::from_secs(1_770_000_000);
        let claimed = HashSet::new();
        let result = discover_vibe_session_id(&sessions, Path::new(cwd), spawn, &claimed);
        assert_eq!(result.as_deref(), Some("this"));
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
}
