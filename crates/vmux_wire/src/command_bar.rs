pub use crate::history::{
    HISTORY_SUGGESTIONS_RESPONSE_EVENT, HistoryEntry, HistorySuggestionsRequest,
    HistorySuggestionsResponse,
};

pub const COMMAND_BAR_OPEN_EVENT: &str = "command-bar-open";

/// Search provider used when command-bar input is not a URL or local path.
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
#[serde(rename_all = "lowercase")]
pub enum SearchEngine {
    #[default]
    Google,
    Bing,
    DuckDuckGo,
    Brave,
    Kagi,
}

impl SearchEngine {
    pub const ALL: [Self; 5] = [
        Self::Google,
        Self::Bing,
        Self::DuckDuckGo,
        Self::Brave,
        Self::Kagi,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Bing => "Bing",
            Self::DuckDuckGo => "DuckDuckGo",
            Self::Brave => "Brave Search",
            Self::Kagi => "Kagi",
        }
    }

    pub fn from_url(url: &str) -> Option<Self> {
        let parsed = url::Url::parse(url).ok()?;
        let host = parsed.host_str()?.trim_start_matches("www.");
        match host {
            "google.com" => Some(Self::Google),
            "bing.com" => Some(Self::Bing),
            "duckduckgo.com" => Some(Self::DuckDuckGo),
            "search.brave.com" => Some(Self::Brave),
            "kagi.com" => Some(Self::Kagi),
            _ => None,
        }
    }

    /// Build a search result URL for `query`.
    pub fn search_url(self, query: &str) -> String {
        let query: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
        match self {
            Self::Google => format!("https://www.google.com/search?q={query}"),
            Self::Bing => format!("https://www.bing.com/search?q={query}"),
            Self::DuckDuckGo => format!("https://duckduckgo.com/?q={query}"),
            Self::Brave => format!("https://search.brave.com/search?q={query}"),
            Self::Kagi => format!("https://kagi.com/search?q={query}"),
        }
    }
}

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
    #[serde(default)]
    pub work_dirs: Vec<CommandBarWorkDir>,
    #[serde(default)]
    pub recent_files: Vec<CommandBarRecentFile>,
    #[serde(default)]
    pub search_engines: Vec<SearchEngine>,
    #[serde(default)]
    pub prompt_context: CommandBarPromptContext,
    pub target: Option<crate::open_target::OpenTarget>,
    #[serde(default)]
    pub space_switch: bool,
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
pub struct CommandBarPromptContext {
    pub cwd: String,
    pub workspace_name: String,
    pub is_git_repo: bool,
    pub is_worktree: bool,
    pub branch: String,
    pub base_ref: String,
    pub uncommitted: u32,
    pub ahead: u32,
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
    pub icon: crate::icon::PageIcon,
    pub shortcut: String,
    /// Whether typing a prompt and pressing Enter can send it here.
    ///
    /// Set by whoever contributed the page. The command bar offers these as prompt targets
    /// without knowing what they are.
    #[serde(default)]
    pub prompt_target: bool,
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
    /// Human-readable location of this open page, `space / pane N / stack M`,
    /// shown instead of a generic "Stack" badge.
    #[serde(default)]
    pub location: String,
}

/// A file or directory inside a current work dir (the cwd of an open terminal/agent
/// pane), surfaced in the command bar's "current work" section so files can be opened
/// via `file://` fast. `is_dir` selects the icon and open behavior.
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
pub struct CommandBarWorkDir {
    pub path: String,
    pub is_dir: bool,
}

/// A recently-opened `file://` entry (from browser history), surfaced in the
/// command bar's "current work" section. `url` is the `file://` URL to reopen.
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
pub struct CommandBarRecentFile {
    pub url: String,
    pub title: String,
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
    /// Which prompt target the user picked, when they picked one explicitly.
    pub target_url: Option<String>,
    pub attachments: Vec<crate::prompt_media::ChatSubmitAttachment>,
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
    pub shell_left: i32,
    pub shell_top: i32,
    pub shell_width: u32,
    pub shell_height: u32,
}

pub fn command_bar_open_should_reset_input(current_open_id: u64, incoming_open_id: u64) -> bool {
    incoming_open_id == 0 || current_open_id != incoming_open_id
}

pub fn command_bar_open_should_ack(open_id: u64) -> bool {
    open_id != 0
}

/// Whether the palette should (re)focus and select-all its input. Only on a fresh
/// (re)open — i.e. when `open_id` changed. Live data refreshes (e.g. the start
/// page's current-work snapshot) reuse the same `open_id` and MUST NOT re-select,
/// or they clobber the user's in-progress typing.
pub fn command_bar_should_refocus(last_focus_open_id: u64, incoming_open_id: u64) -> bool {
    last_focus_open_id != incoming_open_id
}

pub fn should_open_typed_query_on_enter(
    open_target: Option<crate::open_target::OpenTarget>,
    nav_mode: bool,
    query: &str,
) -> bool {
    matches!(open_target, Some(crate::open_target::OpenTarget::InPlace))
        && !nav_mode
        && !query.trim().is_empty()
        && !query.trim_start().starts_with('>')
        && looks_like_url(query.trim())
}

pub fn is_start_prompt_query(query: &str) -> bool {
    let query = query.trim();
    !query.is_empty()
        && !query.starts_with('>')
        && !looks_like_url(query)
        && !looks_like_explicit_path(query)
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

pub fn is_data_uri(s: &str) -> bool {
    s.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("data:"))
}

pub fn looks_like_url(s: &str) -> bool {
    let s = s.trim();
    if is_data_uri(s) {
        return true;
    }
    if s.chars().any(char::is_whitespace)
        || s.starts_with('/')
        || s.starts_with("~/")
        || s.starts_with("./")
        || s.starts_with("../")
    {
        return false;
    }
    if s.contains("://") {
        return true;
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
#[path = "command_bar.test.rs"]
mod tests;
