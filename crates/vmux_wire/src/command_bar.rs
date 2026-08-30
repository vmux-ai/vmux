pub use crate::history::{
    HISTORY_SUGGESTIONS_RESPONSE_EVENT, HistoryEntry, HistorySuggestionsRequest,
    HistorySuggestionsResponse,
};

pub const COMMAND_BAR_OPEN_EVENT: &str = "command-bar-open";

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(transparent)]
pub struct OpenId(pub u64);

impl OpenId {
    pub const NONE: Self = Self(0);

    pub const fn is_open(self) -> bool {
        self.0 != Self::NONE.0
    }

    pub const fn should_reset_input(self, current: Self) -> bool {
        !self.is_open() || current.0 != self.0
    }

    pub const fn should_refocus(self, last_focused: Self) -> bool {
        last_focused.0 != self.0
    }
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
    pub open_id: OpenId,
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
    pub projects: Vec<String>,
    #[serde(default)]
    pub search_engines: Vec<SearchEngine>,
    #[serde(default)]
    pub prompt_context: CommandBarPromptContext,
    #[serde(default)]
    pub agent_models: Vec<AgentModels>,
    pub target: Option<crate::open_target::OpenTarget>,
    #[serde(default)]
    pub picker: Option<CommandBarPicker>,
    #[serde(default)]
    pub picks: Vec<CommandBarPickRow>,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum CommandBarPicker {
    Space,
    GotoLine,
    Indent,
    LineEnding,
    Encoding,
    EncodingReopen,
    EncodingSave,
}

impl CommandBarPicker {
    pub const fn is_space(self) -> bool {
        matches!(self, Self::Space)
    }

    pub const fn takes_typed_value(self) -> bool {
        matches!(self, Self::GotoLine)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Space => "",
            Self::GotoLine => "editor-status-goto-title",
            Self::Indent => "editor-status-indent-title",
            Self::LineEnding => "editor-status-eol-title",
            Self::Encoding => "editor-status-encoding-title",
            Self::EncodingReopen => "editor-status-encoding-reopen",
            Self::EncodingSave => "editor-status-encoding-save",
        }
    }

    pub const fn placeholder(self) -> &'static str {
        match self {
            Self::Space => "command-switch-space",
            Self::GotoLine => "editor-status-goto-placeholder",
            Self::Indent
            | Self::LineEnding
            | Self::Encoding
            | Self::EncodingReopen
            | Self::EncodingSave => "editor-status-pick-placeholder",
        }
    }
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
pub enum CommandBarPick {
    Picker(CommandBarPicker),
    GotoLine { line: u32 },
    Indent { spaces: bool, width: u16 },
    LineEnding { crlf: bool },
    Encoding { label: String, save: bool },
}

impl CommandBarPick {
    pub fn labelled(self, label: impl Into<String>) -> CommandBarPickRow {
        CommandBarPickRow {
            label: label.into(),
            pick: self,
        }
    }

    pub fn goto_line(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        let digits = match trimmed.split_once(':') {
            Some((line, _)) => line.trim(),
            None => trimmed,
        };
        let line = digits.parse::<u32>().ok()?;
        Some(Self::GotoLine {
            line: line.saturating_sub(1),
        })
    }
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
pub struct CommandBarPickRow {
    pub label: String,
    pub pick: CommandBarPick,
}

impl CommandBarPickRow {
    pub fn matches(&self, query: &str) -> bool {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        self.label.to_lowercase().contains(&needle)
    }
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
    pub projects: Vec<crate::space::ProjectRow>,
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
    #[serde(default)]
    pub location: String,
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
pub struct CommandBarWorkDir {
    pub path: String,
    pub is_dir: bool,
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
pub enum CommandBarActionEvent {
    Prompt {
        text: String,
        target_url: Option<String>,
        attachments: Vec<crate::prompt_media::ChatSubmitAttachment>,
    },
    Open {
        value: String,
        open: Option<crate::open_target::OpenTarget>,
    },
    Terminal {
        value: String,
    },
    Command {
        id: String,
        open: Option<crate::open_target::OpenTarget>,
    },
    Space {
        id: String,
    },
    SwitchTab {
        pane: u64,
        index: usize,
    },
    Ex {
        line: String,
    },
    Pick {
        pick: CommandBarPick,
    },
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExCommandName {
    pub name: &'static str,
    pub hint: &'static str,
}

impl ExCommandName {
    pub const ALL: [Self; 8] = [
        Self {
            name: "w",
            hint: "ex-write",
        },
        Self {
            name: "wq",
            hint: "ex-write-quit",
        },
        Self {
            name: "q",
            hint: "ex-quit",
        },
        Self {
            name: "q!",
            hint: "ex-quit-force",
        },
        Self {
            name: "noh",
            hint: "ex-nohighlight",
        },
        Self {
            name: "d",
            hint: "ex-delete",
        },
        Self {
            name: "y",
            hint: "ex-yank",
        },
        Self {
            name: "s/",
            hint: "ex-substitute",
        },
    ];

