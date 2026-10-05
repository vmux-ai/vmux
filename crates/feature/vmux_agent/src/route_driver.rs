use crate::route::AcpRoute;

impl AcpRoute {
    pub(crate) fn agent(id: impl Into<String>) -> Self {
        Self::Acp {
            id: id.into(),
            sid: None,
        }
    }

    pub(crate) fn url(&self) -> String {
        match self {
            Self::AcpDefault => vmux_api::VmuxRoute::SESSIONS_ROOT.to_string(),
            Self::Acp { id, sid: None } => {
                format!("{}{id}", vmux_api::VmuxRoute::SESSIONS_ROOT)
            }
            Self::Acp { id, sid: Some(sid) } => {
                format!("{}{id}/{sid}", vmux_api::VmuxRoute::SESSIONS_ROOT)
            }
        }
    }

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

    pub(crate) fn rejects_persisted_store(body: &str) -> bool {
        let current_root = vmux_api::VmuxRoute::SESSIONS_ROOT.trim_end_matches('/');
        let legacy_root = "vmux://agent";
        for prefix in [vmux_api::VmuxRoute::SESSIONS_ROOT, "vmux://agent/"] {
            if body.split(prefix).skip(1).any(|tail| {
                let suffix = tail.split('"').next().unwrap_or_default();
                let url = format!("{prefix}{suffix}");
                let normalized = url.trim_end_matches('/');
                normalized != current_root
                    && normalized != legacy_root
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
            AcpRoute::parse("vmux://sessions/"),
            Some(AcpRoute::AcpDefault)
        );
        assert_eq!(
            AcpRoute::parse("vmux://sessions/mistral-vibe"),
            Some(AcpRoute::Acp {
                id: "mistral-vibe".into(),
                sid: None,
            })
        );
        assert_eq!(
            AcpRoute::parse("vmux://sessions/claude/session-1"),
            Some(AcpRoute::Acp {
                id: "claude".into(),
                sid: Some("session-1".into()),
            })
        );
    }

    #[test]
    fn rejects_removed_cli_and_malformed_routes() {
        assert_eq!(AcpRoute::parse("vmux://sessions/claude/cli"), None);
        assert_eq!(AcpRoute::parse("vmux://sessions/codex/cli/session-1"), None);
        assert_eq!(AcpRoute::parse("vmux://sessions/a/b/c"), None);
    }

    #[test]
    fn persisted_store_rejects_removed_cli_routes() {
        assert!(AcpRoute::rejects_persisted_store(
            r#"url: "vmux://sessions/codex/cli""#
        ));
        assert!(!AcpRoute::rejects_persisted_store(
            r#"url: "vmux://sessions/codex/session-1""#
        ));
    }
}
