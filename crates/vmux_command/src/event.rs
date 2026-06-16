pub use vmux_history::event::{
    HISTORY_SUGGESTIONS_RESPONSE_EVENT, HistoryEntry, HistorySuggestionsRequest,
    HistorySuggestionsResponse,
};

pub const COMMAND_BAR_OPEN_EVENT: &str = "command-bar-open";

#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarOpenEvent {
    #[serde(default)]
    pub open_id: u64,
    #[serde(default)]
    pub native_windowed: bool,
    pub url: String,
    #[serde(default)]
    pub space_name: String,
    #[serde(default)]
    pub spaces: Vec<CommandBarSpace>,
    pub tabs: Vec<CommandBarTab>,
    pub commands: Vec<CommandBarCommandEntry>,
    #[serde(default)]
    pub pages: Vec<CommandBarPage>,
    pub target: Option<crate::open_target::OpenTarget>,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarPage {
    pub host: String,
    pub url: String,
    pub title: String,
    pub keywords: Vec<String>,
    pub icon: String,
    pub favicon: bool,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarSpace {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub is_active: bool,
    pub tab_count: u32,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarTab {
    pub title: String,
    pub url: String,
    pub pane_id: u64,
    pub tab_index: u32,
    pub is_active: bool,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarCommandEntry {
    pub id: String,
    pub name: String,
    pub shortcut: String,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarActionEvent {
    pub action: String,
    pub value: String,
    pub target: Option<crate::open_target::OpenTarget>,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarReadyEvent;

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarRenderedEvent {
    pub open_id: u64,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarSizeEvent {
    pub width: u32,
    pub height: u32,
}

pub fn command_bar_open_should_reset_input(current_open_id: u64, incoming_open_id: u64) -> bool {
    incoming_open_id == 0 || current_open_id != incoming_open_id
}

pub fn command_bar_open_should_ack(open_id: u64) -> bool {
    open_id != 0
}

pub const PATH_COMPLETE_REQUEST: &str = "path-complete-request";
pub const PATH_COMPLETE_RESPONSE: &str = "path-complete-response";

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PathCompleteRequest {
    pub query: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PathEntry {
    pub name: String,
    pub is_dir: bool,
    pub full_path: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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
            action: "open".to_string(),
            value: "google.com".to_string(),
            target: None,
        };
        assert_eq!(evt.action, "open");
        assert_eq!(evt.value, "google.com");
        assert_eq!(evt.target, None);
    }

    #[test]
    fn command_bar_open_event_carries_space_name() {
        let event = CommandBarOpenEvent {
            space_name: "Work".to_string(),
            ..Default::default()
        };

        assert_eq!(event.space_name, "Work");
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
    fn command_bar_open_event_defaults_to_osr_layout() {
        let event = CommandBarOpenEvent::default();

        assert!(!event.native_windowed);
    }

    #[test]
    fn command_bar_open_event_carries_native_windowed() {
        let event = CommandBarOpenEvent {
            native_windowed: true,
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
        let recovered =
            rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");

        assert!(recovered.native_windowed);
    }

    #[test]
    fn command_bar_duplicate_open_id_does_not_reset_input() {
        assert!(!command_bar_open_should_reset_input(7, 7));
        assert!(command_bar_open_should_reset_input(7, 8));
        assert!(command_bar_open_should_reset_input(0, 8));
        assert!(command_bar_open_should_reset_input(0, 0));
    }

    #[test]
    fn command_bar_retried_open_payload_still_gets_ack() {
        assert!(command_bar_open_should_ack(7));
        assert!(!command_bar_open_should_ack(0));
    }

    #[test]
    fn command_bar_open_event_carries_target_enum() {
        let event = CommandBarOpenEvent {
            target: Some(crate::open_target::OpenTarget::InNewStack),
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
        let recovered =
            rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(
            recovered.target,
            Some(crate::open_target::OpenTarget::InNewStack)
        );
    }

    #[test]
    fn command_bar_open_event_target_none_round_trips() {
        let event = CommandBarOpenEvent::default();
        assert_eq!(event.target, None);
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
        let recovered =
            rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(recovered.target, None);
    }

    #[test]
    fn command_bar_open_event_carries_spaces() {
        let event = CommandBarOpenEvent {
            spaces: vec![CommandBarSpace {
                id: "work".to_string(),
                name: "Work".to_string(),
                profile: "Personal".to_string(),
                is_active: true,
                tab_count: 2,
            }],
            ..Default::default()
        };

        assert_eq!(event.spaces[0].id, "work");
        assert!(event.spaces[0].is_active);
    }

    #[test]
    fn command_bar_open_event_carries_pages() {
        let event = CommandBarOpenEvent {
            pages: vec![CommandBarPage {
                host: "settings".to_string(),
                url: "vmux://settings/".to_string(),
                title: "Settings".to_string(),
                keywords: vec!["preferences".to_string()],
                icon: "settings".to_string(),
                favicon: false,
            }],
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
        let recovered =
            rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(recovered.pages.len(), 1);
        assert_eq!(recovered.pages[0].title, "Settings");
    }
}
