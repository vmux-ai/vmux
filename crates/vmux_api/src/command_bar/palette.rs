use super::{CommandBarPick, CommandBarPicker, CommandBarQuery, SearchEngine};
use crate::PageIcon;
use crate::chat::{ResumableSessionEntry, SlashCommandEntry};

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

impl PaletteMode {
    pub fn infer(query: &str, asserted: Option<CommandBarPicker>) -> Self {
        Self::read(query, asserted, &[])
    }

    pub fn read(
        query: &str,
        asserted: Option<CommandBarPicker>,
        slash_commands: &[SlashCommandEntry],
    ) -> Self {
        if let Some(picker) = asserted {
            return Self::Picking(picker);
        }
        if query.starts_with(':') {
            return Self::Ex;
        }
        let trimmed = query.trim();
        if trimmed.starts_with('>') {
            return Self::Command;
        }
        if Self::names_a_command(query, slash_commands) {
            return Self::Slash;
        }
        if trimmed.starts_with('/') || trimmed.starts_with('~') {
            return Self::Path;
        }
        if trimmed.contains("://") || (trimmed.contains('.') && !trimmed.contains(' ')) {
            return Self::Url;
        }
        Self::Search
    }

    fn names_a_command(query: &str, slash_commands: &[SlashCommandEntry]) -> bool {
        if query.trim() == "/" {
            return !slash_commands.is_empty();
        }
        let held = CommandBarQuery(query);
        let Some((name, _)) = held.slash_token() else {
            return false;
        };
        let lowered = name.to_lowercase();
        slash_commands
            .iter()
            .any(|command| command.name().starts_with(&lowered))
    }

    pub const fn is_ex(self) -> bool {
        matches!(self, Self::Ex)
    }

    pub const fn is_space(self) -> bool {
        matches!(self, Self::Picking(CommandBarPicker::Space))
    }

    pub const fn picking(self) -> Option<CommandBarPicker> {
        match self {
            Self::Picking(picker) => Some(picker),
            _ => None,
        }
    }

    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Ex => ":",
            Self::Command => ">",
            Self::Path | Self::Slash => "/",
            Self::Search | Self::Url | Self::Picking(_) => "",
        }
    }

    pub fn opens_at_end(self, query: &str) -> bool {
        let prefix = self.prefix();
        !prefix.is_empty() && query == prefix
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ex => "palette-mode-ex",
            Self::Command => "palette-mode-command",
            Self::Path => "palette-mode-path",
            Self::Slash => "palette-mode-slash",
            Self::Picking(picker) => picker.label(),
            Self::Search | Self::Url => "",
        }
    }
}

#[vmux_api::contract(Eq)]
pub struct ResumeSection {
    pub agent: String,
    pub project: String,
    pub branch: String,
    pub count: usize,
}

impl From<&ResumableSessionEntry> for ResumeSection {
    fn from(entry: &ResumableSessionEntry) -> Self {
        let agent = match entry.agent_name.is_empty() {
            true => entry.kind.clone(),
            false => entry.agent_name.clone(),
        };
        let project = match entry.project.is_empty() {
            true => entry.subtitle.clone(),
            false => entry.project.clone(),
        };
        Self {
            agent,
            project,
            branch: entry.branch.clone(),
            count: 0,
        }
    }
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
    pub rows: Vec<CommandBarResultItem>,
    pub prompt_targets: Vec<CommandBarResultItem>,
    pub default_target: Option<CommandBarResultItem>,
    pub ghost: String,
    pub start_prompt_mode: bool,
    pub mode: PaletteMode,
}
