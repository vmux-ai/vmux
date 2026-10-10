use crate::{AgentId, SessionId};
use percent_encoding::percent_decode_str;

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
            [id] if route.query().is_none() && route.fragment().is_none() => {
                let id = percent_decode_str(id).decode_utf8().ok()?;
                (!id.is_empty()).then(|| Self::Session(SessionId(id.into_owned())))
            }
            _ => None,
        }
    }

    pub fn url(&self) -> String {
        match self {
            Self::Manager => "vmux://sessions/".to_string(),
            Self::Session(id) => {
                let mut url = url::Url::parse("vmux://sessions/")
                    .expect("static Session route must be valid");
                url.path_segments_mut()
                    .expect("Session route must support path segments")
                    .push(&id.0);
                url.into()
            }
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
        let mut in_page_metadata = false;
        for line in body.lines() {
            let line = line.trim();
            if line.starts_with("\"vmux_header::system::PageMetadata\":") {
                in_page_metadata = true;
                continue;
            }
            if !in_page_metadata {
                continue;
            }
            if let Some(value) = line.strip_prefix("url: \"")
                && let Some((url, _)) = value.split_once('"')
                && (url.starts_with("vmux://agent")
                    || (url.starts_with("vmux://sessions") && Self::parse(url).is_none()))
            {
                return true;
            }
            if line == ")," {
                in_page_metadata = false;
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
    fn session_ids_round_trip_as_one_encoded_segment() {
        let id = SessionId("task/name?draft#1".into());
        let url = Route::Session(id.clone()).url();

        assert_eq!(Route::parse(&url), Some(Route::Session(id)));
        assert!(!url.contains("?draft"));
        assert!(!url.contains("#1"));
    }

    #[test]
    fn rejects_legacy_agent_urls_in_persisted_state() {
        assert!(Route::rejects_persisted_store(
            r#""vmux_header::system::PageMetadata": (
                url: "vmux://agent/codex/session-1",
            ),"#
        ));
    }

    #[test]
    fn persisted_text_outside_page_metadata_does_not_reject_the_store() {
        assert!(!Route::rejects_persisted_store(
            r#""vmux_ecs::Description": ("see vmux://agent/codex/session-1"),"#
        ));
    }

    #[test]
    fn rejects_only_malformed_session_page_urls() {
        assert!(Route::rejects_persisted_store(
            r#""vmux_header::system::PageMetadata": (
                url: "vmux://sessions/a/b",
            ),"#
        ));
        assert!(!Route::rejects_persisted_store(
            r#""vmux_header::system::PageMetadata": (
                url: "vmux://sessions/task%2Fname",
            ),"#
        ));
    }
}
