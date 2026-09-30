use super::{CommandBarPick, CommandBarPicker, SearchEngine};
use crate::PageIcon;
use crate::chat::ResumableSessionEntry;
use crate::mcp::McpServerEntry;

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

#[vmux_api::contract(Copy, Default, Eq)]
pub struct CommandPaletteMenuMoveEffect {
    pub revision: u64,
    pub next: bool,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub struct CommandPaletteMenuChooseEffect {
    pub revision: u64,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub struct CommandPaletteMenuDismissEffect {
    pub revision: u64,
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
    },
    Navigate {
        url: String,
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

#[vmux_api::contract(Default, Eq)]
pub struct CommandPaletteProjection {
    pub query: String,
    pub rows: Vec<CommandBarResultItem>,
    pub selected: u32,
    pub navigating: bool,
    pub input_revision: u64,
    pub close_revision: u64,
    pub mcp_open: bool,
    pub mcp_entries: Vec<McpServerEntry>,
    pub menu_move: Option<CommandPaletteMenuMoveEffect>,
    pub menu_choose: Option<CommandPaletteMenuChooseEffect>,
    pub menu_dismiss: Option<CommandPaletteMenuDismissEffect>,
    pub prompt_targets: Vec<CommandBarResultItem>,
    pub default_target: Option<CommandBarResultItem>,
    pub ghost: String,
    pub start_prompt_mode: bool,
    pub mode: PaletteMode,
}
