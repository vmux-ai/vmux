use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vmux_core::agent::AgentKind;
use vmux_service::message::Message;

use crate::McpServerConfig;
use crate::strategy::AgentStrategy;

/// A resumable agent session discovered on disk. Runtime-agnostic: `(kind, sid, cwd)`
/// identifies the conversation; how it is opened (ACP vs CLI) is a separate choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumableSession {
    pub kind: AgentKind,
    pub sid: String,
    pub cwd: PathBuf,
    pub mtime: SystemTime,
    /// First user message / summary, or a short sid fallback.
    pub title: String,
    /// True when this kind's ACP and CLI runtimes share the session id (Claude only, for now).
    pub cross_runtime: bool,
}

pub trait CliAgentStrategy: AgentStrategy {
    fn sessions_root(&self) -> PathBuf;
    fn build_args(&self, mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String>;
    fn build_env(&self, mcp: &McpServerConfig) -> Vec<(String, String)>;
    /// Launch-time side effects (e.g. writing a managed hooks config file).
    /// Runs once per spawn, after the MCP config is resolved. Default: nothing.
    fn prepare_launch(&self, _mcp: &McpServerConfig) {}
    fn discover_session(
        &self,
        cwd: &Path,
        spawn_time: SystemTime,
        claimed: &HashSet<String>,
    ) -> Option<String>;
    fn detect_end_time(&self, session_id: &str) -> bool;
    /// List this kind's resumable sessions from its on-disk store. Order is not required
    /// (the collector sorts newest-first). Default: none.
    fn list_sessions(&self) -> Vec<ResumableSession> {
        Vec::new()
    }
    fn load_transcript(&self, session_id: &str) -> Result<Vec<Message>, String> {
        Err(format!("transcript loading unsupported for {session_id}"))
    }
}
