use super::{CommandBarPick, CommandBarPicker, SearchEngine};
use crate::PageIcon;
use crate::chat::ResumableSessionEntry;
use crate::conversation::ModelOptionEntry;
use crate::mcp::McpServerEntry;
use crate::protocol::AcpModeOption;
use crate::space::ProjectRow;

#[vmux_api::contract(Copy, Default, Eq)]
pub enum PaletteMode {
    #[default]
    Search,
    Command,
    Ex,
    Path,
    Url,
    Slash,
    Picking(CommandBarPicker),
}

#[vmux_api::contract(Copy, Eq)]
pub enum PaletteGlyph {
    Command,
    Path,
    Url,
    Search,
}

#[vmux_api::contract(Default, Eq)]
pub struct CommandPaletteAgent {
    pub url: String,
    pub title: String,
}

#[vmux_api::contract(Default)]
pub struct CommandPaletteComposer {
    pub loading: bool,
    pub agents: Vec<CommandPaletteAgent>,
    pub agent_title: String,
    pub agent_url: String,
    pub model_name: String,
    pub model_options: Vec<ModelOptionEntry>,
    pub model_agent_key: String,
    pub model_current_id: String,
    pub permission_modes: Vec<AcpModeOption>,
    pub permission_agent_key: String,
    pub permission_current_id: String,
    pub workspace_label: String,
    pub workspace_title: String,
    pub branch_label: String,
    pub branch_title: String,
    pub worktree_title: String,
    pub project: String,
    pub projects: Vec<ProjectRow>,
    pub cwd: String,
    pub is_git_repo: bool,
    pub is_worktree: bool,
    pub uncommitted: u32,
    pub ahead: u32,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub struct CommandPaletteMenus {
    pub agent: bool,
    pub model: bool,
    pub permission: bool,
    pub project: bool,
    pub branch: bool,
}

impl CommandPaletteMenus {
    pub const fn is_open(self) -> bool {
        self.agent || self.model || self.permission || self.project || self.branch
    }
}

#[vmux_api::contract(Eq)]
pub struct ResumeSection {
    pub agent: String,
    pub project: String,
    pub branch: String,
    pub count: usize,
}

#[vmux_api::contract(Eq)]
pub enum CommandBarResultItem {
    Pick {
        label: String,
        pick: CommandBarPick,
    },
    Terminal {
        path: String,
    },
    Editor {
        path: String,
    },
    Stack {
        title: String,
        url: String,
        icon: PageIcon,
        pane_id: u64,
        tab_index: usize,
        location: String,
    },
    Space {
        id: String,
        name: String,
        profile: String,
        is_active: bool,
        tab_count: usize,
    },
    Command {
        id: String,
        name: String,
        shortcut: String,
    },
    Ex {
        name: String,
        hint: String,
    },
    Page {
        url: String,
        title: String,
        icon: PageIcon,
        shortcut: String,
        prompt_target: bool,
        prompt_hint: bool,
    },
    Navigate {
        url: String,
        is_url: bool,
    },
    Search {
        engine: SearchEngine,
        query: String,
    },
    File {
        path: String,
        is_dir: bool,
        project: String,
        relative: String,
    },
    History {
        url: String,
        title: String,
        favicon_url: String,
        visit_count: u32,
        last_visited_at: i64,
    },
    WorkDir {
        path: String,
        is_dir: bool,
    },
    RecentFile {
        url: String,
        title: String,
    },
    Slash {
        name: String,
        hint: String,
    },
    Resume {
        entry: Box<ResumableSessionEntry>,
        section: Option<ResumeSection>,
    },
    ResumePending {
        row: usize,
    },
    PartialIndex,
    MoreMatches {
        shown: usize,
        total: usize,
    },
}

#[vmux_api::contract(Default)]
pub struct CommandPaletteProjection {
    pub query: String,
    pub rows: Vec<CommandBarResultItem>,
    pub selected: u32,
    pub navigating: bool,
    pub history_recalling: bool,
    pub row_text: Option<String>,
    pub placeholder: String,
    pub glyph: Option<PaletteGlyph>,
    pub space_switch: bool,
    pub space_count: u32,
    pub space_name: String,
    pub accent_agent: Option<String>,
    pub composer: CommandPaletteComposer,
    pub menus: CommandPaletteMenus,
    pub menu_cursor: u32,
    pub input_revision: u64,
    pub close_revision: u64,
    pub mcp_open: bool,
    pub mcp_entries: Vec<McpServerEntry>,
    pub prompt_targets: Vec<CommandBarResultItem>,
    pub default_target: Option<CommandBarResultItem>,
    pub ghost: String,
    pub start_prompt_mode: bool,
    pub mode: PaletteMode,
}
