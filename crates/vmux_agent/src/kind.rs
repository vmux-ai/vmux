use crate::AgentVariant;

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

    pub fn url_prefix(self, variant: AgentVariant) -> String {
        match variant {
            AgentVariant::Gui => format!("vmux://{}/", self.as_url_segment()),
            AgentVariant::Cli => format!("vmux://agent/{}/cli/", self.as_url_segment()),
        }
    }

    pub fn all() -> [AgentKind; 3] {
        [AgentKind::Vibe, AgentKind::Claude, AgentKind::Codex]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentUrl {
    pub kind: AgentKind,
    pub variant: AgentVariant,
    pub sid: String,
}

impl AgentUrl {
    pub fn parse(url: &str) -> Option<Self> {
        if let Some(body) = url.strip_prefix("vmux://agent/") {
            let mut segs = body.split('/').filter(|s| !s.is_empty());
            let kind = AgentKind::from_url_segment(segs.next()?)?;
            let after_kind = segs.next()?;
            let (variant, sid) = match AgentVariant::from_url_segment(Some(after_kind)) {
                Some(AgentVariant::Cli) => (AgentVariant::Cli, segs.next()?.to_string()),
                _ => (AgentVariant::Gui, after_kind.to_string()),
            };
            if segs.next().is_some() {
                return None;
            }
            return Some(AgentUrl { kind, variant, sid });
        }
        for kind in AgentKind::all() {
            let prefix = format!("vmux://{}/", kind.as_url_segment());
            if let Some(rest) = url.strip_prefix(&prefix) {
                if rest.is_empty() || rest.contains('/') {
                    return None;
                }
                return Some(AgentUrl {
                    kind,
                    variant: AgentVariant::Gui,
                    sid: rest.to_string(),
                });
            }
        }
        None
    }

    pub fn format(&self) -> String {
        format!("{}{}", self.kind.url_prefix(self.variant), self.sid)
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
    fn url_prefix_gui_is_flat_cli_is_nested() {
        assert_eq!(
            AgentKind::Vibe.url_prefix(AgentVariant::Gui),
            "vmux://vibe/"
        );
        assert_eq!(
            AgentKind::Claude.url_prefix(AgentVariant::Cli),
            "vmux://agent/claude/cli/"
        );
    }

    #[test]
    fn flat_gui_url_parses_canonical() {
        let parsed = AgentUrl::parse("vmux://vibe/abc-123").unwrap();
        assert_eq!(parsed.kind, AgentKind::Vibe);
        assert_eq!(parsed.variant, AgentVariant::Gui);
        assert_eq!(parsed.sid, "abc-123");
    }

    #[test]
    fn nested_agent_gui_url_parses_alias() {
        let parsed = AgentUrl::parse("vmux://agent/vibe/abc-123").unwrap();
        assert_eq!(parsed.kind, AgentKind::Vibe);
        assert_eq!(parsed.variant, AgentVariant::Gui);
        assert_eq!(parsed.sid, "abc-123");
    }

    #[test]
    fn nested_cli_url_parses() {
        let parsed = AgentUrl::parse("vmux://agent/claude/cli/abc-123").unwrap();
        assert_eq!(parsed.kind, AgentKind::Claude);
        assert_eq!(parsed.variant, AgentVariant::Cli);
        assert_eq!(parsed.sid, "abc-123");
    }

    #[test]
    fn unknown_kind_returns_none() {
        assert!(AgentUrl::parse("vmux://agent/nope/abc").is_none());
        assert!(AgentUrl::parse("vmux://nope/abc").is_none());
    }

    #[test]
    fn url_format_round_trips_gui_canonical() {
        let u = AgentUrl {
            kind: AgentKind::Vibe,
            variant: AgentVariant::Gui,
            sid: "xyz".into(),
        };
        assert_eq!(u.format(), "vmux://vibe/xyz");
        assert_eq!(AgentUrl::parse(&u.format()), Some(u));
    }

    #[test]
    fn url_format_round_trips_cli() {
        let u = AgentUrl {
            kind: AgentKind::Codex,
            variant: AgentVariant::Cli,
            sid: "xyz".into(),
        };
        assert_eq!(u.format(), "vmux://agent/codex/cli/xyz");
        assert_eq!(AgentUrl::parse(&u.format()), Some(u));
    }

    #[test]
    fn trailing_garbage_after_gui_sid_rejected() {
        assert_eq!(AgentUrl::parse("vmux://agent/vibe/abc/extra"), None);
        assert_eq!(AgentUrl::parse("vmux://vibe/abc/extra"), None);
    }

    #[test]
    fn trailing_garbage_after_cli_sid_rejected() {
        assert_eq!(AgentUrl::parse("vmux://agent/vibe/cli/abc/extra"), None);
    }

    #[test]
    fn prefix_only_url_rejected() {
        assert_eq!(AgentUrl::parse("vmux://vibe/"), None);
        assert_eq!(AgentUrl::parse("vmux://agent/vibe/"), None);
        assert_eq!(AgentUrl::parse("vmux://agent/vibe/cli/"), None);
    }
}
