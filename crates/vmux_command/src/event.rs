pub const COMMAND_BAR_OPEN_EVENT: &str = "command-bar-open";

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CommandBarOpenEvent {
    #[serde(default)]
    pub open_id: u64,
    pub url: String,
    #[serde(default)]
    pub session_name: String,
    #[serde(default)]
    pub sessions: Vec<CommandBarSession>,
    pub tabs: Vec<CommandBarTab>,
    pub commands: Vec<CommandBarCommandEntry>,
    pub new_tab: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommandBarSession {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub is_active: bool,
    pub tab_count: usize,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandBarTab {
    pub title: String,
    pub url: String,
    pub pane_id: u64,
    pub tab_index: usize,
    pub is_active: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandBarCommandEntry {
    pub id: String,
    pub name: String,
    pub shortcut: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandBarActionEvent {
    pub action: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CommandBarReadyEvent;

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CommandBarRenderedEvent {
    pub open_id: u64,
}

pub const PATH_COMPLETE_REQUEST: &str = "path-complete-request";
pub const PATH_COMPLETE_RESPONSE: &str = "path-complete-response";

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PathCompleteRequest {
    pub query: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PathEntry {
    pub name: String,
    pub is_dir: bool,
    pub full_path: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PathCompleteResponse {
    pub completions: Vec<PathEntry>,
}

pub fn looks_like_url(s: &str) -> bool {
    if s.contains("://") {
        return true;
    }
    if s.contains(' ')
        || s.starts_with('/')
        || s.starts_with("~/")
        || s.starts_with("./")
        || s.starts_with("../")
    {
        return false;
    }
    let before_slash = s.split('/').next().unwrap_or(s);
    before_slash.contains('.')
}

pub fn looks_like_path(s: &str) -> bool {
    if looks_like_url(s) {
        return false;
    }
    s.starts_with('/')
        || s.starts_with("~/")
        || s.starts_with("./")
        || s.starts_with("../")
        || (s.contains('/') && !s.contains(' '))
}

pub fn looks_like_explicit_path(s: &str) -> bool {
    s.starts_with('/') || s.starts_with('~') || s.starts_with("./") || s.starts_with("../")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_path_absolute() {
        assert!(looks_like_path("/usr/bin"));
        assert!(looks_like_path("/"));
    }

    #[test]
    fn looks_like_path_home() {
        assert!(looks_like_path("~/projects"));
        assert!(looks_like_path("~/"));
    }

    #[test]
    fn looks_like_path_relative() {
        assert!(looks_like_path("./src"));
        assert!(looks_like_path("../parent"));
    }

    #[test]
    fn looks_like_path_with_slash() {
        assert!(looks_like_path("src/main.rs"));
        assert!(looks_like_path("foo/bar"));
    }

    #[test]
    fn looks_like_path_rejects_urls() {
        assert!(!looks_like_path("http://example.com/path"));
        assert!(!looks_like_path("https://example.com/path"));
        assert!(!looks_like_path("google.com/maps"));
        assert!(!looks_like_path("example.com"));
    }

    #[test]
    fn looks_like_url_protocols() {
        assert!(looks_like_url("http://example.com"));
        assert!(looks_like_url("https://example.com/path"));
    }

    #[test]
    fn looks_like_url_domain_like() {
        assert!(looks_like_url("google.com"));
        assert!(looks_like_url("google.com/maps"));
        assert!(looks_like_url("example.co.uk/page"));
    }

    #[test]
    fn looks_like_url_rejects_file_paths() {
        assert!(!looks_like_url("src/main.rs"));
        assert!(!looks_like_url("/usr/bin"));
        assert!(!looks_like_url("foo/bar"));
    }

    #[test]
    fn looks_like_url_rejects_spaces() {
        assert!(!looks_like_url("search query"));
        assert!(!looks_like_url("hello world.txt"));
    }

    #[test]
    fn looks_like_path_rejects_bare_words() {
        assert!(!looks_like_path("mistral"));
        assert!(!looks_like_path("hello world"));
        assert!(!looks_like_path("google.com"));
    }

    #[test]
    fn looks_like_path_rejects_spaces_with_slash() {
        assert!(!looks_like_path("some query / thing"));
    }

    #[test]
    fn explicit_path_only_prefixed() {
        assert!(looks_like_explicit_path("/usr"));
        assert!(looks_like_explicit_path("~/foo"));
        assert!(looks_like_explicit_path("./bar"));
        assert!(looks_like_explicit_path("../baz"));
    }

    #[test]
    fn explicit_path_rejects_bare_words() {
        assert!(!looks_like_explicit_path("mistral"));
        assert!(!looks_like_explicit_path("foo/bar"));
        assert!(!looks_like_explicit_path("google.com"));
        assert!(!looks_like_explicit_path("search query"));
    }

    #[test]
    fn explicit_path_rejects_urls() {
        assert!(!looks_like_explicit_path("http://example.com"));
        assert!(!looks_like_explicit_path("https://example.com"));
    }

    #[test]
    fn action_event_fields() {
        let evt = CommandBarActionEvent {
            action: "navigate".to_string(),
            value: "google.com".to_string(),
        };
        assert_eq!(evt.action, "navigate");
        assert_eq!(evt.value, "google.com");
    }

    #[test]
    fn command_bar_open_event_carries_session_name() {
        let event = CommandBarOpenEvent {
            session_name: "Work".to_string(),
            ..Default::default()
        };

        assert_eq!(event.session_name, "Work");
    }

    #[test]
    fn command_bar_open_event_carries_open_id() {
        let event = CommandBarOpenEvent {
            open_id: 7,
            ..Default::default()
        };

        assert_eq!(event.open_id, 7);
    }

    #[test]
    fn command_bar_open_event_carries_sessions() {
        let event = CommandBarOpenEvent {
            sessions: vec![CommandBarSession {
                id: "work".to_string(),
                name: "Work".to_string(),
                profile: "default".to_string(),
                is_active: true,
                tab_count: 2,
            }],
            ..Default::default()
        };

        assert_eq!(event.sessions[0].id, "work");
        assert!(event.sessions[0].is_active);
    }
}
