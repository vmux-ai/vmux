//! The built-in CLI agents, named on both sides of the wire.

use crate::avatar::{AvatarSpec, agent_color};

#[cfg_attr(bevy_linked, derive(bevy_reflect::Reflect))]
#[cfg_attr(bevy_linked, type_path = "vmux_core::agent")]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

    /// How this agent is drawn without a picture.
    pub fn avatar(self) -> AvatarSpec {
        AvatarSpec {
            initials: match self {
                AgentKind::Claude => "CL",
                AgentKind::Codex => "CX",
                AgentKind::Vibe => "VB",
            }
            .into(),
            color: agent_color(self.as_url_segment()),
        }
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
    fn agent_avatar_is_kind_specific() {
        assert_eq!(AgentKind::Claude.avatar().initials, "CL");
        assert_ne!(
            AgentKind::Codex.avatar().color,
            AgentKind::Vibe.avatar().color
        );
    }
}
