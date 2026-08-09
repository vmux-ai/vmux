use std::path::PathBuf;
use std::time::SystemTime;

use bevy::prelude::*;

use crate::terminal::TerminalKind;

#[derive(
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    bevy::prelude::Reflect,
)]
pub enum AgentKind {
    Vibe,
    Claude,
    Codex,
}

impl AgentKind {
    pub fn executable(self) -> &'static str {
        match self {
            AgentKind::Vibe => "vibe",
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            AgentKind::Vibe => "Vibe",
            AgentKind::Claude => "Claude",
            AgentKind::Codex => "Codex",
        }
    }

    pub fn as_url_segment(self) -> &'static str {
        match self {
            AgentKind::Vibe => "vibe",
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
        }
    }

    pub fn from_url_segment(segment: &str) -> Option<Self> {
        match segment {
            "vibe" => Some(AgentKind::Vibe),
            "claude" => Some(AgentKind::Claude),
            "codex" => Some(AgentKind::Codex),
            _ => None,
        }
    }

    pub fn cli_url_prefix(self) -> String {
        format!("vmux://agent/{}/", self.as_url_segment())
    }

    pub fn setup_url(self) -> String {
        format!("vmux://agent/{}/setup", self.as_url_segment())
    }

    pub fn all() -> [AgentKind; 3] {
        [AgentKind::Vibe, AgentKind::Claude, AgentKind::Codex]
    }
}

/// Reasoning-effort levels selectable for an agent, keyed by agent key — an ACP agent id
/// (`"claude"`) or a CLI key (`"cli:claude"`, `"cli:codex"`). Ordered low→high for display.
/// Empty means the agent exposes no effort knob vmux can drive, and the selector is hidden.
///
/// Only keys vmux actually wires are listed: ACP `claude` (forwarded through the adapter's
/// `claudeCode.options` session meta) and the CLI `claude`/`codex` launch flags. ACP `codex`
/// and `gemini` return empty until their runtimes expose an effort control.
pub fn effort_levels(agent_key: &str) -> &'static [&'static str] {
    match agent_key {
        "claude" | "cli:claude" => &["low", "medium", "high", "max"],
        "cli:codex" => &["minimal", "low", "medium", "high"],
        _ => &[],
    }
}

impl From<AgentKind> for TerminalKind {
    fn from(kind: AgentKind) -> Self {
        match kind {
            AgentKind::Vibe => TerminalKind::Vibe,
            AgentKind::Claude => TerminalKind::Claude,
            AgentKind::Codex => TerminalKind::Codex,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct AgentProviderTargetKind(pub AgentKind);

#[derive(Component, Debug, Clone)]
pub struct AgentSession {
    pub kind: AgentKind,
}

#[derive(Component, Debug, Clone)]
pub struct SessionId(pub String);

#[derive(Component, Debug, Clone)]
pub struct PendingAgentSession {
    pub kind: AgentKind,
    pub spawn_time: SystemTime,
    pub cwd: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

#[derive(Message, Debug, Clone)]
pub struct SpawnAgentInStackRequest {
    pub kind: AgentKind,
    pub cwd: PathBuf,
    pub session_id: Option<String>,
    pub stack: Entity,
    /// Optional prompt to deliver into the agent once its TUI is ready. `None`
    /// opens the agent with no pre-filled prompt.
    pub initial_prompt: Option<String>,
    pub initial_attachments: Vec<vmux_wire::protocol::AgentAttachment>,
}

/// Swap the agent session shown on `stack` in place: tear down the current session and
/// re-attach `target_url` (an ACP or CLI agent url) with the given `cwd`. Same tab position.
/// Used by `/resume` (pick a past session) and the ACP↔CLI runtime handoff (`/cli`).
#[derive(Debug, Clone)]
pub struct StackSessionHandoff {
    pub source_agent: String,
    pub source_kind: AgentKind,
    pub source_sid: String,
    pub messages_json: String,
    pub context: String,
    pub truncated: bool,
}

#[derive(Message, Debug, Clone)]
pub struct SwapStackSession {
    pub stack: Entity,
    pub target_url: String,
    pub cwd: PathBuf,
    pub handoff: Option<StackSessionHandoff>,
}

#[derive(Message, Debug, Clone)]
pub struct PageAgentAttachRequest {
    pub stack: Entity,
    pub provider: String,
    pub model: String,
    pub sid: String,
}

#[derive(Message, Debug, Clone)]
pub struct PageAgentSpawnStackRequest {
    pub pane: Entity,
    pub provider: String,
    pub model: String,
    pub sid: String,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct PageAgentSpawnDefaultRequest {
    pub pane: Entity,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct PageAgentAttachDefaultRequest {
    pub stack: Entity,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct RestartAgentPty {
    pub entity: Entity,
}

pub fn parse_page_agent_url(url: &str) -> Option<(String, String, Option<String>)> {
    let body = url.strip_prefix("vmux://agent/")?;
    let segs: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        [provider, model] => Some(((*provider).to_string(), (*model).to_string(), None)),
        [provider, model, sid] => Some((
            (*provider).to_string(),
            (*model).to_string(),
            Some((*sid).to_string()),
        )),
        _ => None,
    }
}

/// `vmux://agent/<id>` (single segment) → an ACP agent id. Two or more segments are the
/// provider-direct page form ([`parse_page_agent_url`]), so ACP claims the single-segment
/// space without collision.
pub fn parse_acp_agent_url(url: &str) -> Option<String> {
    let body = url.strip_prefix("vmux://agent/")?;
    let segs: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        [id] => Some((*id).to_string()),
        _ => None,
    }
}

#[cfg(test)]
#[path = "agent.test.rs"]
mod tests;
