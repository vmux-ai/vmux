use vmux_api::command_bar::{
    CommandBarOpenEvent, CommandBarPicker, CommandPaletteAgent, CommandPaletteComposer,
    CommandPaletteProjection, HistoryEntry, PaletteGlyph, PaletteMode, PathEntry,
};
use vmux_api::open_target::OpenTarget;

use vmux_ui::i18n::translate;
#[cfg(test)]
use vmux_ui::list_nav::MenuDirection;

use crate::CommandPaletteSurface;

pub(super) use self::decision::{AgentSegment, PaletteDecision, PaletteState};
#[cfg(test)]
use self::decision::{RowText, TypedRow};
use self::decision::{SelectedAgentModels, SelectedAgentModes};
#[cfg(test)]
use self::file::ProjectPath;
pub(super) use self::file::{CompletionQuery, Completions, FileRows};
pub use self::results::ResumeRows;
use self::results::{
    CommandBarResultItem, PageRows, PickerRows, SearchRows, SearchRowsInput, SlashRows, SpaceRows,
    StartRows,
};

pub(super) use query::PaletteQuery;

mod decision;
mod file;
mod query;
mod results;

#[derive(Clone, Debug, Default)]
pub struct PaletteDraft {
    pub query: String,
    pub selected: usize,
    pub nav_mode: bool,
    pub target_url: String,
    pub completions: Vec<PathEntry>,
    pub completions_partial: bool,
    pub completions_total: usize,
    pub history: Vec<HistoryEntry>,
    pub sessions: Vec<vmux_api::chat::ResumableSessionEntry>,
    pub sessions_pending: bool,
}

