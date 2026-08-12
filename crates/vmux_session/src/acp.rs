use bevy_ecs::prelude::*;

/// Marks a stack entity as an ACP agent session. vmux is ACP-only, so this is the agent
/// identity (there is no `AgentVariant`/`AgentKind` for ACP).
#[derive(Component, Clone, Debug)]
pub struct AcpSession {
    pub agent_id: String,
    pub sid: String,
    pub cwd: std::path::PathBuf,
    /// Ties this agent's vmux_mcp tool calls back to its pane (also set as a `ProcessId`
    /// component on the chat webview, where the tool router resolves it).
    pub anchor: vmux_wire::ProcessId,
    /// The agent-assigned ACP session id to resume via `session/load` (from a restored
    /// `vmux://agent/<id>/<acp-session-id>` url). `None` opens a fresh session.
    pub resume: Option<String>,
}
