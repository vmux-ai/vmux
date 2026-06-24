use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

use crate::client::cli::strategy::CliAgentStrategy;
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
        // vmux launches vibe non-interactively, so the folder-trust prompt can't
        // be answered. Without trust, vibe runs restricted and ignores the user
        // config (falling back to default models). `--trust` trusts the working
        // directory for this invocation (vibe's documented automation flag).
        let mut args = vec!["--trust".to_string()];
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
        assert_eq!(VibeStrategy.build_args(&mcp, None), vec!["--trust"]);
        assert_eq!(
            VibeStrategy.build_args(&mcp, Some("sid-1")),
            vec!["--trust", "--resume", "sid-1"]
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
}
