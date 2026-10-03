use crate::{AgentId, SessionId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Manager,
    Session(SessionId),
}

impl Route {
    pub fn parse(url: &str) -> Option<Self> {
        let route = vmux_api::VmuxRoute::parse(url)?;
        if !route.is_host("sessions") {
            return None;
        }
        let segments = route.path_segments().collect::<Vec<_>>();
        match segments.as_slice() {
            [] => Some(Self::Manager),
            [id] if !id.is_empty() => Some(Self::Session(SessionId((*id).to_string()))),
            _ => None,
        }
    }

    pub fn url(&self) -> String {
        match self {
            Self::Manager => "vmux://sessions/".to_string(),
            Self::Session(id) => format!("vmux://sessions/{}", id.0),
        }
    }

    pub fn manager_for_agent(agent: &AgentId) -> String {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("agent", &agent.0)
            .finish();
        format!("{}?{query}", Self::Manager.url())
    }

    pub fn requested_agent(url: &str) -> Option<AgentId> {
        if Self::parse(url) != Some(Self::Manager) {
            return None;
        }
        let route = vmux_api::VmuxRoute::parse(url)?;
        url::form_urlencoded::parse(route.query()?.as_bytes()).find_map(|(key, value)| {
            (key == "agent" && !value.is_empty()).then(|| AgentId(value.into()))
        })
    }

    pub fn rejects_persisted_store(body: &str) -> bool {
        if body.contains("vmux://agent") {
            return true;
        }
        let root = "vmux://sessions/";
        for tail in body.split(root).skip(1) {
            let suffix = tail.split('"').next().unwrap_or_default();
            let url = format!("{root}{suffix}");
            if Self::parse(url.trim_end_matches('/')).is_none() {
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
    fn parses_manager_and_session_routes() {
        assert_eq!(Route::parse("vmux://sessions/"), Some(Route::Manager));
        assert_eq!(
            Route::parse("vmux://sessions/01a0fc43-68f9-7121-a388-59e1891fdc41"),
            Some(Route::Session(SessionId(
                "01a0fc43-68f9-7121-a388-59e1891fdc41".into()
            )))
        );
    }

    #[test]
    fn manager_agent_is_transient_creation_context() {
        let url = Route::manager_for_agent(&AgentId("claude acp".into()));

        assert_eq!(url, "vmux://sessions/?agent=claude+acp");
        assert_eq!(
            Route::requested_agent(&url),
            Some(AgentId("claude acp".into()))
        );
        assert_eq!(Route::parse(&url), Some(Route::Manager));
        assert_eq!(Route::requested_agent("vmux://sessions/session-1"), None);
    }

    #[test]
    fn rejects_agent_and_provider_session_segments() {
        assert_eq!(Route::parse("vmux://agent/codex"), None);
        assert_eq!(Route::parse("vmux://sessions/codex/acp-session"), None);
        assert_eq!(Route::parse("vmux://sessions/a/b/c"), None);
    }

    #[test]
    fn rejects_legacy_agent_urls_in_persisted_state() {
        assert!(Route::rejects_persisted_store(
            r#"url: "vmux://agent/codex/session-1""#
        ));
    }
}