#[cfg(test)]
impl PaletteDraft {
    pub fn typed(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Self::default()
        }
    }

    pub fn at(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn navigating(mut self) -> Self {
        self.nav_mode = true;
        self
    }

    pub fn targeting(mut self, target_url: impl Into<String>) -> Self {
        self.target_url = target_url.into();
        self
    }

    pub fn completing(mut self, completions: Vec<PathEntry>) -> Self {
        self.completions_total = completions.len();
        self.completions = completions;
        self
    }

    pub fn partially_completing(mut self, completions: Vec<PathEntry>) -> Self {
        self.completions_total = completions.len();
        self.completions = completions;
        self.completions_partial = true;
        self
    }

    pub fn out_of(mut self, total: usize) -> Self {
        self.completions_total = total;
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaletteRows {
    pub items: Vec<CommandBarResultItem>,
    pub prompt_targets: Vec<CommandBarResultItem>,
    pub default_target: Option<CommandBarResultItem>,
    pub ghost: String,
    pub start_prompt_mode: bool,
    pub mode: PaletteMode,
}

impl PaletteRows {
    pub fn from_projection(projection: &CommandPaletteProjection) -> Self {
        Self {
            items: projection.rows.clone(),
            prompt_targets: projection.prompt_targets.clone(),
            default_target: projection.default_target.clone(),
            ghost: projection.ghost.clone(),
            start_prompt_mode: projection.start_prompt_mode,
            mode: projection.mode,
        }
    }

    pub fn build(
        state: &CommandBarOpenEvent,
        draft: &PaletteDraft,
        surface: CommandPaletteSurface,
    ) -> Self {
        let query = draft.query.as_str();
        let is_start = surface.is_start();
        let slash_commands = state.prompt_context.slash_commands.as_slice();
        let mode = Self::mode(query, state.picker, slash_commands);
        let prompt_targets = if is_start {
            PageRows::prompt_targets(&state.pages, "")
        } else {
            Vec::new()
        };
        let default_target = prompt_targets
            .iter()
            .find(|item| PageRows::prompt_target_url(item) == Some(draft.target_url.as_str()))
            .cloned()
            .or_else(|| prompt_targets.first().cloned());
        let start_prompt_mode = is_start && PaletteQuery::new(query).is_start_prompt();

        let mut items = FileRows::under_projects(
            Self::listed(state, draft, surface, mode, start_prompt_mode),
            &state.projects,
        );
        if start_prompt_mode {
            StartRows::prepend_targets(&mut items, default_target.as_ref(), &prompt_targets, query);
        }
        for item in &mut items {
            let show_hint = start_prompt_mode
                && PageRows::prompt_target_url(item).is_some()
                && !PageRows::prompt_target_matches(item, query);
            if let CommandBarResultItem::Page { prompt_hint, .. } = item {
                *prompt_hint = show_hint;
            }
        }

        Self {
            items,
            prompt_targets,
            default_target,
            ghost: Self::ghost_of(query, &draft.completions),
            start_prompt_mode,
            mode,
        }
    }

    pub fn infer_mode(query: &str, asserted: Option<CommandBarPicker>) -> PaletteMode {
        Self::mode(query, asserted, &[])
    }

    pub fn opens_at_end(query: &str, asserted: Option<CommandBarPicker>) -> bool {
        let prefix = match Self::infer_mode(query, asserted) {
            PaletteMode::Ex => ":",
            PaletteMode::Command => ">",
            PaletteMode::Path | PaletteMode::Slash => "/",
            PaletteMode::Search | PaletteMode::Url | PaletteMode::Picking(_) => "",
        };
        !prefix.is_empty() && query == prefix
    }

    pub(crate) const fn picker(mode: PaletteMode) -> Option<CommandBarPicker> {
        match mode {
            PaletteMode::Picking(picker) => Some(picker),
            _ => None,
        }
    }

    pub(crate) const fn is_ex(mode: PaletteMode) -> bool {
        matches!(mode, PaletteMode::Ex)
    }

    pub(crate) const fn is_space(mode: PaletteMode) -> bool {
        matches!(mode, PaletteMode::Picking(CommandBarPicker::Space))
    }

    fn mode(
        query: &str,
        asserted: Option<CommandBarPicker>,
        slash_commands: &[vmux_api::chat::SlashCommandEntry],
    ) -> PaletteMode {
        if let Some(picker) = asserted {
            return PaletteMode::Picking(picker);
        }
        if query.starts_with(':') {
            return PaletteMode::Ex;
        }
        let trimmed = query.trim();
        if trimmed.starts_with('>') {
            return PaletteMode::Command;
        }
        if Self::names_a_command(query, slash_commands) {
            return PaletteMode::Slash;
        }
        if trimmed.starts_with('/') || trimmed.starts_with('~') {
            return PaletteMode::Path;
        }
        if trimmed.contains("://") || (trimmed.contains('.') && !trimmed.contains(' ')) {
            return PaletteMode::Url;
        }
        PaletteMode::Search
    }

    fn names_a_command(query: &str, slash_commands: &[vmux_api::chat::SlashCommandEntry]) -> bool {
        if query.trim() == "/" {
            return !slash_commands.is_empty();
        }
        let held = PaletteQuery::new(query);
        let Some((name, _)) = held.slash_token() else {
            return false;
        };
        let lowered = name.to_lowercase();
        slash_commands
            .iter()
            .any(|command| SlashRows::name(command.command).starts_with(&lowered))
    }

    fn with_completions(
        query: &str,
        draft: &PaletteDraft,
        matched: Vec<CommandBarResultItem>,
    ) -> Vec<CommandBarResultItem> {
        FileRows::merge(query, Completions::for_query(draft, query), matched)
    }

    fn listed(
        state: &CommandBarOpenEvent,
        draft: &PaletteDraft,
        surface: CommandPaletteSurface,
        mode: PaletteMode,
        start_prompt_mode: bool,
    ) -> Vec<CommandBarResultItem> {
        let query = draft.query.as_str();
        let is_start = surface.is_start();
        if let Some(picker) = Self::picker(mode) {
            if Self::is_space(mode) {
                return SpaceRows::space_switch(
                    &state.spaces,
                    &state.pages,
                    &state.spaces_page_url,
                    query,
                );
            }
            return PickerRows::filtered(picker, &state.picks, query);
        }
        if Self::is_ex(mode) {
            if is_start {
                return Vec::new();
            }
            return ExLine::suggestions(query);
        }
        if mode == PaletteMode::Slash {
            return SlashRows::for_query(
                query,
                state.prompt_context.slash_commands.as_slice(),
                &draft.sessions,
                draft.sessions_pending,
            );
        }
        if is_start && query.trim().is_empty() {
            return PageRows::open_sessions(&state.tabs, &state.pages);
        }
        if start_prompt_mode {
            let matched = StartRows::start(
                &state.pages,
                &state.work_dirs,
                &state.recent_files,
                &state.search_engines,
                &state.terminal_page_url,
                query,
            );
            return Self::with_completions(query, draft, matched);
        }
        let is_new_tab = matches!(state.target, Some(OpenTarget::InNewStack));
        let matched = SearchRows::filter(SearchRowsInput {
            query,
            tabs: &state.tabs,
            commands: &state.commands,
            spaces: &state.spaces,
            pages: &state.pages,
            new_tab: is_new_tab,
            history: &draft.history,
            work_dirs: &state.work_dirs,
            recent_files: &state.recent_files,
            spaces_page_url: &state.spaces_page_url,
        });
        let matched = Self::with_completions(query, draft, matched);
        if !is_start {
            return matched;
        }
        let startup_url = state
            .pages
            .iter()
            .find(|page| page.startup)
            .map(|page| page.url.trim_end_matches('/'));
        let mut kept = Vec::with_capacity(matched.len());
        for item in matched {
            let (CommandBarResultItem::Stack { url, .. } | CommandBarResultItem::Page { url, .. }) =
                &item
            else {
                kept.push(item);
                continue;
            };
            if startup_url == Some(url.trim_end_matches('/')) {
                continue;
            }
            kept.push(item);
        }
        kept
    }

    fn ghost_of(query: &str, completions: &[PathEntry]) -> String {
        if CompletionQuery::parse(query).is_none() {
            return String::new();
        }
        let Some(first) = completions.first() else {
            return String::new();
        };
        let typed = query.trim();
        let full = &first.full_path;
        if !full.to_lowercase().starts_with(&typed.to_lowercase())
            || !full.is_char_boundary(typed.len())
        {
            return String::new();
        }
        full[typed.len()..].to_string()
    }

    pub fn selected(&self, stored: usize) -> usize {
        stored.min(self.items.len().saturating_sub(1))
    }

    #[cfg(test)]
    pub fn step(&self, from: usize, direction: MenuDirection) -> usize {
        match direction {
            MenuDirection::Next => (from + 1).min(self.items.len().saturating_sub(1)),
            MenuDirection::Previous => from.saturating_sub(1),
        }
    }
}

struct Glyph;

impl Glyph {
    fn resolve(
        navigating: Option<&CommandBarResultItem>,
        mode: PaletteMode,
    ) -> Option<PaletteGlyph> {
        if let PaletteMode::Picking(picker) = mode
            && picker != CommandBarPicker::Space
        {
            return None;
        }
        let Some(item) = navigating else {
            return Self::in_mode(mode);
        };
        let glyph = match item {
            CommandBarResultItem::Command { .. }
            | CommandBarResultItem::Ex { .. }
            | CommandBarResultItem::Slash { .. }
            | CommandBarResultItem::Resume { .. }
            | CommandBarResultItem::Pick { .. } => PaletteGlyph::Command,
            CommandBarResultItem::Terminal { path } if path.is_empty() => PaletteGlyph::Command,
            CommandBarResultItem::Terminal { .. }
            | CommandBarResultItem::Editor { .. }
            | CommandBarResultItem::File { .. }
            | CommandBarResultItem::WorkDir { .. }
            | CommandBarResultItem::PartialIndex
            | CommandBarResultItem::MoreMatches { .. }
            | CommandBarResultItem::ResumePending { .. }
            | CommandBarResultItem::RecentFile { .. } => PaletteGlyph::Path,
            CommandBarResultItem::Stack { .. } | CommandBarResultItem::History { .. } => {
                PaletteGlyph::Url
            }
            CommandBarResultItem::Navigate { is_url, .. } => {
                if *is_url {
                    PaletteGlyph::Url
                } else {
                    PaletteGlyph::Search
                }
            }
            CommandBarResultItem::Space { .. }
            | CommandBarResultItem::Page { .. }
            | CommandBarResultItem::Search { .. } => PaletteGlyph::Search,
        };
        Some(glyph)
    }

    const fn in_mode(mode: PaletteMode) -> Option<PaletteGlyph> {
        match mode {
            PaletteMode::Command | PaletteMode::Ex | PaletteMode::Slash => {
                Some(PaletteGlyph::Command)
            }
            PaletteMode::Path => Some(PaletteGlyph::Path),
            PaletteMode::Url => Some(PaletteGlyph::Url),
            PaletteMode::Picking(CommandBarPicker::Space) | PaletteMode::Search => {
                Some(PaletteGlyph::Search)
            }
            PaletteMode::Picking(_) => None,
        }
    }
}

pub struct ExLine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExCommandName {
    pub name: &'static str,
    pub hint: &'static str,
}

impl ExLine {
    pub const COMMANDS: [ExCommandName; 8] = [
        ExCommandName {
            name: "w",
            hint: "ex-write",
        },
        ExCommandName {
            name: "wq",
            hint: "ex-write-quit",
        },
        ExCommandName {
            name: "q",
            hint: "ex-quit",
        },
        ExCommandName {
            name: "q!",
            hint: "ex-quit-force",
        },
        ExCommandName {
            name: "noh",
            hint: "ex-nohighlight",
        },
        ExCommandName {
            name: "d",
            hint: "ex-delete",
        },
        ExCommandName {
            name: "y",
            hint: "ex-yank",
        },
        ExCommandName {
            name: "s/",
            hint: "ex-substitute",
        },
    ];

    pub fn parse(query: &str) -> Option<String> {
        let body = query.strip_prefix(':')?.trim();
        (!body.is_empty()).then(|| body.to_string())
    }

    pub fn suggestions(query: &str) -> Vec<CommandBarResultItem> {
        let typed = query.strip_prefix(':').unwrap_or(query).trim_start();
        let mut rows = Vec::new();
        for entry in Self::COMMANDS {
            if !entry.name.starts_with(typed) {
                continue;
            }
            rows.push(CommandBarResultItem::Ex {
                name: entry.name.to_string(),
                hint: translate(entry.hint),
            });
        }
        rows
    }
}

struct Composer;

impl Composer {
    fn build(
        state: &CommandBarOpenEvent,
        prompt_targets: &[CommandBarResultItem],
        effective_target: Option<&CommandBarResultItem>,
    ) -> CommandPaletteComposer {
        let context = &state.prompt_context;
        let agent_url = effective_target
            .and_then(PageRows::prompt_target_url)
            .unwrap_or_default()
            .to_string();
        let agent_title = match effective_target {
            Some(CommandBarResultItem::Page { title, .. }) => title.clone(),
            _ => "Agent".to_string(),
        };
        let mut agents = Vec::new();
        for item in prompt_targets {
            let CommandBarResultItem::Page { url, title, .. } = item else {
                continue;
            };
            agents.push(CommandPaletteAgent {
                url: url.clone(),
                title: title.clone(),
            });
        }
        let models = SelectedAgentModels::find(&state.agent_models, &agent_url);
        let modes = SelectedAgentModes::find(&state.agent_modes, &agent_url);
        let active_project = context.projects.iter().find(|project| project.is_active);
        let workspace_label = if let Some(project) = active_project {
            project.label.clone()
        } else if !context.workspace_name.is_empty() {
            context.workspace_name.clone()
        } else {
            translate("agent-project-select")
        };
        let workspace_path = active_project
            .map(|project| project.path.as_str())
            .filter(|path| !path.is_empty())
            .unwrap_or(&context.cwd);
        let workspace_title = if workspace_path.is_empty() {
            translate("agent-project-choose")
        } else {
            format!(
                "{} \u{00b7} {}",
                translate("agent-project-choose"),
                workspace_path
            )
        };
        let branch_label = if context.branch.is_empty() {
            "Git".to_string()
        } else {
            context.branch.clone()
        };
        let branch_title = if context.branch.is_empty() {
            "Git repository".to_string()
        } else {
            format!("Branch {}", context.branch)
        };
        let worktree_title = if context.base_ref.is_empty() {
            "Linked worktree".to_string()
        } else {
            format!("Worktree from {}", context.base_ref)
        };

        CommandPaletteComposer {
            loading: state.pages.is_empty(),
            agents,
            agent_title,
            agent_url,
            model_name: SelectedAgentModels::name(models),
            model_options: models.map(|row| row.models.clone()).unwrap_or_default(),
            model_agent_key: models.map(|row| row.agent_key.clone()).unwrap_or_default(),
            model_current_id: models.map(|row| row.selected.clone()).unwrap_or_default(),
            permission_modes: modes.map(|row| row.modes.clone()).unwrap_or_default(),
            permission_agent_key: modes.map(|row| row.agent_key.clone()).unwrap_or_default(),
            permission_current_id: modes.map(|row| row.selected.clone()).unwrap_or_default(),
            workspace_label,
            workspace_title,
            branch_label,
            branch_title,
            worktree_title,
            project: workspace_path.to_string(),
            projects: context.projects.clone(),
            cwd: context.cwd.clone(),
            is_git_repo: context.is_git_repo,
            is_worktree: context.is_worktree,
            uncommitted: context.uncommitted,
            ahead: context.ahead,
        }
    }
}

#[cfg(test)]
mod tests;
