use serde_json::Value;
use vmux_core::extension::match_pattern::ChromeMatchPattern;
use vmux_core::extension::protocol::{ApiRequest, ChromeError};

use super::bridge::BridgeAuthorization;
use super::model::{ChromeModel, ChromeTab};
use super::windows::WINDOW_ID_CURRENT;

pub(crate) struct ChromeTabs<'a> {
    model: &'a ChromeModel,
}

impl<'a> From<&'a ChromeModel> for ChromeTabs<'a> {
    fn from(model: &'a ChromeModel) -> Self {
        Self { model }
    }
}

impl ChromeTabs<'_> {
    pub(crate) fn dispatch(
        &self,
        request: &ApiRequest,
        authorization: &BridgeAuthorization,
    ) -> Result<Value, ChromeError> {
        match request.method.as_str() {
            "query" => Ok(self.query(request, authorization)),
            "get" => self.get(request, authorization),
            method => Err(ChromeError::new(
                "unsupported_api",
                format!("tabs.{method} is not supported"),
            )),
        }
    }

    fn query(&self, request: &ApiRequest, authorization: &BridgeAuthorization) -> Value {
        let filter = TabFilter::from_options(Self::argument(request));
        let mut matched = Vec::new();
        for tab in &self.model.tabs {
            if filter.matches(tab, self.focused_window()) {
                matched.push(tab.clone().disclosed_value(
                    tab.window_id,
                    tab.index,
                    request,
                    authorization,
                ));
            }
        }
        Value::Array(matched)
    }

    fn get(
        &self,
        request: &ApiRequest,
        authorization: &BridgeAuthorization,
    ) -> Result<Value, ChromeError> {
        let id = Self::argument(request)
            .and_then(Value::as_i64)
            .and_then(|id| i32::try_from(id).ok());
        let Some(id) = id else {
            return Err(ChromeError::new(
                "invalid_argument",
                "tabs.get needs a tab id",
            ));
        };
        let Some(tab) = self.model.tabs.iter().find(|tab| tab.id == id) else {
            return Err(ChromeError::new(
                "no_such_tab",
                format!("No tab with id: {id}."),
            ));
        };
        Ok(tab
            .clone()
            .disclosed_value(tab.window_id, tab.index, request, authorization))
    }

    fn argument(request: &ApiRequest) -> Option<&Value> {
        match &request.arguments {
            Value::Array(arguments) => arguments.first(),
            value => Some(value),
        }
    }

    fn focused_window(&self) -> Option<i32> {
        for window in &self.model.windows {
            if window.focused {
                return Some(window.id);
            }
        }
        self.model.windows.first().map(|window| window.id)
    }
}

struct TabFilter {
    active: Option<bool>,
    pinned: Option<bool>,
    highlighted: Option<bool>,
    status: Option<String>,
    window_id: Option<i32>,
    index: Option<u32>,
    current_window: bool,
    urls: Option<UrlFilter>,
}

struct UrlFilter {
    patterns: Vec<String>,
}

impl From<&Value> for UrlFilter {
    fn from(url: &Value) -> Self {
        let mut patterns = Vec::new();
        match url {
            Value::String(single) => patterns.push(single.clone()),
            Value::Array(many) => {
                for entry in many {
                    if let Some(pattern) = entry.as_str() {
                        patterns.push(pattern.to_string());
                    }
                }
            }
            _ => {}
        }
        Self { patterns }
    }
}

impl UrlFilter {
    fn matches(&self, url: &str) -> bool {
        for pattern in &self.patterns {
            if let Ok(parsed) = ChromeMatchPattern::parse(pattern) {
                if url::Url::parse(url).is_ok_and(|url| parsed.matches(&url)) {
                    return true;
                }
                continue;
            }
            if Self::glob(pattern, url) {
                return true;
            }
        }
        false
    }

