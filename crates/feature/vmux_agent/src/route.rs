#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcpRoute {
    Acp { id: String, sid: Option<String> },
    AcpDefault,
}

impl AcpRoute {
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
        let current_root = vmux_chat::ChatPlugin::URL.trim_end_matches('/');
        let legacy_root = "vmux://agent";
        for prefix in [vmux_chat::ChatPlugin::URL, "vmux://agent/"] {
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
