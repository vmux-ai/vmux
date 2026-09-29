use crate::AgentKind;
use vmux_core::terminal::TerminalLaunch;

use super::mcp::McpServerConfig;

pub(crate) trait CliLaunchProvider: Send + Sync + 'static {
    const KIND: AgentKind;

    fn arguments(mcp: &McpServerConfig, session_id: Option<&str>) -> Vec<String>;

    fn model_arguments(_model: &str) -> Vec<String> {
        Vec::new()
    }

    fn model_environment(_model: &str) -> Vec<(String, String)> {
        Vec::new()
    }

    fn effort_arguments(_effort: &str) -> Vec<String> {
        Vec::new()
    }

    fn environment(mcp: &McpServerConfig) -> Vec<(String, String)>;

    fn prepare(_mcp: &McpServerConfig) {}
}

pub(crate) struct PreparedAgentLaunch {
    pub(crate) launch: TerminalLaunch,
    pub(crate) mcp_revision: u64,
}

#[derive(bevy::prelude::Component, Clone)]
pub(crate) struct AgentLaunchRequest {
    pub(crate) cwd: std::path::PathBuf,
    pub(crate) shell: String,
    pub(crate) session_id: Option<String>,
    pub(crate) executable: std::path::PathBuf,
    pub(crate) anchor: vmux_core::ProcessId,
    pub(crate) effort: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) kind: AgentKind,
}

#[derive(bevy::prelude::Component, Clone)]
pub(crate) struct AgentRestartRequest {
    pub(crate) launch: TerminalLaunch,
    pub(crate) shell: String,
    pub(crate) session_id: Option<String>,
    pub(crate) anchor: vmux_core::ProcessId,
    pub(crate) kind: AgentKind,
}
