use crate::{AgentId, SessionId};
use percent_encoding::percent_decode_str;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Manager,
    Setup,
    Session(SessionId),
    Provider { kind: String, sid: String },
}

impl Route {
    pub const SETUP_URL: &'static str = "vmux://start/onboarding/agent";

    pub fn parse(url: &str) -> Option<Self> {
        let route = vmux_api::VmuxRoute::parse(url)?;
        if route.is_host("start") {
            let segments = route.path_segments().collect::<Vec<_>>();
            return match segments.as_slice() {
                ["onboarding", "agent"]
                    if route.query().is_none() && route.fragment().is_none() =>
                {
                    Some(Self::Setup)
                }
                _ => None,
            };
        }
        if !route.is_host("sessions") {
            return None;
        }
        let segments = route.path_segments().collect::<Vec<_>>();
        match segments.as_slice() {
            [] => Some(Self::Manager),
            ["inbox"] => Some(Self::Manager),
            ["setup"] if route.query().is_none() && route.fragment().is_none() => Some(Self::Setup),
            [id] if route.query().is_none() && route.fragment().is_none() => {
                let id = percent_decode_str(id).decode_utf8().ok()?;
                if let Some(source) = id.strip_prefix("resume/") {
                    let (kind, sid) = source.split_once('/')?;
                    let kind = percent_decode_str(kind).decode_utf8().ok()?.into_owned();
                    let sid = percent_decode_str(sid).decode_utf8().ok()?.into_owned();
                    if !kind.starts_with("acp:")
                        || kind.len() <= 4
                        || sid.is_empty()
                        || sid.len() > 256
                        || sid.chars().any(char::is_control)
                    {
                        return None;
                    }
                    return Some(Self::Provider { kind, sid });
                }
                (!id.is_empty()).then(|| Self::Session(SessionId(id.into_owned())))
            }
            _ => None,
        }
    }

    pub fn url(&self) -> String {
        match self {
            Self::Manager => "vmux://sessions/inbox".to_string(),
            Self::Setup => Self::SETUP_URL.to_string(),
            Self::Session(id) => {
                let mut url = url::Url::parse("vmux://sessions/")
                    .expect("static Session route must be valid");
                url.path_segments_mut()
                    .expect("Session route must support path segments")
                    .push(&id.0);
                url.into()
            }
            Self::Provider { kind, sid } => {
                let kind =
                    percent_encoding::utf8_percent_encode(kind, percent_encoding::NON_ALPHANUMERIC);
                let sid =
                    percent_encoding::utf8_percent_encode(sid, percent_encoding::NON_ALPHANUMERIC);
                let mut url = url::Url::parse("vmux://sessions/")
                    .expect("static provider route must be valid");
                url.path_segments_mut()
                    .expect("provider route must support path segments")
                    .push(&format!("resume/{kind}/{sid}"));
                url.into()
            }
        }
    }

    #[cfg(all(host, feature = "app"))]
    pub fn page_metadata(url: &str) -> Option<vmux_ecs::PageMetadata> {
        let route = Self::parse(url)?;
        let title = match &route {
            Self::Manager => vmux_ui::i18n::translate("sessions-title"),
            Self::Setup => return None,
            Self::Session(id) => id.0.clone(),
            Self::Provider { .. } => vmux_ui::i18n::translate("sessions-title"),
        };
        let url = match &route {
            Self::Manager => Self::manager_url(url).unwrap_or_else(|| route.url()),
            Self::Session(_) | Self::Provider { .. } => route.url(),
            Self::Setup => return None,
        };
        Some(vmux_ecs::PageMetadata {
            url,
            title,
            ..Default::default()
        })
    }

    pub fn manager_for_agent(agent: &AgentId) -> String {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("agent", &agent.0)
            .finish();
        format!("{}?{query}", Self::Manager.url())
    }

    pub fn manager_url(value: &str) -> Option<String> {
        if Self::parse(value) != Some(Self::Manager) {
            return None;
        }
        let mut url = url::Url::parse(value).ok()?;
        url.set_path("/inbox");
        Some(url.into())
    }

