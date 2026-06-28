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

    pub fn compose_url(self) -> String {
        format!("vmux://compose/{}/", self.as_url_segment())
    }

    pub fn all() -> [AgentKind; 3] {
        [AgentKind::Vibe, AgentKind::Claude, AgentKind::Codex]
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
    /// Prompt to deliver into the agent once its TUI is ready. `None` opens the
    /// agent with no pre-filled prompt.
    pub initial_prompt: Option<String>,
    /// Show the typeable loading page over the booting terminal so the user can
    /// compose a first prompt while the agent boots (delivered when ready).
    pub compose: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_url_segment_recognizes_known_kinds() {
        assert_eq!(AgentKind::from_url_segment("vibe"), Some(AgentKind::Vibe));
        assert_eq!(
            AgentKind::from_url_segment("claude"),
            Some(AgentKind::Claude)
        );
        assert_eq!(AgentKind::from_url_segment("codex"), Some(AgentKind::Codex));
        assert_eq!(AgentKind::from_url_segment("nope"), None);
    }

    #[test]
    fn executable_returns_cli_binary_name() {
        assert_eq!(AgentKind::Vibe.executable(), "vibe");
        assert_eq!(AgentKind::Claude.executable(), "claude");
        assert_eq!(AgentKind::Codex.executable(), "codex");
    }

    #[test]
    fn cli_url_prefix_returns_three_segment_form() {
        assert_eq!(AgentKind::Vibe.cli_url_prefix(), "vmux://agent/vibe/");
        assert_eq!(AgentKind::Claude.cli_url_prefix(), "vmux://agent/claude/");
    }

    #[test]
    fn agent_kind_into_terminal_kind() {
        assert_eq!(TerminalKind::from(AgentKind::Vibe), TerminalKind::Vibe);
        assert_eq!(TerminalKind::from(AgentKind::Claude), TerminalKind::Claude);
        assert_eq!(TerminalKind::from(AgentKind::Codex), TerminalKind::Codex);
    }

    #[test]
    fn parse_page_agent_url_provider_model_only() {
        let (provider, model, sid) = parse_page_agent_url("vmux://agent/openai/gpt-5.5").unwrap();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-5.5");
        assert!(sid.is_none());
    }

    #[test]
    fn parse_page_agent_url_with_sid() {
        let (provider, model, sid) =
            parse_page_agent_url("vmux://agent/anthropic/claude-opus-4.7/xHigh").unwrap();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-opus-4.7");
        assert_eq!(sid.as_deref(), Some("xHigh"));
    }

    #[test]
    fn parse_page_agent_url_rejects_single_segment() {
        assert!(parse_page_agent_url("vmux://agent/vibe").is_none());
    }

    #[test]
    fn parse_page_agent_url_rejects_too_many_segments() {
        assert!(parse_page_agent_url("vmux://agent/openai/gpt/sid/extra").is_none());
    }

    #[test]
    fn parse_page_agent_url_rejects_non_agent_host() {
        assert!(parse_page_agent_url("https://google.com").is_none());
    }
}
