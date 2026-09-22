use super::{AgentModels, AgentModes, CommandBarPickRow, CommandBarPicker};

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
#[vmux_api::host_event(targets = ["command-bar", "start", "layout"])]
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
    #[serde(default)]
    pub agent_modes: Vec<AgentModes>,
    pub target: Option<crate::open_target::OpenTarget>,
    #[serde(default)]
    pub picker: Option<CommandBarPicker>,
    #[serde(default)]
    pub picks: Vec<CommandBarPickRow>,
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
    pub slash_commands: Vec<crate::chat::SlashCommandEntry>,
}

impl CommandBarPromptContext {
    pub fn unrooted() -> Self {
        Self {
            slash_commands: crate::chat::SlashCommands::for_start().commands,
            ..Self::default()
        }
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
