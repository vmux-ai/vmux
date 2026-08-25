use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vmux_core::agent::AgentKind;
use vmux_service::message::Message;

use crate::McpServerConfig;
use crate::strategy::AgentStrategy;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumableSession {
    pub kind: AgentKind,
    pub sid: String,
    pub cwd: PathBuf,
    pub mtime: SystemTime,
    pub title: String,
    pub cross_runtime: bool,
}

pub(crate) fn lines_skipping_invalid_utf8<R: std::io::BufRead>(
    reader: R,
) -> impl Iterator<Item = String> {
    reader
        .lines()
        .map_while(|line| match line {
            Ok(line) => Some(Some(line)),
            Err(err) if err.kind() == std::io::ErrorKind::InvalidData => Some(None),
            Err(_) => None,
        })
        .flatten()
}

pub trait CliAgentStrategy: AgentStrategy {
    fn sessions_root(&self) -> PathBuf;
    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String>;
    fn effort_args(&self, _level: &str) -> Vec<String> {
        Vec::new()
    }

    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)>;
    fn prepare_launch(&self, _mcp: &McpServerConfig) {}
    fn discover_session(
        &self,
        cwd: &Path,
        spawn_time: SystemTime,
        claimed: &HashSet<String>,
    ) -> Option<String>;
    fn detect_end_time(&self, session_id: &str) -> bool;
    fn list_sessions(&self) -> Vec<ResumableSession> {
        Vec::new()
    }

    fn load_transcript(&self, session_id: &str) -> Result<Vec<Message>, String> {
        Err(format!("transcript loading unsupported for {session_id}"))
    }
}