    pub fn matching(typed: &str) -> Vec<Self> {
        let mut found = Vec::new();
        for entry in Self::ALL {
            if entry.name.starts_with(typed) {
                found.push(entry);
            }
        }
        found
    }
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
pub struct StartSelectWorkspace {
    pub current_dir: String,
}

pub const START_PROJECT_BRANCHES_EVENT: &str = "start_project_branches";

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
pub struct StartBranchesRequest {
    pub project: String,
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
pub struct StartProjectBranches {
    pub project: String,
    pub branches: Vec<crate::space::ProjectBranch>,
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
pub struct StartGoToBranch {
    pub project: String,
    pub branch: String,
    #[serde(default)]
    pub checkout: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentModels {
    pub agent_key: String,
    pub url: String,
    pub selected: String,
    pub models: Vec<crate::room::ModelOptionEntry>,
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
pub struct StartSelectModel {
    pub agent_key: String,
    pub model_id: String,
}

impl CommandBarActionEvent {
    pub fn open(value: &str, open: Option<crate::open_target::OpenTarget>) -> Self {
        Self::Open {
            value: value.to_string(),
            open,
        }
    }

    pub fn prompt(
        text: &str,
        target_url: &str,
        attachments: &[crate::prompt_media::ChatAttachment],
    ) -> Self {
        let mut submitted = Vec::with_capacity(attachments.len());
        for attachment in attachments {
            submitted.push(crate::prompt_media::ChatSubmitAttachment {
                path: attachment.path.clone(),
                name: attachment.name.clone(),
                mime_type: attachment.mime_type.clone(),
                size: attachment.size,
            });
        }
        Self::Prompt {
            text: text.to_string(),
            target_url: (!target_url.is_empty()).then(|| target_url.to_string()),
            attachments: submitted,
        }
    }
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

pub const COMMAND_BAR_KEY_EVENT: &str = "command-bar-key";

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum CommandBarKey {
    Next,
    Previous,
    Complete,
    Dismiss,
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
pub struct CommandBarRenderedEvent {
    pub open_id: OpenId,
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

#[derive(Clone, Copy, Debug)]
pub struct CommandBarQuery<'a>(pub &'a str);

impl CommandBarQuery<'_> {
    pub fn opens_typed_url_on_enter(
        &self,
        open_target: Option<crate::open_target::OpenTarget>,
        nav_mode: bool,
    ) -> bool {
        let query = self.0.trim();
        matches!(open_target, Some(crate::open_target::OpenTarget::InPlace))
            && !nav_mode
            && !query.is_empty()
            && !self.0.trim_start().starts_with('>')
            && looks_like_url(query)
    }

    pub fn is_start_prompt(&self) -> bool {
        let query = self.0.trim();
        !query.is_empty()
            && !query.starts_with('>')
            && !looks_like_url(query)
            && !looks_like_explicit_path(query)
    }
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
    pub project: String,
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
    pub truncated: bool,
    pub total: u32,
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
        assert!(looks_like_url("file:///Users/me/main.rs"));
    }

    #[test]
    fn looks_like_url_domain_like() {
        assert!(looks_like_url("google.com"));
        assert!(looks_like_url("google.com/maps"));
        assert!(looks_like_url("example.co.uk/page"));
    }

    #[test]
    fn looks_like_url_data_scheme() {
        assert!(looks_like_url("data:text/html,<h1>hi</h1>"));
        assert!(looks_like_url(
            "data:text/html,<style>body{background:white}</style>"
        ));
        assert!(looks_like_url("DATA:text/html,<h1>hi</h1>"));
        assert!(looks_like_url("Data:text/html,<h1>hi</h1>"));
        assert!(!looks_like_path("data:text/html,<h1>hi</h1>"));
        assert!(!looks_like_path("DATA:text/html,<h1>hi</h1>"));
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
    fn multiline_prompt_with_embedded_url_is_not_a_url() {
        let prompt = "Continue DSK-627 in:\n\nWorktree:\n  /tmp/dashboard\n\nPR:\n  https://github.com/mistralai/dashboard/pull/39364";

        assert!(!looks_like_url(prompt));
        assert!(CommandBarQuery(prompt).is_start_prompt());
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
            open_id: OpenId(7),
            ..Default::default()
        };

        assert_eq!(event.open_id, OpenId(7));
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
        assert!(!OpenId(7).should_reset_input(OpenId(7)));
        assert!(OpenId(8).should_reset_input(OpenId(7)));
        assert!(OpenId(8).should_reset_input(OpenId::NONE));
        assert!(OpenId::NONE.should_reset_input(OpenId::NONE));
    }

    #[test]
    fn command_bar_refocus_only_on_open_id_change() {
        assert!(OpenId::NONE.should_refocus(OpenId(u64::MAX)));
        assert!(OpenId(8).should_refocus(OpenId(7)));
        assert!(!OpenId::NONE.should_refocus(OpenId::NONE));
        assert!(!OpenId(7).should_refocus(OpenId(7)));
    }

    #[test]
    fn only_a_real_open_is_acked_and_revealed() {
        assert!(OpenId(7).is_open());
        assert!(!OpenId::NONE.is_open());
    }

    #[test]
    fn in_place_enter_opens_typed_query_without_nav_selection() {
        assert!(
            CommandBarQuery("https://example.com")
                .opens_typed_url_on_enter(Some(crate::open_target::OpenTarget::InPlace), false)
        );
    }

    #[test]
    fn in_place_enter_keeps_explicit_nav_selection() {
        assert!(
            !CommandBarQuery("https://example.com")
                .opens_typed_url_on_enter(Some(crate::open_target::OpenTarget::InPlace), true)
        );
    }

    #[test]
    fn command_query_enter_keeps_command_selection() {
        assert!(
            !CommandBarQuery("> close")
                .opens_typed_url_on_enter(Some(crate::open_target::OpenTarget::InPlace), false)
        );
    }

    #[test]
    fn in_place_enter_keeps_highlighted_suggestion_for_plain_text_query() {
        assert!(
            !CommandBarQuery("terminal")
                .opens_typed_url_on_enter(Some(crate::open_target::OpenTarget::InPlace), false)
        );
    }

    #[test]
    fn in_place_enter_opens_typed_domain_query() {
        assert!(
            CommandBarQuery("google.com")
                .opens_typed_url_on_enter(Some(crate::open_target::OpenTarget::InPlace), false)
        );
    }

    #[test]
    fn start_plain_text_is_prompt_query() {
        assert!(CommandBarQuery("fix the failing test").is_start_prompt());
    }

    #[test]
    fn search_engines_build_encoded_urls() {
        assert_eq!(
            SearchEngine::Google.search_url("hello world"),
            "https://www.google.com/search?q=hello+world"
        );
        assert_eq!(
            SearchEngine::Bing.search_url("hello world"),
            "https://www.bing.com/search?q=hello+world"
        );
        assert_eq!(
            SearchEngine::DuckDuckGo.search_url("hello world"),
            "https://duckduckgo.com/?q=hello+world"
        );
        assert_eq!(
            SearchEngine::Brave.search_url("hello world"),
            "https://search.brave.com/search?q=hello+world"
        );
        assert_eq!(
            SearchEngine::Kagi.search_url("hello world"),
            "https://kagi.com/search?q=hello+world"
        );
    }

    #[test]
    fn start_agent_name_is_still_prompt_query() {
        assert!(CommandBarQuery("codex").is_start_prompt());
    }

    #[test]
    fn start_explicit_navigation_inputs_are_not_prompts() {
        for query in [
            "https://example.com",
            "example.com",
            "vmux://settings/",
            "/tmp/file",
            "~/project",
            "./src",
            "../repo",
            "> close tab",
        ] {
            assert!(!CommandBarQuery(query).is_start_prompt(), "{query}");
        }
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
                icon: crate::icon::PageIcon::Builtin(crate::icon::BuiltinIcon::Settings),
                shortcut: String::new(),
                prompt_target: false,
            }],
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
        let recovered =
            rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(recovered.pages.len(), 1);
        assert_eq!(recovered.pages[0].title, "Settings");
    }

