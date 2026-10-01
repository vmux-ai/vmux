use vmux_api::command_bar::{
    AgentModels, AgentModes, CommandBarOpenEvent, CommandBarPick, CommandBarPicker,
    CommandPaletteAgent, CommandPaletteComposer, CommandPaletteProjection, ExRequest, HistoryEntry,
    InvokeRequest, OpenRequest, PathEntry, PickRequest, PromptRequest, SwitchSpaceRequest,
    SwitchTabRequest, TerminalRequest,
};
use vmux_api::open_target::OpenTarget;
use vmux_api::prompt_media::{ChatAttachment, ChatSubmitAttachment};

use vmux_ui::i18n::translate;
#[cfg(test)]
use vmux_ui::list_nav::MenuDirection;

pub use self::results::ResumeRows;
use self::results::{CommandBarResultItem, PickerRows, SlashRows};

mod query;
mod results;

pub(super) use query::PaletteQuery;

use vmux_api::command_bar::{PaletteGlyph, PaletteMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteSurface {
    Modal,
    Start,
}

impl PaletteSurface {
    pub const fn is_start(self) -> bool {
        matches!(self, Self::Start)
    }
}

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
        surface: PaletteSurface,
    ) -> Self {
        let query = draft.query.as_str();
        let is_start = surface.is_start();
        let slash_commands = state.prompt_context.slash_commands.as_slice();
        let mode = Self::mode(query, state.picker, slash_commands);
        let prompt_targets = if is_start {
            PaletteRows::prompt_targets(&state.pages, "")
        } else {
            Vec::new()
        };
        let default_target = prompt_targets
            .iter()
            .find(|item| PaletteRows::prompt_target_url(item) == Some(draft.target_url.as_str()))
            .cloned()
            .or_else(|| prompt_targets.first().cloned());
        let start_prompt_mode = is_start && PaletteQuery::new(query).is_start_prompt();

        let mut items = FileRows::under_projects(
            Self::listed(state, draft, surface, mode, start_prompt_mode),
            &state.projects,
        );
        if start_prompt_mode {
            PaletteRows::prepend_targets(
                &mut items,
                default_target.as_ref(),
                &prompt_targets,
                query,
            );
        }
        for item in &mut items {
            let show_hint = start_prompt_mode
                && PaletteRows::prompt_target_url(item).is_some()
                && !PaletteRows::prompt_target_matches(item, query);
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
        surface: PaletteSurface,
        mode: PaletteMode,
        start_prompt_mode: bool,
    ) -> Vec<CommandBarResultItem> {
        let query = draft.query.as_str();
        let is_start = surface.is_start();
        if let Some(picker) = Self::picker(mode) {
            if Self::is_space(mode) {
                return PaletteRows::space_switch(
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
            return PaletteRows::open_sessions(&state.tabs, &state.pages);
        }
        if start_prompt_mode {
            let matched = PaletteRows::start(
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
        let matched = PaletteRows::filter(
            query,
            &state.tabs,
            &state.commands,
            &state.spaces,
            &state.pages,
            is_new_tab,
            &draft.history,
            &state.work_dirs,
            &state.recent_files,
            &state.spaces_page_url,
        );
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
            .and_then(PaletteRows::prompt_target_url)
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

#[derive(Clone, Debug, Default, PartialEq)]
pub enum PaletteDecision {
    #[default]
    None,
    Close,
    Retype(String),
    Prompt {
        close: bool,
        request: PromptRequest,
    },
    Open {
        close: bool,
        request: OpenRequest,
    },
    Terminal(TerminalRequest),
    Invoke(InvokeRequest),
    SwitchSpace(SwitchSpaceRequest),
    SwitchTab(SwitchTabRequest),
    Ex(ExRequest),
    Pick(PickRequest),
}

impl PaletteDecision {
    fn closing() -> Self {
        Self::Close
    }

    fn prompt(close: bool, text: &str, target_url: &str, attachments: &[ChatAttachment]) -> Self {
        let mut submitted = Vec::with_capacity(attachments.len());
        for attachment in attachments {
            submitted.push(ChatSubmitAttachment::from(attachment));
        }
        Self::Prompt {
            close,
            request: PromptRequest {
                text: text.to_string(),
                target_url: (!target_url.is_empty()).then(|| target_url.to_string()),
                attachments: submitted,
            },
        }
    }

    fn open(close: bool, value: &str, open: Option<OpenTarget>) -> Self {
        Self::Open {
            close,
            request: OpenRequest {
                value: value.to_string(),
                open,
            },
        }
    }

    fn terminal(value: String) -> Self {
        Self::Terminal(TerminalRequest { value })
    }

    fn invoke(id: String, open: Option<OpenTarget>) -> Self {
        Self::Invoke(InvokeRequest { id, open })
    }

    fn switch_space(id: String) -> Self {
        Self::SwitchSpace(SwitchSpaceRequest { id })
    }

    fn switch_tab(pane: u64, index: usize) -> Self {
        Self::SwitchTab(SwitchTabRequest { pane, index })
    }

    fn ex(line: String) -> Self {
        Self::Ex(ExRequest { line })
    }

    fn pick(pick: CommandBarPick) -> Self {
        Self::Pick(PickRequest { pick })
    }

    fn retyping(query: impl Into<String>) -> Self {
        Self::Retype(query.into())
    }

    pub const fn closes(&self) -> bool {
        match self {
            Self::Prompt { close, .. } | Self::Open { close, .. } => *close,
            Self::Close
            | Self::Terminal(_)
            | Self::Invoke(_)
            | Self::SwitchSpace(_)
            | Self::SwitchTab(_)
            | Self::Ex(_)
            | Self::Pick(_) => true,
            Self::None | Self::Retype(_) => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaletteState {
    pub surface: PaletteSurface,
    pub query: String,
    pub rows: Vec<CommandBarResultItem>,
    pub selected: usize,
    pub ghost: String,
    pub row_text: Option<String>,
    pub placeholder: String,
    pub glyph: Option<PaletteGlyph>,
    pub mode: PaletteMode,
    pub start_prompt_mode: bool,
    pub space_switch: bool,
    pub nav_mode: bool,
    pub open_target: Option<OpenTarget>,
    pub space_name: String,
    pub prompt_targets: Vec<CommandBarResultItem>,
    pub default_target: Option<CommandBarResultItem>,
    pub effective_target: Option<CommandBarResultItem>,
    pub accent_agent: Option<String>,
    pub composer: CommandPaletteComposer,
}

impl PaletteState {
    #[cfg(test)]
    pub fn resolve(
        state: &CommandBarOpenEvent,
        draft: &PaletteDraft,
        surface: PaletteSurface,
    ) -> Self {
        Self::from_rows(
            &PaletteRows::build(state, draft, surface),
            state,
            draft,
            surface,
        )
    }

    pub fn from_rows(
        rows: &PaletteRows,
        state: &CommandBarOpenEvent,
        draft: &PaletteDraft,
        surface: PaletteSurface,
    ) -> Self {
        let selected = rows.selected(draft.selected);
        let active = rows.items.get(selected);
        let navigating = if draft.nav_mode { active } else { None };
        let effective_target = active
            .filter(|item| PaletteRows::prompt_target_url(item).is_some())
            .or(rows.default_target.as_ref())
            .cloned();
        let accent_agent = AgentSegment::from_item(if draft.nav_mode {
            active
        } else {
            rows.default_target.as_ref()
        })
        .or_else(|| AgentSegment::from_item(rows.default_target.as_ref()));
        let row_text = if rows.start_prompt_mode {
            None
        } else {
            RowText::over(navigating, &draft.query)
        };

        Self {
            surface,
            query: draft.query.clone(),
            rows: rows.items.clone(),
            selected,
            ghost: rows.ghost.clone(),
            row_text,
            placeholder: Placeholder::resolve(rows.mode, state, surface),
            glyph: Glyph::resolve(navigating, rows.mode),
            mode: rows.mode,
            start_prompt_mode: rows.start_prompt_mode,
            space_switch: PaletteRows::is_space(rows.mode),
            nav_mode: draft.nav_mode,
            open_target: state.target,
            space_name: state.space_name.clone(),
            prompt_targets: rows.prompt_targets.clone(),
            default_target: rows.default_target.clone(),
            composer: Composer::build(state, &rows.prompt_targets, effective_target.as_ref()),
            effective_target,
            accent_agent,
        }
    }

    pub fn row(&self, index: usize) -> Option<&CommandBarResultItem> {
        self.rows.get(index)
    }

    pub fn projection(&self) -> CommandPaletteProjection {
        let space_count = self
            .rows
            .iter()
            .filter(|row| matches!(row, CommandBarResultItem::Space { .. }))
            .count() as u32;
        CommandPaletteProjection {
            query: self.query.clone(),
            rows: self.rows.clone(),
            selected: self.selected as u32,
            navigating: self.nav_mode,
            row_text: self.row_text.clone(),
            placeholder: self.placeholder.clone(),
            glyph: self.glyph,
            space_switch: self.space_switch,
            space_count,
            space_name: self.space_name.clone(),
            accent_agent: self.accent_agent.clone(),
            composer: self.composer.clone(),
            prompt_targets: self.prompt_targets.clone(),
            default_target: self.default_target.clone(),
            ghost: self.ghost.clone(),
            start_prompt_mode: self.start_prompt_mode,
            mode: self.mode,
            ..Default::default()
        }
    }

    #[cfg(test)]
    pub fn space_digit(&self, digit: usize) -> Option<usize> {
        let spaces = self
            .rows
            .iter()
            .filter(|row| matches!(row, CommandBarResultItem::Space { .. }))
            .count();
        (digit < spaces).then_some(digit)
    }

    pub fn accepts_typed(&self, item: &CommandBarResultItem) -> bool {
        self.nav_mode
            || PaletteRows::prompt_target_matches(item, &self.query)
            || (matches!(item, CommandBarResultItem::Terminal { .. })
                && PaletteRows::terminal_matches(&self.query))
    }

    pub fn activate(
        &self,
        item: &CommandBarResultItem,
        attachments: &[ChatAttachment],
    ) -> PaletteDecision {
        if self.surface.is_start()
            && (PaletteQuery::new(&self.query).is_start_prompt() || !attachments.is_empty())
            && let Some(target_url) = PaletteRows::prompt_target_url(item)
        {
            return if PaletteRows::prompt_target_matches(item, &self.query)
                && attachments.is_empty()
            {
                PaletteDecision::open(true, target_url, self.open_target)
            } else {
                PaletteDecision::prompt(true, self.query.trim(), target_url, attachments)
            };
        }

        if let CommandBarResultItem::Slash { name, .. } = item {
            return PaletteDecision::retyping(format!("/{name} "));
        }
        self.acted(item).unwrap_or_else(PaletteDecision::closing)
    }

    fn acted(&self, item: &CommandBarResultItem) -> Option<PaletteDecision> {
        match item {
            CommandBarResultItem::Slash { .. } => None,
            CommandBarResultItem::Resume { entry, .. } => {
                Some(PaletteDecision::open(true, &entry.url, self.open_target))
            }
            CommandBarResultItem::Terminal { path } => {
                Some(PaletteDecision::terminal(path.clone()))
            }
            CommandBarResultItem::Editor { path } | CommandBarResultItem::File { path, .. } => {
                Some(PaletteDecision::open(
                    true,
                    &format!("file://{path}"),
                    self.open_target,
                ))
            }
            CommandBarResultItem::WorkDir { path, .. } => Some(PaletteDecision::open(
                true,
                &format!("file://{path}"),
                self.open_target,
            )),
            CommandBarResultItem::Stack {
                pane_id, tab_index, ..
            } => Some(PaletteDecision::switch_tab(*pane_id, *tab_index)),
            CommandBarResultItem::Command { id, .. } => {
                Some(PaletteDecision::invoke(id.clone(), self.open_target))
            }
            CommandBarResultItem::Ex { name, .. } => Some(PaletteDecision::ex(name.clone())),
            CommandBarResultItem::Pick { pick, .. } => Some(PaletteDecision::pick(pick.clone())),
            CommandBarResultItem::Space { id, .. } => {
                Some(PaletteDecision::switch_space(id.clone()))
            }
            CommandBarResultItem::Page { url, .. }
            | CommandBarResultItem::Navigate { url, .. }
            | CommandBarResultItem::History { url, .. } => {
                (!url.is_empty()).then(|| PaletteDecision::open(true, url, self.open_target))
            }
            CommandBarResultItem::RecentFile { url, .. } => {
                Some(PaletteDecision::open(true, url, self.open_target))
            }
            CommandBarResultItem::Search { engine, query } => Some(PaletteDecision::open(
                true,
                &engine.query_url(query),
                self.open_target,
            )),
            CommandBarResultItem::PartialIndex
            | CommandBarResultItem::MoreMatches { .. }
            | CommandBarResultItem::ResumePending { .. } => None,
        }
    }

    pub fn submit_modal(&self, attachments: &[ChatAttachment]) -> PaletteDecision {
        if let Some(picker) = PaletteRows::picker(self.mode) {
            return self.submit_picked(picker, attachments);
        }
        if PaletteRows::is_ex(self.mode) {
            if self.nav_mode
                && let Some(item) = self.row(self.selected)
            {
                return self.activate(item, attachments);
            }
            let Some(line) = ExLine::parse(&self.query) else {
                return PaletteDecision::default();
            };
            return PaletteDecision::ex(line);
        }
        self.submit_typed(attachments)
    }

    fn submit_picked(
        &self,
        picker: CommandBarPicker,
        attachments: &[ChatAttachment],
    ) -> PaletteDecision {
        if PickerRows::takes_typed_value(picker) {
            let Some(pick) = PickerRows::typed(picker, &self.query) else {
                return PaletteDecision::default();
            };
            return PaletteDecision::pick(pick);
        }
        let Some(item) = self.row(self.selected) else {
            return PaletteDecision::default();
        };
        self.activate(item, attachments)
    }

    pub fn submit_start(&self, attachments: &[ChatAttachment]) -> PaletteDecision {
        if self.mode == PaletteMode::Slash && self.rows.is_empty() {
            return PaletteDecision::default();
        }
        if self.query.trim().is_empty() && !attachments.is_empty() {
            if let Some(item) = self.default_target.as_ref() {
                return self.activate(item, attachments);
            }
            return PaletteDecision::prompt(false, "", "", attachments);
        }
        if self.space_switch {
            let Some(item) = self.row(self.selected) else {
                return PaletteDecision::default();
            };
            return self.activate(item, attachments);
        }
        if !self.start_prompt_mode {
            return self.submit_typed(attachments);
        }
        if let Some(item) = self
            .row(self.selected)
            .filter(|item| self.accepts_typed(item))
        {
            return self.activate(item, attachments);
        }
        if let Some(item) = self.default_target.as_ref() {
            return self.activate(item, attachments);
        }
        PaletteDecision::prompt(true, self.query.trim(), "", attachments)
    }

    fn submit_typed(&self, attachments: &[ChatAttachment]) -> PaletteDecision {
        if !TypedRow::beats_a_guessed_url(self.row(self.selected), &self.query)
            && PaletteQuery::new(&self.query)
                .opens_typed_url_on_enter(self.open_target, self.nav_mode)
        {
            return PaletteDecision::open(true, &self.query, self.open_target);
        }
        if let Some(item) = self.row(self.selected) {
            return self.activate(item, attachments);
        }
        if !self.query.is_empty() {
            return PaletteDecision::open(false, &self.query, self.open_target);
        }
        PaletteDecision::default()
    }

    pub fn opening_selection(state: &CommandBarOpenEvent) -> usize {
        if state.picker == Some(CommandBarPicker::Space) {
            state
                .spaces
                .iter()
                .position(|space| space.is_active)
                .unwrap_or(0)
        } else {
            0
        }
    }
}

struct TypedRow;

impl TypedRow {
    fn beats_a_guessed_url(row: Option<&CommandBarResultItem>, query: &str) -> bool {
        let query = query.trim();
        let Some(row) = row else {
            return false;
        };
        match row {
            CommandBarResultItem::Page { url, .. } => {
                query.starts_with("vmux://") && url.starts_with(query)
            }
            CommandBarResultItem::File { path, is_dir, .. } => {
                !is_dir && Self::is_named(path, query)
            }
            CommandBarResultItem::Editor { path } => Self::is_named(path, query),
            CommandBarResultItem::RecentFile { title, .. } => Self::is_called(title, query),
            _ => false,
        }
    }

    fn is_named(path: &str, query: &str) -> bool {
        Self::is_called(path.rsplit('/').next().unwrap_or(path), query)
    }

    fn is_called(name: &str, query: &str) -> bool {
        !query.contains("://") && name.eq_ignore_ascii_case(query)
    }
}

struct Placeholder;

impl Placeholder {
    fn resolve(mode: PaletteMode, state: &CommandBarOpenEvent, surface: PaletteSurface) -> String {
        if let Some(picker) = PaletteRows::picker(mode) {
            return translate(PickerRows::placeholder(picker));
        }
        if PaletteRows::is_ex(mode) {
            return translate("command-ex-placeholder");
        }
        match surface {
            PaletteSurface::Start => translate("command-search-ask"),
            PaletteSurface::Modal => {
                if matches!(state.target, Some(OpenTarget::InNewStack)) {
                    translate("command-new-tab-placeholder")
                } else {
                    translate("command-placeholder")
                }
            }
        }
    }
}

struct RowText;

impl RowText {
    fn over(item: Option<&CommandBarResultItem>, query: &str) -> Option<String> {
        let item = item?;
        if Self::names_itself_in_the_row(item) {
            return None;
        }
        let text = Self::resolve(item, query);
        if text == query {
            return None;
        }
        Some(text)
    }

    fn names_itself_in_the_row(item: &CommandBarResultItem) -> bool {
        matches!(
            item,
            CommandBarResultItem::File { .. }
                | CommandBarResultItem::Editor { .. }
                | CommandBarResultItem::WorkDir { .. }
                | CommandBarResultItem::Resume { .. }
        )
    }

    fn resolve(item: &CommandBarResultItem, query: &str) -> String {
        match item {
            CommandBarResultItem::Command { name, .. } => format!("> {name}"),
            CommandBarResultItem::Ex { name, .. } => format!(":{name}"),
            CommandBarResultItem::Slash { name, .. } => format!("/{name} "),
            CommandBarResultItem::Resume { entry, .. } => entry.title.clone(),
            CommandBarResultItem::Pick { label, .. } => label.clone(),
            CommandBarResultItem::Navigate { url, .. } => url.clone(),
            CommandBarResultItem::Search { query, .. } => query.clone(),
            CommandBarResultItem::Stack { url, .. } => url.clone(),
            CommandBarResultItem::Space { name, .. } => name.clone(),
            CommandBarResultItem::Page { title, .. } => title.clone(),
            CommandBarResultItem::Terminal { path } if path.is_empty() => {
                translate("command-terminal")
            }
            CommandBarResultItem::Terminal { path } => path.clone(),
            CommandBarResultItem::Editor { path } => path.clone(),
            CommandBarResultItem::History { title, url, .. } => Self::titled(title, url),
            CommandBarResultItem::File { path, .. } => path.clone(),
            CommandBarResultItem::WorkDir { path, .. } => path.clone(),
            CommandBarResultItem::RecentFile { title, url } => Self::titled(title, url),
            CommandBarResultItem::PartialIndex
            | CommandBarResultItem::MoreMatches { .. }
            | CommandBarResultItem::ResumePending { .. } => query.to_string(),
        }
    }

    fn titled(title: &str, url: &str) -> String {
        if title.is_empty() {
            url.to_string()
        } else {
            title.to_string()
        }
    }
}

pub struct AgentSegment;

impl AgentSegment {
    fn from_item(item: Option<&CommandBarResultItem>) -> Option<String> {
        Self::in_url(PaletteRows::prompt_target_url(item?)?)
    }

    pub fn in_url(url: &str) -> Option<String> {
        let path = url
            .strip_prefix("vmux://sessions/")
            .or_else(|| url.strip_prefix("vmux://agent/"))?;
        let segment = path.split('/').next()?;
        (!segment.is_empty()).then(|| segment.to_string())
    }
}

pub struct SelectedAgentModels;

struct AgentCatalogUrl;

impl AgentCatalogUrl {
    fn matches(left: &str, right: &str) -> bool {
        left.trim_end_matches('/') == right.trim_end_matches('/')
    }
}

impl SelectedAgentModels {
    pub fn find<'a>(rows: &'a [AgentModels], target_url: &str) -> Option<&'a AgentModels> {
        if target_url.is_empty() {
            return None;
        }
        rows.iter()
            .find(|row| AgentCatalogUrl::matches(&row.url, target_url))
    }

    pub fn name(row: Option<&AgentModels>) -> String {
        let Some(row) = row else {
            return String::new();
        };
        for model in &row.models {
            if model.id == row.selected {
                return model.name.clone();
            }
        }
        String::new()
    }
}

pub struct SelectedAgentModes;

impl SelectedAgentModes {
    pub fn find<'a>(rows: &'a [AgentModes], target_url: &str) -> Option<&'a AgentModes> {
        if target_url.is_empty() {
            return None;
        }
        rows.iter()
            .find(|row| AgentCatalogUrl::matches(&row.url, target_url))
    }
}

pub struct CompletionQuery;

impl CompletionQuery {
    pub fn parse(input: &str) -> Option<String> {
        let trimmed = input.trim();
        if let Some(rest) = trimmed.strip_prefix("file://") {
            return Some(rest.to_string());
        }
        if PaletteQuery::new(trimmed).looks_like_path() {
            return Some(trimmed.to_string());
        }
        if trimmed.is_empty() || trimmed.contains("://") || PaletteQuery::new(trimmed).is_data_uri()
        {
            return None;
        }
        Some(trimmed.to_string())
    }

    pub fn names_a_file(value: &str) -> bool {
        for term in value.split_whitespace() {
            if term.contains('/') {
                return true;
            }
            let Some((stem, extension)) = term.rsplit_once('.') else {
                continue;
            };
            if stem.is_empty() || extension.is_empty() || extension.len() > 5 {
                continue;
            }
            if extension.chars().all(|c| c.is_ascii_alphanumeric()) {
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Completions<'a> {
    pub entries: &'a [PathEntry],
    pub partial: bool,
    pub total: usize,
}

impl<'a> Completions<'a> {
    pub fn for_query(draft: &'a PaletteDraft, query: &str) -> Self {
        if CompletionQuery::parse(query).is_none() {
            return Self::default();
        }
        Self {
            entries: &draft.completions,
            partial: draft.completions_partial,
            total: draft.completions_total,
        }
    }

    fn withheld(&self) -> usize {
        self.total.saturating_sub(self.entries.len())
    }
}

pub struct FileRows;

impl FileRows {
    pub fn merge(
        query: &str,
        completions: Completions<'_>,
        matched: Vec<CommandBarResultItem>,
    ) -> Vec<CommandBarResultItem> {
        if completions.entries.is_empty() && !completions.partial {
            return matched;
        }
        let trimmed = query.trim();
        let leads =
            PaletteQuery::new(trimmed).looks_like_path() || CompletionQuery::names_a_file(trimmed);
        let mut files = Vec::with_capacity(completions.entries.len());
        let mut listed = Vec::with_capacity(completions.entries.len());
        for entry in completions.entries {
            files.push(CommandBarResultItem::File {
                path: entry.full_path.clone(),
                is_dir: entry.is_dir,
                project: entry.project.clone(),
                relative: entry.name.clone(),
            });
            listed.push(entry.full_path.as_str());
        }
        let mut rest = Vec::with_capacity(matched.len());
        for item in matched {
            if Self::already_listed(&item, &listed) {
                continue;
            }
            rest.push(item);
        }
        let mut merged = if leads {
            files.extend(rest);
            files
        } else {
            let mut ahead = Vec::with_capacity(rest.len());
            let mut fallbacks = Vec::new();
            for item in rest {
                match item {
                    CommandBarResultItem::Search { .. } => fallbacks.push(item),
                    _ => ahead.push(item),
                }
            }
            ahead.extend(files);
            ahead.extend(fallbacks);
            ahead
        };
        if completions.partial {
            merged.push(CommandBarResultItem::PartialIndex);
        }
        if completions.withheld() > 0 {
            merged.push(CommandBarResultItem::MoreMatches {
                shown: completions.entries.len(),
                total: completions.total,
            });
        }
        merged
    }

    fn already_listed(item: &CommandBarResultItem, listed: &[&str]) -> bool {
        let Some(path) = Self::local_path(item) else {
            return false;
        };
        listed.contains(&path)
    }

    fn local_path(item: &CommandBarResultItem) -> Option<&str> {
        match item {
            CommandBarResultItem::Editor { path } => Some(path.as_str()),
            CommandBarResultItem::RecentFile { url, .. }
            | CommandBarResultItem::History { url, .. } => url.strip_prefix("file://"),
            _ => None,
        }
    }

    pub fn under_projects(
        items: Vec<CommandBarResultItem>,
        projects: &[String],
    ) -> Vec<CommandBarResultItem> {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            let Some(path) = Self::local_path(&item) else {
                out.push(item);
                continue;
            };
            let Some((project, relative)) = ProjectPath::split(path, projects) else {
                out.push(item);
                continue;
            };
            out.push(CommandBarResultItem::File {
                path: path.to_string(),
                is_dir: false,
                project,
                relative,
            });
        }
        out
    }
}

struct ProjectPath;

impl ProjectPath {
    fn split(path: &str, projects: &[String]) -> Option<(String, String)> {
        let mut owner = "";
        for project in projects {
            let root = project.trim().trim_end_matches('/');
            if root.is_empty() || root.len() <= owner.len() {
                continue;
            }
            let Some(rest) = path.strip_prefix(root) else {
                continue;
            };
            if !rest.starts_with('/') {
                continue;
            }
            owner = root;
        }
        if owner.is_empty() {
            return None;
        }
        let label = owner.rsplit('/').next().unwrap_or(owner);
        Some((label.to_string(), path[owner.len() + 1..].to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::command_bar::{
        CommandBarCommandEntry, CommandBarPage, CommandBarPromptContext, CommandBarSpace,
        CommandBarTab, SearchEngine,
    };
    use vmux_api::protocol::AcpModeOption;
    use vmux_api::room::ModelOptionEntry;
    use vmux_api::space::ProjectRow;

    impl<'a> Completions<'a> {
        fn listing(entries: &'a [PathEntry]) -> Self {
            Self {
                entries,
                partial: false,
                total: entries.len(),
            }
        }

        fn partial(entries: &'a [PathEntry]) -> Self {
            Self {
                entries,
                partial: true,
                total: entries.len(),
            }
        }
    }

    impl FileRows {
        fn hits(paths: &[&str]) -> Vec<PathEntry> {
            let mut entries = Vec::new();
            for path in paths {
                entries.push(PathEntry {
                    name: (*path).to_string(),
                    is_dir: false,
                    full_path: format!("/root/{path}"),
                    project: "root".to_string(),
                });
            }
            entries
        }

        fn a_command() -> CommandBarResultItem {
            CommandBarResultItem::Command {
                id: "settings".to_string(),
                name: "Settings".to_string(),
                shortcut: String::new(),
            }
        }
    }

    struct Launcher;

    impl Launcher {
        fn state() -> CommandBarOpenEvent {
            CommandBarOpenEvent {
                pages: vec![
                    CommandBarPage {
                        url: "vmux://settings/".into(),
                        title: "Settings".into(),
                        keywords: vec!["preferences".into()],
                        icon: vmux_api::PageIcon::None,
                        shortcut: String::new(),
                        prompt_target: false,
                        startup: false,
                    },
                    CommandBarPage {
                        url: "vmux://sessions/vibe/".into(),
                        title: "Vibe".into(),
                        keywords: vec!["vibe".into()],
                        icon: vmux_api::PageIcon::None,
                        shortcut: String::new(),
                        prompt_target: true,
                        startup: false,
                    },
                    CommandBarPage {
                        url: "vmux://sessions/codex/cli".into(),
                        title: "Codex".into(),
                        keywords: vec!["codex".into()],
                        icon: vmux_api::PageIcon::None,
                        shortcut: String::new(),
                        prompt_target: true,
                        startup: false,
                    },
                ],
                spaces_page_url: "vmux://spaces/".into(),
                terminal_page_url: "vmux://terminal/".into(),
                commands: vec![CommandBarCommandEntry {
                    id: "close_tab".into(),
                    name: "Close Tab".into(),
                    shortcut: String::new(),
                }],
                search_engines: vec![SearchEngine::Google],
                ..CommandBarOpenEvent::default()
            }
        }

        fn switching_spaces() -> CommandBarOpenEvent {
            CommandBarOpenEvent {
                picker: Some(CommandBarPicker::Space),
                spaces: vec![
                    CommandBarSpace {
                        id: "space-1".into(),
                        name: "Space 1".into(),
                        profile: "Personal".into(),
                        is_active: false,
                        tab_count: 0,
                    },
                    CommandBarSpace {
                        id: "work".into(),
                        name: "Work".into(),
                        profile: "Personal".into(),
                        is_active: true,
                        tab_count: 3,
                    },
                ],
                ..Self::state()
            }
        }

        fn picking(picker: CommandBarPicker) -> CommandBarOpenEvent {
            let picks = match picker {
                CommandBarPicker::Encoding => vec![
                    vmux_api::command_bar::CommandBarPickRow {
                        label: "Reopen with Encoding".to_string(),
                        pick: CommandBarPick::Picker(CommandBarPicker::EncodingReopen),
                    },
                    vmux_api::command_bar::CommandBarPickRow {
                        label: "Save with Encoding".to_string(),
                        pick: CommandBarPick::Picker(CommandBarPicker::EncodingSave),
                    },
                ],
                CommandBarPicker::EncodingReopen => {
                    let mut rows = Vec::new();
                    for label in ["UTF-8", "Shift_JIS", "EUC-JP"] {
                        rows.push(vmux_api::command_bar::CommandBarPickRow {
                            label: label.to_string(),
                            pick: CommandBarPick::Encoding {
                                label: label.to_string(),
                                save: false,
                            },
                        });
                    }
                    rows
                }
                _ => Vec::new(),
            };
            CommandBarOpenEvent {
                picker: Some(picker),
                picks,
                ..Self::state()
            }
        }

        fn with_open_stack() -> CommandBarOpenEvent {
            CommandBarOpenEvent {
                tabs: vec![CommandBarTab {
                    title: "Docs".into(),
                    url: "vmux://sessions/codex/def".into(),
                    pane_id: 8,
                    tab_index: 1,
                    is_active: false,
                    location: "space-1 / pane 2".into(),
                }],
                ..Self::state()
            }
        }
    }

    struct ExNames;

    impl ExNames {
        fn list(palette: &PaletteState) -> Vec<String> {
            let mut names = Vec::new();
            for row in &palette.rows {
                let CommandBarResultItem::Ex { name, .. } = row else {
                    continue;
                };
                names.push(name.clone());
            }
            names
        }
    }

    impl PaletteState {
        fn start(state: &CommandBarOpenEvent, draft: PaletteDraft) -> Self {
            Self::resolve(state, &draft, PaletteSurface::Start)
        }

        fn modal(state: &CommandBarOpenEvent, draft: PaletteDraft) -> Self {
            Self::resolve(state, &draft, PaletteSurface::Modal)
        }
    }

    #[test]
    fn a_bare_word_keeps_commands_above_the_files_it_also_matched() {
        let hits = FileRows::hits(&["src/settings.rs"]);
        let merged = FileRows::merge(
            "settings",
            Completions::listing(&hits),
            vec![FileRows::a_command()],
        );
        assert!(matches!(merged[0], CommandBarResultItem::Command { .. }));
        assert!(matches!(merged[1], CommandBarResultItem::File { .. }));
    }

    #[test]
    fn a_typed_path_puts_its_files_first() {
        let hits = FileRows::hits(&["src/settings.rs"]);
        let merged = FileRows::merge(
            "~/src",
            Completions::listing(&hits),
            vec![FileRows::a_command()],
        );
        assert!(matches!(merged[0], CommandBarResultItem::File { .. }));
        assert!(matches!(merged[1], CommandBarResultItem::Command { .. }));
    }

    #[test]
    fn an_editor_row_for_an_already_listed_file_is_dropped() {
        let hits = FileRows::hits(&["src/settings.rs"]);
        let merged = FileRows::merge(
            "~/src",
            Completions::listing(&hits),
            vec![CommandBarResultItem::Editor {
                path: "/root/src/settings.rs".to_string(),
            }],
        );
        assert_eq!(merged.len(), 1);
        assert!(matches!(merged[0], CommandBarResultItem::File { .. }));
    }

    #[test]
    fn every_ranked_completion_is_listed_rather_than_the_first_handful() {
        let paths: Vec<String> = (0..40).map(|at| format!("src/main_{at:02}.rs")).collect();
        let named: Vec<&str> = paths.iter().map(String::as_str).collect();
        let hits = FileRows::hits(&named);
        let merged = FileRows::merge("main.rs", Completions::listing(&hits), Vec::new());

        assert_eq!(merged.len(), 40);
        assert!(
            !merged
                .iter()
                .any(|row| matches!(row, CommandBarResultItem::MoreMatches { .. })),
            "nothing was withheld, so the palette must not claim otherwise"
        );
    }

    #[test]
    fn a_withheld_tail_is_counted_in_the_last_row() {
        let state = Launcher::state();
        let palette = PaletteState::modal(
            &state,
            PaletteDraft::typed("main.rs")
                .completing(FileRows::hits(&["src/main.rs", "src/other/main.rs"]))
                .out_of(14),
        );

        assert_eq!(
            palette.rows.last(),
            Some(&CommandBarResultItem::MoreMatches {
                shown: 2,
                total: 14
            })
        );
    }

    #[test]
    fn a_partial_index_owns_up_to_it_below_the_files_it_did_find() {
        let hits = FileRows::hits(&["src/settings.rs"]);
        let merged = FileRows::merge(
            "settings",
            Completions::partial(&hits),
            vec![FileRows::a_command()],
        );

        assert_eq!(merged.last(), Some(&CommandBarResultItem::PartialIndex));
        assert!(matches!(merged[1], CommandBarResultItem::File { .. }));
    }

    #[test]
    fn a_partial_index_that_found_nothing_still_says_why() {
        let merged = FileRows::merge("settings", Completions::partial(&[]), Vec::new());

        assert_eq!(merged, vec![CommandBarResultItem::PartialIndex]);
    }

    #[test]
    fn the_partial_index_notice_does_nothing_and_leaves_the_typed_text_alone() {
        let state = Launcher::state();
        let palette = PaletteState::modal(
            &state,
            PaletteDraft::typed("settings")
                .partially_completing(FileRows::hits(&["src/settings.rs"]))
                .navigating(),
        );
        let at = palette
            .rows
            .iter()
            .position(|row| matches!(row, CommandBarResultItem::PartialIndex))
            .expect("the notice is listed");

        let submission = palette.activate(&palette.rows[at], &[]);
        assert_eq!(submission, PaletteDecision::Close);
        assert_eq!(
            RowText::over(Some(&CommandBarResultItem::PartialIndex), "settings"),
            None
        );
    }

    #[test]
    fn a_complete_index_says_nothing() {
        let hits = FileRows::hits(&["src/settings.rs"]);
        let merged = FileRows::merge(
            "settings",
            Completions::listing(&hits),
            vec![FileRows::a_command()],
        );

        assert!(
            !merged
                .iter()
                .any(|row| matches!(row, CommandBarResultItem::PartialIndex))
        );
    }

    #[test]
    fn a_bare_word_reaches_the_host_but_prose_and_urls_do_not() {
        assert_eq!(
            CompletionQuery::parse("handler").as_deref(),
            Some("handler")
        );
        assert_eq!(
            CompletionQuery::parse("https://example.com").as_deref(),
            None
        );
        assert_eq!(CompletionQuery::parse("file://~/x").as_deref(), Some("~/x"));
    }

    #[test]
    fn several_words_reach_the_host_so_a_path_can_be_narrowed_word_by_word() {
        assert_eq!(
            CompletionQuery::parse("mobile main").as_deref(),
            Some("mobile main")
        );
        assert_eq!(
            CompletionQuery::parse("desktop src/lib").as_deref(),
            Some("desktop src/lib")
        );
    }

    #[test]
    fn a_file_under_a_project_is_shown_against_that_project() {
        let projects = vec!["/code/dashboard".to_string(), "/code".to_string()];
        assert_eq!(
            ProjectPath::split("/code/dashboard/src/main.rs", &projects),
            Some(("dashboard".to_string(), "src/main.rs".to_string())),
            "the longest matching root wins, or a worktree is shown against its parent repo"
        );
        assert_eq!(ProjectPath::split("/elsewhere/main.rs", &projects), None);
    }

    #[test]
    fn the_start_surface_rests_on_open_stacks_and_hides_itself() {
        let mut state = Launcher::with_open_stack();
        state.pages.push(CommandBarPage {
            url: "vmux://start/".into(),
            startup: true,
            ..Default::default()
        });
        state.tabs.push(CommandBarTab {
            title: "Start".into(),
            url: "vmux://start".into(),
            pane_id: 9,
            tab_index: 2,
            is_active: false,
            location: String::new(),
        });

        let resting = PaletteState::start(&state, PaletteDraft::default());
        assert!(
            resting
                .rows
                .iter()
                .all(|row| matches!(row, CommandBarResultItem::Stack { .. })),
            "the empty start surface offers open stacks only: {:?}",
            resting.rows
        );

        let searched = PaletteState::start(&state, PaletteDraft::typed("vmux://"));
        assert!(
            !searched.rows.iter().any(|row| matches!(
                row,
                CommandBarResultItem::Stack { url, .. } | CommandBarResultItem::Page { url, .. }
                    if url.trim_end_matches('/') == "vmux://start"
            )),
            "the start surface never offers itself: {:?}",
            searched.rows
        );
    }

    #[test]
    fn typing_prose_on_start_leads_with_the_chosen_agent() {
        let state = Launcher::state();

        let defaulted = PaletteState::start(&state, PaletteDraft::typed("fix the failing test"));
        assert_eq!(
            PaletteRows::prompt_target_url(&defaulted.rows[0]),
            Some("vmux://sessions/vibe/")
        );

        let chosen = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test").targeting("vmux://sessions/codex/cli"),
        );
        assert_eq!(
            PaletteRows::prompt_target_url(&chosen.rows[0]),
            Some("vmux://sessions/codex/cli")
        );
        assert_eq!(chosen.composer.agent_title, "Codex");
        assert_eq!(chosen.accent_agent.as_deref(), Some("codex"));
    }

    #[test]
    fn the_modal_surface_offers_no_agents_and_no_composer_agent() {
        let state = Launcher::state();
        let bar = PaletteState::modal(&state, PaletteDraft::typed("fix the failing test"));

        assert!(bar.prompt_targets.is_empty());
        assert!(bar.default_target.is_none());
        assert!(!bar.start_prompt_mode);
        assert_eq!(bar.composer.agent_title, "Agent");
    }

    #[test]
    fn a_command_with_nothing_to_show_yet_never_becomes_a_web_search() {
        let state = Launcher::state();
        let mut bar = PaletteState::start(&state, PaletteDraft::typed("/resume"));
        bar.mode = PaletteMode::Slash;
        bar.rows = Vec::new();

        let submitted = bar.submit_start(&[]);

        assert!(
            matches!(submitted, PaletteDecision::None),
            "the list is still loading, so Enter must wait rather than search the web for the command"
        );
    }

    #[test]
    fn navigation_overlays_the_highlighted_row_and_still_edits_the_typed_text() {
        let state = Launcher::state();

        let navigated =
            PaletteState::modal(&state, PaletteDraft::typed("setti").at(0).navigating());
        assert_eq!(navigated.row_text.as_deref(), Some("Settings"));
        assert_eq!(navigated.query, "setti");

        let prompting = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test")
                .at(0)
                .navigating(),
        );
        assert_eq!(prompting.row_text, None);
        assert_eq!(prompting.query, "fix the failing test");

        let path = RowText::over(
            Some(&CommandBarResultItem::File {
                path: "/Users/jun/projects/common/src/lib.rs".into(),
                is_dir: false,
                project: "vmx-198".into(),
                relative: "common/src/lib.rs".into(),
            }),
            "lib.rs",
        );
        assert_eq!(
            path, None,
            "the row already names the file and where it lives, so painting its path over the query only hides what was typed"
        );
    }

    #[test]
    fn the_input_glyph_follows_the_highlighted_row_then_the_typed_shape() {
        let state = Launcher::state();

        assert_eq!(
            PaletteState::modal(&state, PaletteDraft::typed("> close")).glyph,
            Some(PaletteGlyph::Command)
        );
        assert_eq!(
            PaletteState::modal(&state, PaletteDraft::typed("~/src")).glyph,
            Some(PaletteGlyph::Path)
        );
        assert_eq!(
            PaletteState::modal(&state, PaletteDraft::typed("example.com")).glyph,
            Some(PaletteGlyph::Url)
        );
        assert_eq!(
            PaletteState::modal(&state, PaletteDraft::typed("how do i")).glyph,
            Some(PaletteGlyph::Search)
        );

        let navigated =
            PaletteState::modal(&state, PaletteDraft::typed("close").at(0).navigating());
        assert_eq!(
            navigated.glyph,
            Glyph::resolve(navigated.row(0), navigated.mode),
            "navigating reads the row, not the text"
        );
    }

    #[test]
    fn a_picker_shows_no_input_glyph_because_its_chip_already_names_it() {
        assert_eq!(
            Glyph::resolve(None, PaletteMode::Picking(CommandBarPicker::Encoding)),
            None
        );
        assert_eq!(
            Glyph::resolve(None, PaletteMode::Picking(CommandBarPicker::Space)),
            Some(PaletteGlyph::Search),
            "the space switcher is a picker but reads as a search"
        );
    }

    #[test]
    fn a_highlighted_file_wins_over_reading_its_name_as_a_hostname() {
        let row = CommandBarResultItem::File {
            path: "/repo/ts/packages/csp/src/index.ts".into(),
            is_dir: false,
            project: "dashboard".into(),
            relative: "ts/packages/csp/src".into(),
        };

        assert!(TypedRow::beats_a_guessed_url(Some(&row), "index.ts"));
        assert!(TypedRow::beats_a_guessed_url(Some(&row), " Index.TS "));
        assert!(
            !TypedRow::beats_a_guessed_url(Some(&row), "csp"),
            "a partial match is still a search, not an open"
        );
        assert!(
            !TypedRow::beats_a_guessed_url(Some(&row), "https://index.ts"),
            "an explicit scheme means the user typed a URL"
        );
    }

    #[test]
    fn a_highlighted_file_never_hijacks_a_real_domain() {
        let row = CommandBarResultItem::File {
            path: "/repo/docs/google.com".into(),
            is_dir: false,
            project: "dashboard".into(),
            relative: "docs".into(),
        };
        let directory = CommandBarResultItem::File {
            path: "/repo/example.com".into(),
            is_dir: true,
            project: "dashboard".into(),
            relative: "".into(),
        };

        assert!(
            TypedRow::beats_a_guessed_url(Some(&row), "google.com"),
            "a file that really is named google.com is still the highlighted row"
        );
        assert!(
            !TypedRow::beats_a_guessed_url(Some(&directory), "example.com"),
            "a directory is not something Enter opens over a URL"
        );
        assert!(!TypedRow::beats_a_guessed_url(None, "google.com"));
    }

    #[test]
    fn a_colon_offers_the_ex_commands_and_narrows_them_as_the_line_grows() {
        let state = Launcher::state();

        let offered = PaletteState::modal(&state, PaletteDraft::typed(":"));
        let names = ExNames::list(&offered);
        assert_eq!(names.len(), ExLine::COMMANDS.len(), "{names:?}");

        let narrowed = PaletteState::modal(&state, PaletteDraft::typed(":w"));
        assert_eq!(ExNames::list(&narrowed), vec!["w", "wq"]);

        let typed_out = PaletteState::modal(&state, PaletteDraft::typed(":%s/a/b/g"));
        assert!(
            typed_out.rows.is_empty(),
            "a line the catalog cannot complete offers nothing: {:?}",
            typed_out.rows
        );
    }

    #[test]
    fn slash_commands_open_from_the_prefix_and_complete_mcp() {
        let mut state = Launcher::state();
        state.prompt_context.slash_commands = vec![
            vmux_api::chat::SlashCommandEntry {
                command: vmux_api::chat::SlashCommand::Upload,
                description: "Attach files".to_string(),
            },
            vmux_api::chat::SlashCommandEntry {
                command: vmux_api::chat::SlashCommand::Resume,
                description: "Resume a past session".to_string(),
            },
            vmux_api::chat::SlashCommandEntry {
                command: vmux_api::chat::SlashCommand::Mcp,
                description: String::new(),
            },
        ];

        let palette = PaletteState::start(&state, PaletteDraft::typed("/"));
        let names = palette
            .rows
            .iter()
            .filter_map(|row| match row {
                CommandBarResultItem::Slash { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(palette.mode, PaletteMode::Slash);
        assert_eq!(names, ["upload", "resume", "mcp"]);

        let mcp = PaletteState::start(&state, PaletteDraft::typed("/mcp"));
        assert!(matches!(
            mcp.rows.as_slice(),
            [CommandBarResultItem::Slash { name, .. }] if name == "mcp"
        ));
        assert_eq!(
            mcp.submit_start(&[]),
            PaletteDecision::Retype("/mcp ".to_string())
        );
    }

    #[test]
    fn an_ex_line_runs_what_was_typed_unless_a_suggestion_is_highlighted() {
        let state = Launcher::state();

        let typed = PaletteState::modal(&state, PaletteDraft::typed(":noh"));
        assert_eq!(
            typed.submit_modal(&[]),
            PaletteDecision::Ex(ExRequest {
                line: "noh".to_string(),
            })
        );

        let picked = PaletteState::modal(&state, PaletteDraft::typed(":").at(1).navigating());
        assert_eq!(
            picked.submit_modal(&[]),
            PaletteDecision::Ex(ExRequest {
                line: ExLine::COMMANDS[1].name.to_string(),
            }),
            "an empty line still runs the row the user walked to: {:?}",
            picked.rows
        );
    }

    #[test]
    fn an_asserted_picker_outranks_every_shape_the_typed_text_could_take() {
        let asserted = CommandBarPicker::EncodingReopen;
        for typed in [">", ":", "~/etc", "example.com", "how do i", ""] {
            assert_eq!(
                PaletteRows::infer_mode(typed, Some(asserted)),
                PaletteMode::Picking(asserted),
                "`{typed}` must not steal the picker the caller asked for"
            );
        }

        assert_eq!(
            PaletteRows::infer_mode("> close", None),
            PaletteMode::Command
        );
        assert_eq!(PaletteRows::infer_mode(":w", None), PaletteMode::Ex);
        assert_eq!(PaletteRows::infer_mode("~/src", None), PaletteMode::Path);
        assert_eq!(
            PaletteRows::infer_mode("example.com", None),
            PaletteMode::Url
        );
        assert_eq!(
            PaletteRows::infer_mode("how do i", None),
            PaletteMode::Search
        );
    }

    #[test]
    fn a_picker_narrows_its_host_built_rows_and_submits_the_highlighted_one() {
        let state = Launcher::picking(CommandBarPicker::EncodingReopen);

        let offered = PaletteState::modal(&state, PaletteDraft::default());
        assert_eq!(offered.rows.len(), 3, "{:?}", offered.rows);

        let narrowed = PaletteState::modal(&state, PaletteDraft::typed("shift"));
        assert_eq!(narrowed.rows.len(), 1, "{:?}", narrowed.rows);
        assert_eq!(
            narrowed.submit_modal(&[]),
            PaletteDecision::Pick(PickRequest {
                pick: CommandBarPick::Encoding {
                    label: "Shift_JIS".to_string(),
                    save: false,
                },
            })
        );
    }

    #[test]
    fn a_sub_list_row_asks_for_another_picker_rather_than_applying_anything() {
        let state = Launcher::picking(CommandBarPicker::Encoding);
        let palette = PaletteState::modal(&state, PaletteDraft::default());

        assert_eq!(
            palette.submit_modal(&[]),
            PaletteDecision::Pick(PickRequest {
                pick: CommandBarPick::Picker(CommandBarPicker::EncodingReopen),
            })
        );
    }

    #[test]
    fn the_line_picker_reads_the_typed_number_instead_of_a_row() {
        let state = Launcher::picking(CommandBarPicker::GotoLine);

        for (input, line) in [("42", 41), ("  7  ", 6), ("12:5", 11), ("0", 0)] {
            let typed = PaletteState::modal(&state, PaletteDraft::typed(input));
            assert!(typed.rows.is_empty(), "{:?}", typed.rows);
            assert_eq!(
                typed.submit_modal(&[]),
                PaletteDecision::Pick(PickRequest {
                    pick: CommandBarPick::GotoLine { line },
                }),
                "{input}"
            );
        }

        for input in ["", "abc", "-3", "3.5"] {
            let refused = PaletteState::modal(&state, PaletteDraft::typed(input));
            assert_eq!(refused.submit_modal(&[]), PaletteDecision::default());
        }
    }

    #[test]
    fn a_seeded_prefix_is_typed_past_but_a_seeded_url_is_replaced() {
        for seed in [":", ">", "/"] {
            assert!(
                PaletteRows::opens_at_end(seed, None),
                "`{seed}` opens a mode, so the next keystroke must append to it"
            );
        }
        for seed in ["https://example.com", "", ":w"] {
            assert!(
                !PaletteRows::opens_at_end(seed, None),
                "`{seed}` is a value, so the next keystroke must replace it"
            );
        }
    }

    #[test]
    fn the_ghost_completes_a_typed_path_but_never_prose() {
        let state = Launcher::state();
        let hits = FileRows::hits(&["src/main.rs"]);

        let path = PaletteState::start(
            &state,
            PaletteDraft::typed("/root/src").completing(hits.clone()),
        );
        assert_eq!(path.ghost, "/main.rs");

        let prose = PaletteState::start(
            &state,
            PaletteDraft::typed("how do i").completing(hits.clone()),
        );
        assert!(prose.ghost.is_empty());

        let mismatched =
            PaletteState::start(&state, PaletteDraft::typed("/other").completing(hits));
        assert!(mismatched.ghost.is_empty());
    }

    #[test]
    fn selection_clamps_to_the_rows_that_exist() {
        let state = Launcher::state();
        let listed =
            PaletteState::start(&state, PaletteDraft::typed("fix the failing test").at(999));

        assert_eq!(listed.selected, listed.rows.len() - 1);

        let single = PaletteState::modal(&state, PaletteDraft::typed("zzzz").at(4));
        assert_eq!(single.rows.len(), 1, "{:?}", single.rows);
        assert_eq!(single.selected, 0);
    }

    #[test]
    fn arrow_keys_stop_at_both_ends_of_the_list() {
        let state = Launcher::state();
        let rows = PaletteRows::build(
            &state,
            &PaletteDraft::typed("fix the failing test"),
            PaletteSurface::Start,
        );
        let last = rows.items.len() - 1;

        assert_eq!(rows.step(0, MenuDirection::Previous), 0);
        assert_eq!(rows.step(last, MenuDirection::Next), last);
        assert_eq!(rows.step(0, MenuDirection::Next), 1);
    }

    #[test]
    fn a_space_digit_only_lands_on_a_space_row() {
        let state = Launcher::switching_spaces();
        let switching = PaletteState::start(&state, PaletteDraft::default());

        assert_eq!(switching.space_digit(0), Some(0));
        assert_eq!(switching.space_digit(1), Some(1));
        assert_eq!(
            switching.space_digit(2),
            None,
            "the manage-spaces page is not a space: {:?}",
            switching.rows
        );
    }

    #[test]
    fn opening_the_space_switcher_preselects_the_active_space() {
        assert_eq!(
            PaletteState::opening_selection(&Launcher::switching_spaces()),
            1
        );
        assert_eq!(PaletteState::opening_selection(&Launcher::state()), 0);
    }

    #[test]
    fn prose_on_start_prompts_the_agent() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::typed("fix the failing test"));

        let submitted = palette.submit_start(&[]);

        assert_eq!(
            submitted,
            PaletteDecision::Prompt {
                close: true,
                request: PromptRequest {
                    text: "fix the failing test".to_string(),
                    target_url: Some("vmux://sessions/vibe/".to_string()),
                    attachments: Vec::new(),
                },
            }
        );
    }

    #[test]
    fn a_cli_agent_is_prompted() {
        let state = Launcher::state();
        let palette = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test").targeting("vmux://sessions/codex/cli"),
        );

        let submitted = palette.submit_start(&[]);

        assert_eq!(
            submitted,
            PaletteDecision::Prompt {
                close: true,
                request: PromptRequest {
                    text: "fix the failing test".to_string(),
                    target_url: Some("vmux://sessions/codex/cli".to_string()),
                    attachments: Vec::new(),
                },
            }
        );
    }

    #[test]
    fn naming_an_agent_opens_it_instead_of_prompting_it() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::typed("vibe"));

        let submitted = palette.submit_start(&[]);

        assert_eq!(
            submitted,
            PaletteDecision::Open {
                close: true,
                request: OpenRequest {
                    value: "vmux://sessions/vibe/".to_string(),
                    open: palette.open_target,
                },
            }
        );
    }

    #[test]
    fn an_attachment_alone_prompts_the_default_agent() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::default());
        let attached = [ChatAttachment {
            path: "/tmp/a.png".into(),
            name: "a.png".into(),
            mime_type: "image/png".into(),
            size: 12,
            preview_data_url: String::new(),
        }];

        let submitted = palette.submit_start(&attached);

        assert_eq!(
            submitted,
            PaletteDecision::Prompt {
                close: true,
                request: PromptRequest {
                    text: String::new(),
                    target_url: Some("vmux://sessions/vibe/".to_string()),
                    attachments: vec![ChatSubmitAttachment::from(&attached[0])],
                },
            }
        );
    }

    #[test]
    fn an_attachment_with_no_agent_still_reaches_the_host() {
        let state = CommandBarOpenEvent::default();
        let palette = PaletteState::start(&state, PaletteDraft::default());
        let attached = [ChatAttachment {
            path: "/tmp/a.png".into(),
            name: "a.png".into(),
            mime_type: "image/png".into(),
            size: 12,
            preview_data_url: String::new(),
        }];

        let submitted = palette.submit_start(&attached);

        assert_eq!(
            submitted,
            PaletteDecision::Prompt {
                close: false,
                request: PromptRequest {
                    text: String::new(),
                    target_url: None,
                    attachments: vec![ChatSubmitAttachment::from(&attached[0])],
                },
            },
            "the composer keeps its draft on screen"
        );
    }

    #[test]
    fn a_typed_url_opens_in_place_unless_a_matching_page_is_highlighted() {
        let mut state = Launcher::state();
        state.target = Some(OpenTarget::InPlace);

        let typed = PaletteState::modal(&state, PaletteDraft::typed("https://example.com"));
        assert_eq!(
            typed.submit_modal(&[]),
            PaletteDecision::Open {
                close: true,
                request: OpenRequest {
                    value: "https://example.com".to_string(),
                    open: Some(OpenTarget::InPlace),
                },
            }
        );

        let page = PaletteState::modal(&state, PaletteDraft::typed("vmux://settings"));
        let opened = page.submit_modal(&[]);
        assert_eq!(
            opened,
            PaletteDecision::Open {
                close: true,
                request: OpenRequest {
                    value: "vmux://settings/".to_string(),
                    open: Some(OpenTarget::InPlace),
                },
            },
            "the page row wins over the raw text: {:?}",
            page.rows
        );
    }

    #[test]
    fn switching_a_space_sends_the_space_id_from_the_highlighted_row() {
        let state = Launcher::switching_spaces();
        let palette = PaletteState::modal(&state, PaletteDraft::default().at(1));

        assert_eq!(
            palette.submit_modal(&[]),
            PaletteDecision::SwitchSpace(SwitchSpaceRequest {
                id: "work".to_string(),
            })
        );
    }

    #[test]
    fn a_highlighted_stack_switches_tab_rather_than_opening_a_url() {
        let state = Launcher::with_open_stack();
        let palette = PaletteState::start(&state, PaletteDraft::default());

        assert_eq!(
            palette.submit_start(&[]),
            PaletteDecision::SwitchTab(SwitchTabRequest { pane: 8, index: 1 })
        );
    }

    #[test]
    fn a_file_row_opens_through_the_file_scheme() {
        let state = Launcher::state();
        let palette = PaletteState::modal(&state, PaletteDraft::default());

        let opened = palette.activate(
            &CommandBarResultItem::File {
                path: "/work/main.rs".into(),
                is_dir: false,
                project: String::new(),
                relative: String::new(),
            },
            &[],
        );

        assert_eq!(
            opened,
            PaletteDecision::Open {
                close: true,
                request: OpenRequest {
                    value: "file:///work/main.rs".to_string(),
                    open: palette.open_target,
                },
            }
        );
    }

    #[test]
    fn an_empty_navigate_row_closes_without_asking_the_host_for_anything() {
        let state = Launcher::state();
        let palette = PaletteState::modal(&state, PaletteDraft::default());

        let submitted = palette.activate(
            &CommandBarResultItem::Navigate {
                url: String::new(),
                is_url: false,
            },
            &[],
        );

        assert_eq!(submitted, PaletteDecision::Close);
    }

    #[test]
    fn the_send_button_prompts_the_composer_agent_when_no_row_answers_the_text() {
        let state = Launcher::state();
        let palette = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test").targeting("vmux://sessions/codex/cli"),
        );

        assert_eq!(
            palette.submit_start(&[]),
            PaletteDecision::Prompt {
                close: true,
                request: PromptRequest {
                    text: "fix the failing test".to_string(),
                    target_url: Some("vmux://sessions/codex/cli".to_string()),
                    attachments: Vec::new(),
                },
            }
        );
    }

    #[test]
    fn the_send_button_does_nothing_on_an_empty_composer() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::default());
        let empty = PaletteState {
            rows: Vec::new(),
            ..palette
        };

        assert_eq!(empty.submit_start(&[]), PaletteDecision::default());
    }

    #[test]
    fn the_composer_reads_the_model_of_the_targeted_agent_only() {
        let mut state = Launcher::state();
        state.agent_models = vec![AgentModels {
            agent_key: "vibe".into(),
            url: "vmux://sessions/vibe/".into(),
            selected: "big".into(),
            models: vec![
                ModelOptionEntry {
                    id: "big".into(),
                    name: "Big".into(),
                    ..ModelOptionEntry::default()
                },
                ModelOptionEntry {
                    id: "small".into(),
                    name: "Small".into(),
                    ..ModelOptionEntry::default()
                },
            ],
        }];

        let vibe = PaletteState::start(&state, PaletteDraft::typed("fix it"));
        assert_eq!(vibe.composer.model_name, "Big");
        assert_eq!(vibe.composer.model_agent_key, "vibe");
        assert_eq!(vibe.composer.model_options.len(), 2);

        let codex = PaletteState::start(
            &state,
            PaletteDraft::typed("fix it").targeting("vmux://sessions/codex/cli"),
        );
        assert!(codex.composer.model_name.is_empty());
        assert!(codex.composer.model_options.is_empty());
    }

    #[test]
    fn the_composer_matches_permission_modes_across_a_trailing_slash() {
        let mut state = Launcher::state();
        state.agent_modes = vec![AgentModes {
            agent_key: "vibe".into(),
            url: "vmux://sessions/vibe".into(),
            selected: "agent".into(),
            modes: vec![AcpModeOption {
                id: "agent".into(),
                name: "Agent".into(),
                description: None,
            }],
        }];

        let vibe = PaletteState::start(&state, PaletteDraft::typed("fix it"));

        assert_eq!(vibe.composer.permission_agent_key, "vibe");
        assert_eq!(vibe.composer.permission_current_id, "agent");
        assert_eq!(vibe.composer.permission_modes.len(), 1);
    }

    #[test]
    fn the_composer_prefers_the_active_project_over_the_working_directory() {
        let mut state = Launcher::state();
        state.prompt_context = CommandBarPromptContext {
            cwd: "/tmp/scratch".into(),
            workspace_name: "scratch".into(),
            projects: vec![
                ProjectRow {
                    path: "/work/one".into(),
                    label: "one".into(),
                    is_active: false,
                    ..ProjectRow::default()
                },
                ProjectRow {
                    path: "/work/two".into(),
                    label: "two".into(),
                    is_active: true,
                    ..ProjectRow::default()
                },
            ],
            ..CommandBarPromptContext::default()
        };
        let palette = PaletteState::start(&state, PaletteDraft::typed("fix it"));
        assert_eq!(palette.composer.project, "/work/two");
        assert_eq!(palette.composer.workspace_label, "two");
        assert_eq!(
            palette.composer.workspace_title,
            "Choose project · /work/two"
        );

        state.prompt_context = CommandBarPromptContext {
            cwd: "/tmp/scratch".into(),
            ..CommandBarPromptContext::default()
        };
        let unrooted = PaletteState::start(&state, PaletteDraft::typed("fix it"));
        assert_eq!(unrooted.composer.project, "/tmp/scratch");
    }

    #[test]
    fn the_composer_lists_every_agent_the_launcher_knows() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::typed("fix it"));
        let urls: Vec<_> = palette
            .composer
            .agents
            .iter()
            .map(|agent| agent.url.as_str())
            .collect();

        assert_eq!(
            urls,
            vec!["vmux://sessions/vibe/", "vmux://sessions/codex/cli"]
        );
    }
}
