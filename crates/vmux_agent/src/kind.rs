#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
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

    pub fn url_scheme(self) -> &'static str {
        match self {
            AgentKind::Vibe => "vmux://vibe/",
            AgentKind::Claude => "vmux://claude/",
            AgentKind::Codex => "vmux://codex/",
        }
    }

    pub fn from_host(host: &str) -> Option<Self> {
        match host {
            "vibe" => Some(AgentKind::Vibe),
            "claude" => Some(AgentKind::Claude),
            "codex" => Some(AgentKind::Codex),
            _ => None,
        }
    }

    pub fn all() -> [AgentKind; 3] {
        [AgentKind::Vibe, AgentKind::Claude, AgentKind::Codex]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_host_recognizes_known_schemes() {
        assert_eq!(AgentKind::from_host("vibe"), Some(AgentKind::Vibe));
        assert_eq!(AgentKind::from_host("claude"), Some(AgentKind::Claude));
        assert_eq!(AgentKind::from_host("codex"), Some(AgentKind::Codex));
        assert_eq!(AgentKind::from_host("nope"), None);
    }

    #[test]
    fn executable_returns_cli_binary_name() {
        assert_eq!(AgentKind::Vibe.executable(), "vibe");
        assert_eq!(AgentKind::Claude.executable(), "claude");
        assert_eq!(AgentKind::Codex.executable(), "codex");
    }

    #[test]
    fn url_scheme_returns_vmux_prefix_with_trailing_slash() {
        assert_eq!(AgentKind::Vibe.url_scheme(), "vmux://vibe/");
        assert_eq!(AgentKind::Claude.url_scheme(), "vmux://claude/");
        assert_eq!(AgentKind::Codex.url_scheme(), "vmux://codex/");
    }
}