    #[test]
    fn a_typed_line_number_is_one_based_and_anything_else_is_refused() {
        assert_eq!(
            CommandBarPick::goto_line("42"),
            Some(CommandBarPick::GotoLine { line: 41 })
        );
        assert_eq!(
            CommandBarPick::goto_line("  7  "),
            Some(CommandBarPick::GotoLine { line: 6 })
        );
        assert_eq!(
            CommandBarPick::goto_line("12:5"),
            Some(CommandBarPick::GotoLine { line: 11 }),
            "a pasted line:column lands on the line"
        );
        assert_eq!(
            CommandBarPick::goto_line("0"),
            Some(CommandBarPick::GotoLine { line: 0 })
        );
        for refused in ["", "abc", "-3", "3.5"] {
            assert_eq!(CommandBarPick::goto_line(refused), None, "{refused}");
        }
    }

    #[test]
    fn an_asserted_picker_survives_the_wire() {
        let event = CommandBarOpenEvent {
            picker: Some(CommandBarPicker::EncodingReopen),
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
        let recovered =
            rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");

        assert_eq!(recovered.picker, Some(CommandBarPicker::EncodingReopen));
        assert_eq!(CommandBarOpenEvent::default().picker, None);
    }

    #[test]
    fn command_bar_open_event_carries_work_and_recent() {
        let event = CommandBarOpenEvent {
            work_dirs: vec![CommandBarWorkDir {
                path: "/work/proj/main.rs".into(),
                is_dir: false,
            }],
            recent_files: vec![CommandBarRecentFile {
                url: "file:///work/proj/main.rs".into(),
                title: "main.rs".into(),
            }],
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
        let recovered =
            rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(recovered.work_dirs.len(), 1);
        assert_eq!(recovered.work_dirs[0].path, "/work/proj/main.rs");
        assert!(!recovered.work_dirs[0].is_dir);
        assert_eq!(recovered.recent_files[0].title, "main.rs");
    }
}