    pub fn manager_for_project(project: &str, status: &str) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        if !project.is_empty() {
            query.append_pair("project", project);
        }
        if !status.is_empty() {
            query.append_pair("status", status);
        }
        let query = query.finish();
        if query.is_empty() {
            return Self::Manager.url();
        }
        format!("{}?{query}", Self::Manager.url())
    }

    pub fn manager_filter(url: &str, key: &str) -> Option<String> {
        if Self::parse(url) != Some(Self::Manager) {
            return None;
        }
        let route = vmux_api::VmuxRoute::parse(url)?;
        for (name, value) in url::form_urlencoded::parse(route.query()?.as_bytes()) {
            if name == key && !value.is_empty() {
                return Some(value.into_owned());
            }
        }
        None
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
    fn setup_is_a_dedicated_route_without_collection_filters() {
        let url = Route::Setup.url();
        assert_eq!(Route::parse(&url), Some(Route::Setup));
        assert_eq!(Route::manager_url(&url), None);
        assert_eq!(Route::requested_agent(&url), None);
        assert_eq!(Route::parse("vmux://sessions/setup?agent=codex"), None);
        assert_eq!(Route::parse("vmux://sessions/setup#agent"), None);
    }

    #[test]
    fn inbox_is_a_collection_and_canonicalization_preserves_filter_context() {
        assert_eq!(Route::parse("vmux://sessions/inbox"), Some(Route::Manager));
        let url = Route::manager_url("vmux://sessions/?project=%2Fwork%2Fa&agent=codex").unwrap();
        assert_eq!(url, "vmux://sessions/inbox?project=%2Fwork%2Fa&agent=codex");
        assert_eq!(
            Route::manager_filter(&url, "project").as_deref(),
            Some("/work/a")
        );
        assert_eq!(Route::requested_agent(&url), Some(AgentId("codex".into())));
    }

    #[test]
    fn project_session_links_round_trip_paths_and_activity_filters() {
        let path = "/work/a project/#draft+one";
        let url = Route::manager_for_project(path, "current");
        assert_eq!(Route::parse(&url), Some(Route::Manager));
        assert_eq!(
            Route::manager_filter(&url, "project").as_deref(),
            Some(path)
        );
        assert_eq!(
            Route::manager_filter(&url, "status").as_deref(),
            Some("current")
        );
        assert_eq!(
            Route::manager_filter("vmux://sessions/session-1?project=a", "project"),
            None
        );
    }

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
    fn agent_onboarding_is_a_topic_route_with_a_canonical_legacy_alias() {
        for url in ["vmux://start/onboarding/agent", "vmux://sessions/setup"] {
            let route = Route::parse(url).expect("agent setup route");
            assert_eq!(route, Route::Setup);
            assert_eq!(route.url(), "vmux://start/onboarding/agent");
            assert_eq!(Route::requested_agent(url), None);
        }
        for url in [
            "vmux://start/",
            "vmux://start/onboarding",
            "vmux://start/onboarding/project",
            "vmux://start/onboarding/agent/extra",
            "vmux://start/onboarding/agent?session=other",
            "vmux://start/onboarding/agent#step",
        ] {
            assert_eq!(Route::parse(url), None, "{url}");
        }
    }

    #[test]
    fn pending_provider_routes_preserve_encoded_identity_and_reject_missing_source() {
        let route = Route::Provider {
            kind: "acp:codex test".into(),
            sid: "thread/name?part#one".into(),
        };
        assert_eq!(Route::parse(&route.url()), Some(route));
        assert_eq!(Route::parse("vmux://sessions/resume/acp:/thread"), None);
        assert_eq!(Route::parse("vmux://sessions/resume/codex/thread"), None);
        assert_eq!(Route::parse("vmux://sessions/resume/acp:codex/"), None);
    }

    #[test]
    fn manager_agent_is_transient_creation_context() {
        let url = Route::manager_for_agent(&AgentId("claude acp".into()));

        assert_eq!(url, "vmux://sessions/inbox?agent=claude+acp");
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
