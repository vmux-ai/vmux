#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentUrl {
    Acp { id: String, sid: Option<String> },
    AcpDefault,
}

impl AgentUrl {
    pub fn parse(url: &str) -> Option<Self> {
        let route = vmux_api::VmuxRoute::parse(url)?;
        if !route.is_agent() {
            return None;
        }
        let segments: Vec<&str> = route.path_segments().collect();
        match segments.as_slice() {
            [] => Some(Self::AcpDefault),
            [id] => Some(Self::Acp {
                id: (*id).to_string(),
                sid: None,
            }),
            [id, sid] if *sid != "cli" => Some(Self::Acp {
                id: (*id).to_string(),
                sid: Some((*sid).to_string()),
            }),
            _ => None,
        }
    }

    pub fn sid(&self) -> &str {
        match self {
            Self::Acp { sid, .. } => sid.as_deref().unwrap_or(""),
            Self::AcpDefault => "",
        }
    }

    pub fn format(&self) -> String {
        match self {
            Self::Acp { id, sid } => match sid {
                Some(sid) => format!("vmux://sessions/{id}/{sid}"),
                None => format!("vmux://sessions/{id}"),
            },
            Self::AcpDefault => "vmux://sessions/".to_string(),
        }
    }

    pub(crate) fn rejects_persisted_store(body: &str) -> bool {
        for prefix in ["vmux://sessions/", "vmux://agent/"] {
            if body.split(prefix).skip(1).any(|tail| {
                let suffix = tail.split('"').next().unwrap_or_default();
                let url = format!("{prefix}{suffix}");
                let normalized = url.trim_end_matches('/');
                !matches!(normalized, "vmux://sessions" | "vmux://agent")
                    && Self::parse(normalized).is_none()
            }) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_fresh_and_resumed_acp_urls() {
        assert_eq!(
            AgentUrl::parse("vmux://sessions/"),
            Some(AgentUrl::AcpDefault)
        );
        assert_eq!(
            AgentUrl::parse("vmux://sessions/mistral-vibe"),
            Some(AgentUrl::Acp {
                id: "mistral-vibe".into(),
                sid: None,
            })
        );
        assert_eq!(
            AgentUrl::parse("vmux://sessions/claude/session-1"),
            Some(AgentUrl::Acp {
                id: "claude".into(),
                sid: Some("session-1".into()),
            })
        );
    }

    #[test]
    fn rejects_removed_cli_and_malformed_routes() {
        assert_eq!(AgentUrl::parse("vmux://sessions/claude/cli"), None);
        assert_eq!(AgentUrl::parse("vmux://sessions/codex/cli/session-1"), None);
        assert_eq!(AgentUrl::parse("vmux://sessions/a/b/c"), None);
    }

    #[test]
    fn acp_urls_round_trip() {
        for url in [
            AgentUrl::Acp {
                id: "claude".into(),
                sid: None,
            },
            AgentUrl::Acp {
                id: "mistral-vibe".into(),
                sid: Some("session-9".into()),
            },
            AgentUrl::AcpDefault,
        ] {
            assert_eq!(AgentUrl::parse(&url.format()), Some(url));
        }
    }

    #[test]
    fn persisted_store_rejects_removed_cli_routes() {
        assert!(AgentUrl::rejects_persisted_store(
            r#"url: "vmux://sessions/codex/cli""#
        ));
        assert!(!AgentUrl::rejects_persisted_store(
            r#"url: "vmux://sessions/codex/session-1""#
        ));
    }
}
