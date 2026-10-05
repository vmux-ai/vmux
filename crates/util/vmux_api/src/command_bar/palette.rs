use super::CommandBarPicker;
use crate::PageIcon;
use crate::protocol::AcpModeOption;
use crate::room::ModelOptionEntry;
use crate::space::ProjectRow;

#[vmux_api::contract(Default, Eq)]
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
    pub permission_name: String,
    pub permission_title: String,
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

#[vmux_api::contract(Default, Eq)]
pub struct CommandBarSection {
    pub labels: Vec<String>,
    pub count: u32,
}

#[vmux_api::contract(Default, Eq)]
pub struct CommandBarResultItem {
    pub key: String,
    pub leading: String,
    pub title: String,
    pub subtitle: String,
    pub detail: String,
    pub trailing: String,
    pub badge: String,
    pub url: String,
    pub favicon_url: String,
    pub file_path: String,
    pub icon: PageIcon,
    pub active: bool,
    pub directory: bool,
    pub pending: bool,
    pub disabled: bool,
    pub section: Option<CommandBarSection>,
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
    pub mode_label: String,
    pub glyph: Option<PaletteGlyph>,
    pub numbered: bool,
    pub numbered_count: u32,
    pub context_label: String,
    pub accent_agent: Option<String>,
    pub composer: CommandPaletteComposer,
    pub menus: CommandPaletteMenus,
    pub menu_cursor: u32,
    pub input_revision: u64,
    pub close_revision: u64,
    pub prompt_targets: Vec<CommandBarResultItem>,
    pub default_target: Option<CommandBarResultItem>,
    pub ghost: String,
    pub start_prompt_mode: bool,
    pub mode: PaletteMode,
}
