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
        format!("vmux://sessions/{}/", self.as_url_segment())
    }

    pub fn setup_url(self) -> String {
        format!("vmux://sessions/{}/setup", self.as_url_segment())
    }

    pub fn is_setup_url(self, url: &str) -> bool {
        url == self.setup_url() || url == format!("vmux://agent/{}/setup", self.as_url_segment())
    }

    pub fn all() -> [AgentKind; 3] {
        [AgentKind::Vibe, AgentKind::Claude, AgentKind::Codex]
    }

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

pub fn supports_inline_agent_transition(url: &str) -> bool {
    let Some(path) = url
        .strip_prefix("vmux://sessions/")
        .or_else(|| url.strip_prefix("vmux://agent/"))
    else {
        return false;
    };
    !path
        .split('/')
        .any(|segment| matches!(segment, "cli" | "setup"))
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
        assert_eq!(AgentKind::Vibe.cli_url_prefix(), "vmux://sessions/vibe/");
        assert_eq!(
            AgentKind::Claude.cli_url_prefix(),
            "vmux://sessions/claude/"
        );
    }

    #[test]
    fn setup_urls_accept_canonical_and_legacy_forms() {
        for kind in AgentKind::all() {
            assert!(kind.is_setup_url(&kind.setup_url()));
            assert!(kind.is_setup_url(&format!("vmux://agent/{}/setup", kind.as_url_segment())));
            assert!(!kind.is_setup_url(&format!(
                "vmux://sessions/{}/session-1",
                kind.as_url_segment()
            )));
        }
    }

    #[test]
    fn legacy_agent_urls_still_support_inline_transition() {
        assert!(supports_inline_agent_transition("vmux://agent/claude"));
        assert!(!supports_inline_agent_transition("vmux://agent/claude/cli"));
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