    fn glob(pattern: &str, value: &str) -> bool {
        let mut rest = value;
        let mut segments = pattern.split('*');
        let Some(first) = segments.next() else {
            return pattern == value;
        };
        let Some(stripped) = rest.strip_prefix(first) else {
            return false;
        };
        rest = stripped;
        let mut last = None;
        for segment in segments {
            last = Some(segment);
            if segment.is_empty() {
                continue;
            }
            let Some(at) = rest.find(segment) else {
                return false;
            };
            rest = &rest[at + segment.len()..];
        }
        match last {
            Some(segment) if !segment.is_empty() => rest.is_empty(),
            Some(_) => true,
            None => rest.is_empty(),
        }
    }
}

impl TabFilter {
    fn from_options(options: Option<&Value>) -> Self {
        let Some(options) = options.and_then(Value::as_object) else {
            return Self::everything();
        };
        let read_bool = |key: &str| options.get(key).and_then(Value::as_bool);
        let requested_window = options
            .get("windowId")
            .and_then(Value::as_i64)
            .map(|id| id as i32);
        let current_window = read_bool("currentWindow").unwrap_or_default()
            || read_bool("lastFocusedWindow").unwrap_or_default()
            || requested_window == Some(WINDOW_ID_CURRENT);
        Self {
            active: read_bool("active"),
            pinned: read_bool("pinned"),
            highlighted: read_bool("highlighted"),
            status: options
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_string),
            window_id: requested_window.filter(|id| *id != WINDOW_ID_CURRENT),
            index: options
                .get("index")
                .and_then(Value::as_u64)
                .map(|index| index as u32),
            current_window,
            urls: options.get("url").map(UrlFilter::from),
        }
    }

    fn everything() -> Self {
        Self {
            active: None,
            pinned: None,
            highlighted: None,
            status: None,
            window_id: None,
            index: None,
            current_window: false,
            urls: None,
        }
    }

    fn matches(&self, tab: &ChromeTab, focused_window: Option<i32>) -> bool {
        if self.active.is_some_and(|active| active != tab.active) {
            return false;
        }
        if self.pinned.is_some_and(|pinned| pinned != tab.pinned) {
            return false;
        }
        if self
            .highlighted
            .is_some_and(|highlighted| highlighted != tab.highlighted)
        {
            return false;
        }
        if self
            .status
            .as_ref()
            .is_some_and(|status| status != &tab.status)
        {
            return false;
        }
        if self.window_id.is_some_and(|id| id != tab.window_id) {
            return false;
        }
        if self.index.is_some_and(|index| index != tab.index) {
            return false;
        }
        if self.current_window && focused_window.is_some_and(|focused| focused != tab.window_id) {
            return false;
        }
        let Some(urls) = &self.urls else {
            return true;
        };
        urls.matches(&tab.url)
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::ChromeWindow;
    use super::*;
    use serde_json::json;
    use std::collections::HashSet;
    use vmux_core::extension::protocol::ExtensionCallerContext;

    impl ChromeModel {
        fn fixture() -> Self {
            Self {
                windows: vec![
                    ChromeWindow::fixture(1, false),
                    ChromeWindow::fixture(2, true),
                ],
                tabs: vec![
                    ChromeTab::fixture(10, 1, 0, false, "https://example.test/one"),
                    ChromeTab::fixture(11, 2, 0, false, "https://example.test/two"),
                    ChromeTab::fixture(12, 2, 1, true, "https://accounts.google.com/signin"),
                ],
            }
        }
    }

    impl ChromeWindow {
        fn fixture(id: i32, focused: bool) -> Self {
            Self {
                id,
                focused,
                left: 0,
                top: 0,
                width: 800,
                height: 600,
                incognito: false,
                window_type: "normal".into(),
                state: "normal".into(),
                always_on_top: false,
            }
        }
    }

    impl ChromeTab {
        fn fixture(id: i32, window_id: i32, index: u32, active: bool, url: &str) -> Self {
            Self {
                id,
                window_id,
                index,
                active,
                highlighted: active,
                pinned: false,
                url: url.into(),
                title: "page".into(),
                status: "complete".into(),
            }
        }
    }

    impl BridgeAuthorization {
        fn fixture() -> Self {
            Self {
                permissions: HashSet::from(["tabs".to_string()]),
                host_permissions: Vec::new(),
                conformance: false,
            }
        }
    }

    struct WorkerCall;

    impl WorkerCall {
        fn to(method: &str, arguments: Vec<Value>) -> ApiRequest {
            ApiRequest {
                request_id: "1".into(),
                namespace: "tabs".into(),
                method: method.into(),
                arguments: Value::Array(arguments),
                caller_context: ExtensionCallerContext::ServiceWorker {
                    extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                    context_id: "ctx".into(),
                    url: None,
                },
            }
        }
    }

    fn ids(value: &Value) -> Vec<i64> {
        value
            .as_array()
            .expect("array")
            .iter()
            .map(|tab| tab["id"].as_i64().expect("id"))
            .collect()
    }

    #[test]
    fn the_active_tab_of_the_focused_window_is_the_one_a_content_script_sits_in() {
        let model = ChromeModel::fixture();
        let request = WorkerCall::to(
            "query",
            vec![json!({ "active": true, "currentWindow": true })],
        );

        let result = ChromeTabs::from(&model)
            .dispatch(&request, &BridgeAuthorization::fixture())
            .expect("query answers");

        assert_eq!(ids(&result), [12]);
    }

    #[test]
    fn a_url_pattern_narrows_the_query_to_the_pages_it_matches() {
        let model = ChromeModel::fixture();
        let request = WorkerCall::to(
            "query",
            vec![json!({ "url": ["https://accounts.google.com/*"] })],
        );

        let result = ChromeTabs::from(&model)
            .dispatch(&request, &BridgeAuthorization::fixture())
            .expect("query answers");

        assert_eq!(ids(&result), [12]);
    }

    #[test]
    fn a_scheme_chrome_patterns_cannot_express_still_narrows_the_query() {
        let model = ChromeModel::fixture();
        let request = WorkerCall::to(
            "query",
            vec![json!({ "url": "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/popup/*" })],
        );

        let result = ChromeTabs::from(&model)
            .dispatch(&request, &BridgeAuthorization::fixture())
            .expect("query answers");

        assert_eq!(ids(&result), [] as [i64; 0]);
    }

    #[test]
    fn getting_a_tab_the_model_does_not_hold_fails_the_way_chrome_fails() {
        let model = ChromeModel::fixture();
        let request = WorkerCall::to("get", vec![json!(1)]);

        let error = ChromeTabs::from(&model)
            .dispatch(&request, &BridgeAuthorization::fixture())
            .expect_err("no such tab");

        assert_eq!(error.message, "No tab with id: 1.");
    }

    #[test]
    fn the_current_window_sentinel_is_not_a_window_id() {
        let model = ChromeModel::fixture();
        let request = WorkerCall::to("query", vec![json!({ "active": true, "windowId": -2 })]);

        let result = ChromeTabs::from(&model)
            .dispatch(&request, &BridgeAuthorization::fixture())
            .expect("query answers");

        assert_eq!(ids(&result), [12]);
    }

    #[test]
    fn an_id_too_large_for_a_tab_is_refused_rather_than_truncated() {
        let model = ChromeModel::fixture();
        let request = WorkerCall::to("get", vec![json!(4294967306i64)]);

        let error = ChromeTabs::from(&model)
            .dispatch(&request, &BridgeAuthorization::fixture())
            .expect_err("out of range");

        assert_eq!(error.code, "invalid_argument");
    }

    #[test]
    fn an_extension_without_the_tabs_permission_is_told_no_url() {
        let model = ChromeModel::fixture();
        let request = WorkerCall::to("get", vec![json!(12)]);
        let authorization = BridgeAuthorization {
            permissions: HashSet::new(),
            ..BridgeAuthorization::fixture()
        };

        let result = ChromeTabs::from(&model)
            .dispatch(&request, &authorization)
            .expect("get answers");

        assert_eq!(result["id"], json!(12));
        assert!(result.get("url").is_none());
    }
}
