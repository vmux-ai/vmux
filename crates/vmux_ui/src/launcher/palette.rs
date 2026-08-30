use vmux_wire::agent::supports_inline_agent_transition;
use vmux_wire::command_bar::{
    AgentModels, CommandBarActionEvent, CommandBarOpenEvent, CommandBarPick, CommandBarPicker,
    CommandBarPromptContext, CommandBarQuery, ExCommandName, HistoryEntry, PathEntry, is_data_uri,
};
use vmux_wire::open_target::OpenTarget;
use vmux_wire::prompt_media::ChatAttachment;
use vmux_wire::room::ModelOptionEntry;
use vmux_wire::space::ProjectRow;

use crate::components::agent_menu::ComposerAgentOption;
use crate::i18n::translate;
use crate::launcher::results::{
    CommandBarResultItem, PickerRows, active_space_index, filter_results, open_session_results,
    prepend_prompt_targets, prompt_target_matches_query, prompt_target_results, prompt_target_url,
    space_switch_results, start_page_results, terminal_matches_query,
};
use crate::list_nav::MenuDirection;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaletteMode {
    #[default]
    Search,
    Command,
    Ex,
    Path,
    Url,
    Picking(CommandBarPicker),
}

impl PaletteMode {
    pub fn of(query: &str, asserted: Option<CommandBarPicker>) -> Self {
        if let Some(picker) = asserted {
            return Self::Picking(picker);
        }
        if ExLine::claims(query) {
            return Self::Ex;
        }
        let trimmed = query.trim();
        if trimmed.starts_with('>') {
            return Self::Command;
        }
        if trimmed.starts_with('/') || trimmed.starts_with('~') {
            return Self::Path;
        }
        if trimmed.contains("://") || (trimmed.contains('.') && !trimmed.contains(' ')) {
            return Self::Url;
        }
        Self::Search
    }

    pub fn opened(state: &CommandBarOpenEvent) -> Self {
        Self::of(&state.url, state.picker)
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
            Self::Path => "/",
            Self::Search | Self::Url | Self::Picking(_) => "",
        }
    }

    pub fn opens_at_end(self, query: &str) -> bool {
        let prefix = self.prefix();
        !prefix.is_empty() && query == prefix
    }

    pub fn label(self) -> String {
        match self {
            Self::Ex => translate("palette-mode-ex"),
            Self::Command => translate("palette-mode-command"),
            Self::Path => translate("palette-mode-path"),
            Self::Picking(picker) => {
                let id = picker.label();
                if id.is_empty() {
                    String::new()
                } else {
                    translate(id)
                }
            }
            Self::Search | Self::Url => String::new(),
        }
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
}

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
    pub fn of(state: &CommandBarOpenEvent, draft: &PaletteDraft, surface: PaletteSurface) -> Self {
        let query = draft.query.as_str();
        let is_start = surface.is_start();
        let mode = PaletteMode::of(query, state.picker);
        let prompt_targets = if is_start {
            prompt_target_results(&state.pages, "")
        } else {
            Vec::new()
        };
        let default_target = prompt_targets
            .iter()
            .find(|item| prompt_target_url(item) == Some(draft.target_url.as_str()))
            .cloned()
            .or_else(|| prompt_targets.first().cloned());
        let start_prompt_mode = is_start && CommandBarQuery(query).is_start_prompt();

        let mut items = FileRows::under_projects(
            Self::listed(state, draft, surface, mode, start_prompt_mode),
            &state.projects,
        );
        if start_prompt_mode {
            prepend_prompt_targets(&mut items, default_target.as_ref(), &prompt_targets, query);
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

    fn with_completions(
        query: &str,
        draft: &PaletteDraft,
        matched: Vec<CommandBarResultItem>,
    ) -> Vec<CommandBarResultItem> {
        FileRows::merge(query, Completions::of(draft, query), matched)
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
        if let Some(picker) = mode.picking() {
            if picker.is_space() {
                return space_switch_results(&state.spaces, &state.pages, query);
            }
            return PickerRows::of(picker, &state.picks, query);
        }
        if mode.is_ex() {
            if is_start {
                return Vec::new();
            }
            return ExLine::suggestions(query);
        }
        if is_start && query.trim().is_empty() {
            return open_session_results(&state.tabs, &state.pages);
        }
        if start_prompt_mode {
            let matched = start_page_results(
                &state.pages,
                &state.work_dirs,
                &state.recent_files,
                &state.search_engines,
                query,
            );
            return Self::with_completions(query, draft, matched);
        }
        let is_new_tab = matches!(state.target, Some(OpenTarget::InNewStack));
        let matched = filter_results(
            query,
            &state.tabs,
            &state.commands,
            &state.spaces,
            &state.pages,
            is_new_tab,
            &draft.history,
            &state.work_dirs,
            &state.recent_files,
        );
        let matched = Self::with_completions(query, draft, matched);
        if !is_start {
            return matched;
        }
        let mut kept = Vec::with_capacity(matched.len());
        for item in matched {
            let (CommandBarResultItem::Stack { url, .. } | CommandBarResultItem::Page { url, .. }) =
                &item
            else {
                kept.push(item);
                continue;
            };
            if url.trim_end_matches('/') == "vmux://start" {
                continue;
            }
            kept.push(item);
        }
        kept
    }

    fn ghost_of(query: &str, completions: &[PathEntry]) -> String {
        if CompletionQuery::of(query).is_none() {
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

    pub fn completed(&self, query: &str) -> String {
        format!("{query}{}", self.ghost)
    }

    pub fn selected(&self, stored: usize) -> usize {
        stored.min(self.items.len().saturating_sub(1))
    }

    pub fn step(&self, from: usize, direction: MenuDirection) -> usize {
        match direction {
            MenuDirection::Next => (from + 1).min(self.items.len().saturating_sub(1)),
            MenuDirection::Previous => from.saturating_sub(1),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteGlyph {
    Command,
    Path,
    Url,
    Search,
}

impl PaletteGlyph {
    fn of(navigating: Option<&CommandBarResultItem>, mode: PaletteMode) -> Option<Self> {
        if let PaletteMode::Picking(picker) = mode
            && !picker.is_space()
        {
            return None;
        }
        let Some(item) = navigating else {
            return Self::in_mode(mode);
        };
        let glyph = match item {
            CommandBarResultItem::Command { .. }
            | CommandBarResultItem::Ex { .. }
            | CommandBarResultItem::Pick { .. } => Self::Command,
            CommandBarResultItem::Terminal { path } if path.is_empty() => Self::Command,
            CommandBarResultItem::Terminal { .. }
            | CommandBarResultItem::Editor { .. }
            | CommandBarResultItem::File { .. }
            | CommandBarResultItem::WorkDir { .. }
            | CommandBarResultItem::PartialIndex
            | CommandBarResultItem::MoreMatches { .. }
            | CommandBarResultItem::RecentFile { .. } => Self::Path,
            CommandBarResultItem::Stack { .. } | CommandBarResultItem::History { .. } => Self::Url,
            CommandBarResultItem::Navigate { url } => {
                let is_url = url.contains("://") || (url.contains('.') && !url.contains(' '));
                if is_url { Self::Url } else { Self::Search }
            }
            CommandBarResultItem::Space { .. }
            | CommandBarResultItem::Page { .. }
            | CommandBarResultItem::Search { .. } => Self::Search,
        };
        Some(glyph)
    }

    const fn in_mode(mode: PaletteMode) -> Option<Self> {
        match mode {
            PaletteMode::Command | PaletteMode::Ex => Some(Self::Command),
            PaletteMode::Path => Some(Self::Path),
            PaletteMode::Url => Some(Self::Url),
            PaletteMode::Picking(CommandBarPicker::Space) | PaletteMode::Search => {
                Some(Self::Search)
            }
            PaletteMode::Picking(_) => None,
        }
    }
}

pub struct ExLine;

impl ExLine {
    pub fn claims(query: &str) -> bool {
        query.starts_with(':')
    }

    pub fn of(query: &str) -> Option<String> {
        let body = query.strip_prefix(':')?.trim();
        (!body.is_empty()).then(|| body.to_string())
    }

    pub fn suggestions(query: &str) -> Vec<CommandBarResultItem> {
        let typed = query.strip_prefix(':').unwrap_or(query).trim_start();
        let mut rows = Vec::new();
        for entry in ExCommandName::matching(typed) {
            rows.push(CommandBarResultItem::Ex {
                name: entry.name.to_string(),
                hint: translate(entry.hint),
            });
        }
        rows
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComposerState {
    pub loading: bool,
    pub agents: Vec<ComposerAgentOption>,
    pub agent_title: String,
    pub agent_url: String,
    pub model_name: String,
    pub model_options: Vec<ModelOptionEntry>,
    pub model_agent_key: String,
    pub model_current_id: String,
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

impl ComposerState {
    fn of(
        state: &CommandBarOpenEvent,
        prompt_targets: &[CommandBarResultItem],
        effective_target: Option<&CommandBarResultItem>,
    ) -> Self {
        let context = &state.prompt_context;
        let agent_url = effective_target
            .and_then(prompt_target_url)
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
            agents.push(ComposerAgentOption {
                url: url.clone(),
                title: title.clone(),
            });
        }
        let models = SelectedAgentModels::of(&state.agent_models, &agent_url);
        let workspace_label = if context.workspace_name.is_empty() {
            translate("agent-project-select")
        } else {
            context.workspace_name.clone()
        };
        let workspace_title = if context.cwd.is_empty() {
            translate("agent-project-choose")
        } else {
            format!(
                "{} \u{00b7} {}",
                translate("agent-project-choose"),
                context.cwd
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

        Self {
            loading: state.pages.is_empty(),
            agents,
            agent_title,
            agent_url,
            model_name: SelectedAgentModels::name(models),
            model_options: models.map(|row| row.models.clone()).unwrap_or_default(),
            model_agent_key: models.map(|row| row.agent_key.clone()).unwrap_or_default(),
            model_current_id: models.map(|row| row.selected.clone()).unwrap_or_default(),
            workspace_label,
            workspace_title,
            branch_label,
            branch_title,
            worktree_title,
            project: ActiveProject::of(context),
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
pub struct Submission {
    pub close: bool,
    pub action: Option<CommandBarActionEvent>,
    pub inline_target: Option<String>,
}

impl Submission {
    fn closing(action: CommandBarActionEvent) -> Self {
        Self {
            close: true,
            action: Some(action),
            inline_target: None,
        }
    }

    fn silent(action: CommandBarActionEvent) -> Self {
        Self {
            close: false,
            action: Some(action),
            inline_target: None,
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
    pub display_text: String,
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
    pub composer: ComposerState,
}

impl PaletteState {
    pub fn resolve(
        state: &CommandBarOpenEvent,
        draft: &PaletteDraft,
        surface: PaletteSurface,
    ) -> Self {
        Self::of(
            &PaletteRows::of(state, draft, surface),
            state,
            draft,
            surface,
        )
    }

    pub fn of(
        rows: &PaletteRows,
        state: &CommandBarOpenEvent,
        draft: &PaletteDraft,
        surface: PaletteSurface,
    ) -> Self {
        let selected = rows.selected(draft.selected);
        let active = rows.items.get(selected);
        let navigating = if draft.nav_mode { active } else { None };
        let effective_target = active
            .filter(|item| prompt_target_url(item).is_some())
            .or(rows.default_target.as_ref())
            .cloned();
        let accent_agent = AgentSegment::of(if draft.nav_mode {
            active
        } else {
            rows.default_target.as_ref()
        })
        .or_else(|| AgentSegment::of(rows.default_target.as_ref()));
        let display_text = if draft.nav_mode && !rows.start_prompt_mode {
            DisplayText::of(active, &draft.query)
        } else {
            draft.query.clone()
        };

        Self {
            surface,
            query: draft.query.clone(),
            rows: rows.items.clone(),
            selected,
            ghost: rows.ghost.clone(),
            display_text,
            placeholder: Placeholder::of(rows.mode, state, surface),
            glyph: PaletteGlyph::of(navigating, rows.mode),
            mode: rows.mode,
            start_prompt_mode: rows.start_prompt_mode,
            space_switch: rows.mode.is_space(),
            nav_mode: draft.nav_mode,
            open_target: state.target,
            space_name: state.space_name.clone(),
            prompt_targets: rows.prompt_targets.clone(),
            default_target: rows.default_target.clone(),
            composer: ComposerState::of(state, &rows.prompt_targets, effective_target.as_ref()),
            effective_target,
            accent_agent,
        }
    }

    pub fn row(&self, index: usize) -> Option<&CommandBarResultItem> {
        self.rows.get(index)
    }

    pub fn step(&self, direction: MenuDirection) -> usize {
        match direction {
            MenuDirection::Next => (self.selected + 1).min(self.rows.len().saturating_sub(1)),
            MenuDirection::Previous => self.selected.saturating_sub(1),
        }
    }

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
            || prompt_target_matches_query(item, &self.query)
            || (matches!(item, CommandBarResultItem::Terminal { .. })
                && terminal_matches_query(&self.query))
    }

    pub fn activate(
        &self,
        item: &CommandBarResultItem,
        attachments: &[ChatAttachment],
    ) -> Submission {
        let inline_target = self.inline_target(item);
        if self.surface.is_start()
            && (CommandBarQuery(&self.query).is_start_prompt() || !attachments.is_empty())
            && let Some(target_url) = prompt_target_url(item)
        {
            let action = if prompt_target_matches_query(item, &self.query) && attachments.is_empty()
            {
                CommandBarActionEvent::open(target_url, self.open_target)
            } else {
                CommandBarActionEvent::prompt(self.query.trim(), target_url, attachments)
            };
            return Submission {
                close: true,
                action: Some(action),
                inline_target,
            };
        }

        Submission {
            close: true,
            action: self.acted(item),
            inline_target,
        }
    }

    fn inline_target(&self, item: &CommandBarResultItem) -> Option<String> {
        if !self.surface.is_start() {
            return None;
        }
        let url = prompt_target_url(item)?;
        supports_inline_agent_transition(url).then(|| url.to_string())
    }

    fn acted(&self, item: &CommandBarResultItem) -> Option<CommandBarActionEvent> {
        match item {
            CommandBarResultItem::Terminal { path } => Some(CommandBarActionEvent::Terminal {
                value: path.clone(),
            }),
            CommandBarResultItem::Editor { path } | CommandBarResultItem::File { path, .. } => {
                Some(CommandBarActionEvent::open(
                    &format!("file://{path}"),
                    self.open_target,
                ))
            }
            CommandBarResultItem::WorkDir { path, .. } => Some(CommandBarActionEvent::open(
                &format!("file://{path}"),
                self.open_target,
            )),
            CommandBarResultItem::Stack {
                pane_id, tab_index, ..
            } => Some(CommandBarActionEvent::SwitchTab {
                pane: *pane_id,
                index: *tab_index,
            }),
            CommandBarResultItem::Command { id, .. } => Some(CommandBarActionEvent::Command {
                id: id.clone(),
                open: self.open_target,
            }),
            CommandBarResultItem::Ex { name, .. } => {
                Some(CommandBarActionEvent::Ex { line: name.clone() })
            }
            CommandBarResultItem::Pick { pick, .. } => {
                Some(CommandBarActionEvent::Pick { pick: pick.clone() })
            }
            CommandBarResultItem::Space { id, .. } => {
                Some(CommandBarActionEvent::Space { id: id.clone() })
            }
            CommandBarResultItem::Page { url, .. }
            | CommandBarResultItem::Navigate { url }
            | CommandBarResultItem::History { url, .. } => {
                (!url.is_empty()).then(|| CommandBarActionEvent::open(url, self.open_target))
            }
            CommandBarResultItem::RecentFile { url, .. } => {
                Some(CommandBarActionEvent::open(url, self.open_target))
            }
            CommandBarResultItem::Search { engine, query } => Some(CommandBarActionEvent::open(
                &engine.search_url(query),
                self.open_target,
            )),
            CommandBarResultItem::PartialIndex | CommandBarResultItem::MoreMatches { .. } => None,
        }
    }

    pub fn submit_modal(&self, attachments: &[ChatAttachment]) -> Submission {
        if let Some(picker) = self.mode.picking() {
            return self.submit_picked(picker, attachments);
        }
        if self.mode.is_ex() {
            if self.nav_mode
                && let Some(item) = self.row(self.selected)
            {
                return self.activate(item, attachments);
            }
            let Some(line) = ExLine::of(&self.query) else {
                return Submission::default();
            };
            return Submission::closing(CommandBarActionEvent::Ex { line });
        }
        self.submit_typed(attachments)
    }

    fn submit_picked(
        &self,
        picker: CommandBarPicker,
        attachments: &[ChatAttachment],
    ) -> Submission {
        if picker.takes_typed_value() {
            let Some(pick) = CommandBarPick::goto_line(&self.query) else {
                return Submission::default();
            };
            return Submission::closing(CommandBarActionEvent::Pick { pick });
        }
        let Some(item) = self.row(self.selected) else {
            return Submission::default();
        };
        self.activate(item, attachments)
    }

    pub fn submit_start(&self, attachments: &[ChatAttachment]) -> Submission {
        if self.query.trim().is_empty() && !attachments.is_empty() {
            if let Some(item) = self.default_target.as_ref() {
                return self.activate(item, attachments);
            }
            return Submission::silent(CommandBarActionEvent::prompt("", "", attachments));
        }
        if self.space_switch {
            let Some(item) = self.row(self.selected) else {
                return Submission::default();
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
        Submission::closing(CommandBarActionEvent::prompt(
            self.query.trim(),
            "",
            attachments,
        ))
    }

    pub fn submit_action(&self, attachments: &[ChatAttachment]) -> Submission {
        if let Some(item) = self
            .row(self.selected)
            .filter(|item| !self.start_prompt_mode || self.accepts_typed(item))
        {
            return self.activate(item, attachments);
        }
        if self.query.trim().is_empty() && attachments.is_empty() {
            return Submission::default();
        }
        if let Some(item) = self.effective_target.as_ref() {
            return self.activate(item, attachments);
        }
        Submission::closing(CommandBarActionEvent::prompt(
            self.query.trim(),
            "",
            attachments,
        ))
    }

    fn submit_typed(&self, attachments: &[ChatAttachment]) -> Submission {
        if !TypedRow::beats_a_guessed_url(self.row(self.selected), &self.query)
            && CommandBarQuery(&self.query)
                .opens_typed_url_on_enter(self.open_target, self.nav_mode)
        {
            return Submission::closing(CommandBarActionEvent::open(&self.query, self.open_target));
        }
        if let Some(item) = self.row(self.selected) {
            return self.activate(item, attachments);
        }
        if !self.query.is_empty() {
            return Submission::silent(CommandBarActionEvent::open(&self.query, self.open_target));
        }
        Submission::default()
    }

    pub fn opening_selection(state: &CommandBarOpenEvent) -> usize {
        if state.picker == Some(CommandBarPicker::Space) {
            active_space_index(&state.spaces)
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
    fn of(mode: PaletteMode, state: &CommandBarOpenEvent, surface: PaletteSurface) -> String {
        if let Some(picker) = mode.picking() {
            return translate(picker.placeholder());
        }
        if mode.is_ex() {
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

struct DisplayText;

impl DisplayText {
    fn of(item: Option<&CommandBarResultItem>, query: &str) -> String {
        match item {
            Some(CommandBarResultItem::Command { name, .. }) => format!("> {name}"),
            Some(CommandBarResultItem::Ex { name, .. }) => format!(":{name}"),
            Some(CommandBarResultItem::Pick { label, .. }) => label.clone(),
            Some(CommandBarResultItem::Navigate { url }) => url.clone(),
            Some(CommandBarResultItem::Search { query, .. }) => query.clone(),
            Some(CommandBarResultItem::Stack { url, .. }) => url.clone(),
            Some(CommandBarResultItem::Space { name, .. }) => name.clone(),
            Some(CommandBarResultItem::Page { title, .. }) => title.clone(),
            Some(CommandBarResultItem::Terminal { path }) if path.is_empty() => {
                translate("command-terminal")
            }
            Some(CommandBarResultItem::Terminal { path }) => path.clone(),
            Some(CommandBarResultItem::Editor { path }) => path.clone(),
            Some(CommandBarResultItem::History { title, url, .. }) => Self::titled(title, url),
            Some(CommandBarResultItem::File { path, .. }) => path.clone(),
            Some(CommandBarResultItem::WorkDir { path, .. }) => path.clone(),
            Some(CommandBarResultItem::RecentFile { title, url }) => Self::titled(title, url),
            Some(CommandBarResultItem::PartialIndex)
            | Some(CommandBarResultItem::MoreMatches { .. })
            | None => query.to_string(),
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

struct AgentSegment;

impl AgentSegment {
    fn of(item: Option<&CommandBarResultItem>) -> Option<String> {
        let url = prompt_target_url(item?)?;
        let path = url.strip_prefix("vmux://agent/")?;
        let segment = path.split('/').next()?;
        (!segment.is_empty()).then(|| segment.to_string())
    }
}

pub struct ActiveProject;

impl ActiveProject {
    pub fn of(context: &CommandBarPromptContext) -> String {
        for project in &context.projects {
            if project.is_active {
                return project.path.clone();
            }
        }
        context.cwd.clone()
    }
}

pub struct SelectedAgentModels;

impl SelectedAgentModels {
    pub fn of<'a>(rows: &'a [AgentModels], target_url: &str) -> Option<&'a AgentModels> {
        if target_url.is_empty() {
            return None;
        }
        rows.iter().find(|row| row.url == target_url)
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

pub struct CompletionQuery;

impl CompletionQuery {
    pub fn of(input: &str) -> Option<String> {
        let trimmed = input.trim();
        if let Some(rest) = trimmed.strip_prefix("file://") {
            return Some(rest.to_string());
        }
        if Self::looks_like_path(trimmed) {
            return Some(trimmed.to_string());
        }
        if trimmed.is_empty() || trimmed.contains("://") || is_data_uri(trimmed) {
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

    fn looks_like_path(value: &str) -> bool {
        if is_data_uri(value) {
            return false;
        }
        value.starts_with('/')
            || value.starts_with("~/")
            || value.starts_with("./")
            || value.starts_with("../")
            || value.contains('/') && !value.contains(' ') && !value.contains("://")
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Completions<'a> {
    pub entries: &'a [PathEntry],
    pub partial: bool,
    pub total: usize,
}

impl<'a> Completions<'a> {
    pub fn of(draft: &'a PaletteDraft, query: &str) -> Self {
        if CompletionQuery::of(query).is_none() {
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
            CompletionQuery::looks_like_path(trimmed) || CompletionQuery::names_a_file(trimmed);
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
            rest.extend(files);
            rest
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
            let Some((project, relative)) = ProjectPath::of(path, projects) else {
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
    fn of(path: &str, projects: &[String]) -> Option<(String, String)> {
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
    use vmux_wire::command_bar::{
        CommandBarCommandEntry, CommandBarPage, CommandBarSpace, CommandBarTab, SearchEngine,
    };

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
                        host: "settings".into(),
                        url: "vmux://settings/".into(),
                        title: "Settings".into(),
                        keywords: vec!["preferences".into()],
                        icon: vmux_wire::PageIcon::None,
                        shortcut: String::new(),
                        prompt_target: false,
                    },
                    CommandBarPage {
                        host: "agent".into(),
                        url: "vmux://agent/vibe/".into(),
                        title: "Vibe".into(),
                        keywords: vec!["vibe".into()],
                        icon: vmux_wire::PageIcon::None,
                        shortcut: String::new(),
                        prompt_target: true,
                    },
                    CommandBarPage {
                        host: "agent".into(),
                        url: "vmux://agent/codex/cli".into(),
                        title: "Codex".into(),
                        keywords: vec!["codex".into()],
                        icon: vmux_wire::PageIcon::None,
                        shortcut: String::new(),
                        prompt_target: true,
                    },
                ],
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
                    CommandBarPick::Picker(CommandBarPicker::EncodingReopen)
                        .labelled("Reopen with Encoding"),
                    CommandBarPick::Picker(CommandBarPicker::EncodingSave)
                        .labelled("Save with Encoding"),
                ],
                CommandBarPicker::EncodingReopen => {
                    let mut rows = Vec::new();
                    for label in ["UTF-8", "Shift_JIS", "EUC-JP"] {
                        rows.push(
                            CommandBarPick::Encoding {
                                label: label.to_string(),
                                save: false,
                            }
                            .labelled(label),
                        );
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
                    url: "vmux://agent/codex/def".into(),
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
        fn of(palette: &PaletteState) -> Vec<String> {
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
        assert_eq!(submission.action, None);
        assert_eq!(
            DisplayText::of(Some(&CommandBarResultItem::PartialIndex), "settings"),
            "settings"
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
        assert_eq!(CompletionQuery::of("handler").as_deref(), Some("handler"));
        assert_eq!(CompletionQuery::of("https://example.com").as_deref(), None);
        assert_eq!(CompletionQuery::of("file://~/x").as_deref(), Some("~/x"));
    }

    #[test]
    fn several_words_reach_the_host_so_a_path_can_be_narrowed_word_by_word() {
        assert_eq!(
            CompletionQuery::of("mobile main").as_deref(),
            Some("mobile main")
        );
        assert_eq!(
            CompletionQuery::of("desktop src/lib").as_deref(),
            Some("desktop src/lib")
        );
    }

    #[test]
    fn a_file_under_a_project_is_shown_against_that_project() {
        let projects = vec!["/code/dashboard".to_string(), "/code".to_string()];
        assert_eq!(
            ProjectPath::of("/code/dashboard/src/main.rs", &projects),
            Some(("dashboard".to_string(), "src/main.rs".to_string())),
            "the longest matching root wins, or a worktree is shown against its parent repo"
        );
        assert_eq!(ProjectPath::of("/elsewhere/main.rs", &projects), None);
    }

    #[test]
    fn the_start_surface_rests_on_open_stacks_and_hides_itself() {
        let mut state = Launcher::with_open_stack();
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
            prompt_target_url(&defaulted.rows[0]),
            Some("vmux://agent/vibe/")
        );

        let chosen = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test").targeting("vmux://agent/codex/cli"),
        );
        assert_eq!(
            prompt_target_url(&chosen.rows[0]),
            Some("vmux://agent/codex/cli")
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
    fn navigation_shows_the_highlighted_row_but_prose_keeps_the_prompt() {
        let state = Launcher::state();

        let navigated =
            PaletteState::modal(&state, PaletteDraft::typed("setti").at(0).navigating());
        assert_eq!(navigated.display_text, "Settings");

        let prompting = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test")
                .at(0)
                .navigating(),
        );
        assert_eq!(prompting.display_text, "fix the failing test");
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
            PaletteGlyph::of(navigated.row(0), navigated.mode),
            "navigating reads the row, not the text"
        );
    }

    #[test]
    fn a_picker_shows_no_input_glyph_because_its_chip_already_names_it() {
        assert_eq!(
            PaletteGlyph::of(None, PaletteMode::Picking(CommandBarPicker::Encoding)),
            None
        );
        assert_eq!(
            PaletteGlyph::of(None, PaletteMode::Picking(CommandBarPicker::Space)),
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
        let names = ExNames::of(&offered);
        assert_eq!(names.len(), ExCommandName::ALL.len(), "{names:?}");

        let narrowed = PaletteState::modal(&state, PaletteDraft::typed(":w"));
        assert_eq!(ExNames::of(&narrowed), vec!["w", "wq"]);

        let typed_out = PaletteState::modal(&state, PaletteDraft::typed(":%s/a/b/g"));
        assert!(
            typed_out.rows.is_empty(),
            "a line the catalog cannot complete offers nothing: {:?}",
            typed_out.rows
        );
    }

    #[test]
    fn an_ex_line_runs_what_was_typed_unless_a_suggestion_is_highlighted() {
        let state = Launcher::state();

        let typed = PaletteState::modal(&state, PaletteDraft::typed(":noh"));
        assert_eq!(
            typed.submit_modal(&[]).action,
            Some(CommandBarActionEvent::Ex {
                line: "noh".to_string()
            })
        );

        let picked = PaletteState::modal(&state, PaletteDraft::typed(":").at(1).navigating());
        assert_eq!(
            picked.submit_modal(&[]).action,
            Some(CommandBarActionEvent::Ex {
                line: ExCommandName::ALL[1].name.to_string()
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
                PaletteMode::of(typed, Some(asserted)),
                PaletteMode::Picking(asserted),
                "`{typed}` must not steal the picker the caller asked for"
            );
        }

        assert_eq!(PaletteMode::of("> close", None), PaletteMode::Command);
        assert_eq!(PaletteMode::of(":w", None), PaletteMode::Ex);
        assert_eq!(PaletteMode::of("~/src", None), PaletteMode::Path);
        assert_eq!(PaletteMode::of("example.com", None), PaletteMode::Url);
        assert_eq!(PaletteMode::of("how do i", None), PaletteMode::Search);
    }

    #[test]
    fn a_picker_narrows_its_host_built_rows_and_submits_the_highlighted_one() {
        let state = Launcher::picking(CommandBarPicker::EncodingReopen);

        let offered = PaletteState::modal(&state, PaletteDraft::default());
        assert_eq!(offered.rows.len(), 3, "{:?}", offered.rows);

        let narrowed = PaletteState::modal(&state, PaletteDraft::typed("shift"));
        assert_eq!(narrowed.rows.len(), 1, "{:?}", narrowed.rows);
        assert_eq!(
            narrowed.submit_modal(&[]).action,
            Some(CommandBarActionEvent::Pick {
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
            palette.submit_modal(&[]).action,
            Some(CommandBarActionEvent::Pick {
                pick: CommandBarPick::Picker(CommandBarPicker::EncodingReopen),
            })
        );
    }

    #[test]
    fn the_line_picker_reads_the_typed_number_instead_of_a_row() {
        let state = Launcher::picking(CommandBarPicker::GotoLine);

        let typed = PaletteState::modal(&state, PaletteDraft::typed("42"));
        assert!(typed.rows.is_empty(), "{:?}", typed.rows);
        assert_eq!(
            typed.submit_modal(&[]).action,
            Some(CommandBarActionEvent::Pick {
                pick: CommandBarPick::GotoLine { line: 41 },
            })
        );

        let refused = PaletteState::modal(&state, PaletteDraft::typed("abc"));
        assert_eq!(refused.submit_modal(&[]), Submission::default());
    }

    #[test]
    fn a_seeded_prefix_is_typed_past_but_a_seeded_url_is_replaced() {
        for seed in [":", ">", "/"] {
            assert!(
                PaletteMode::of(seed, None).opens_at_end(seed),
                "`{seed}` opens a mode, so the next keystroke must append to it"
            );
        }
        for seed in ["https://example.com", "", ":w"] {
            assert!(
                !PaletteMode::of(seed, None).opens_at_end(seed),
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
        let rows = PaletteRows::of(
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
    fn prose_on_start_prompts_the_agent_and_offers_an_inline_handoff() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::typed("fix the failing test"));

        let submitted = palette.submit_start(&[]);

        assert!(submitted.close);
        assert_eq!(
            submitted.action,
            Some(CommandBarActionEvent::prompt(
                "fix the failing test",
                "vmux://agent/vibe/",
                &[]
            ))
        );
        assert_eq!(
            submitted.inline_target.as_deref(),
            Some("vmux://agent/vibe/")
        );
    }

    #[test]
    fn a_cli_agent_is_prompted_without_an_inline_handoff() {
        let state = Launcher::state();
        let palette = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test").targeting("vmux://agent/codex/cli"),
        );

        let submitted = palette.submit_start(&[]);

        assert_eq!(
            submitted.action,
            Some(CommandBarActionEvent::prompt(
                "fix the failing test",
                "vmux://agent/codex/cli",
                &[]
            ))
        );
        assert_eq!(submitted.inline_target, None);
    }

    #[test]
    fn naming_an_agent_opens_it_instead_of_prompting_it() {
        let state = Launcher::state();
        let palette = PaletteState::start(&state, PaletteDraft::typed("vibe"));

        let submitted = palette.submit_start(&[]);

        assert_eq!(
            submitted.action,
            Some(CommandBarActionEvent::open(
                "vmux://agent/vibe/",
                palette.open_target
            ))
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
            submitted.action,
            Some(CommandBarActionEvent::prompt(
                "",
                "vmux://agent/vibe/",
                &attached
            ))
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

        assert!(!submitted.close, "the composer keeps its draft on screen");
        assert_eq!(
            submitted.action,
            Some(CommandBarActionEvent::prompt("", "", &attached))
        );
    }

    #[test]
    fn a_typed_url_opens_in_place_unless_a_matching_page_is_highlighted() {
        let mut state = Launcher::state();
        state.target = Some(OpenTarget::InPlace);

        let typed = PaletteState::modal(&state, PaletteDraft::typed("https://example.com"));
        assert_eq!(
            typed.submit_modal(&[]).action,
            Some(CommandBarActionEvent::open(
                "https://example.com",
                Some(OpenTarget::InPlace)
            ))
        );

        let page = PaletteState::modal(&state, PaletteDraft::typed("vmux://settings"));
        let opened = page.submit_modal(&[]);
        assert_eq!(
            opened.action,
            Some(CommandBarActionEvent::open(
                "vmux://settings/",
                Some(OpenTarget::InPlace)
            )),
            "the page row wins over the raw text: {:?}",
            page.rows
        );
    }

    #[test]
    fn switching_a_space_sends_the_space_id_from_the_highlighted_row() {
        let state = Launcher::switching_spaces();
        let palette = PaletteState::modal(&state, PaletteDraft::default().at(1));

        assert_eq!(
            palette.submit_modal(&[]).action,
            Some(CommandBarActionEvent::Space {
                id: "work".to_string()
            })
        );
    }

    #[test]
    fn a_highlighted_stack_switches_tab_rather_than_opening_a_url() {
        let state = Launcher::with_open_stack();
        let palette = PaletteState::start(&state, PaletteDraft::default());

        assert_eq!(
            palette.submit_start(&[]).action,
            Some(CommandBarActionEvent::SwitchTab { pane: 8, index: 1 })
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

        assert!(opened.close);
        assert_eq!(
            opened.action,
            Some(CommandBarActionEvent::open(
                "file:///work/main.rs",
                palette.open_target
            ))
        );
    }

    #[test]
    fn an_empty_navigate_row_closes_without_asking_the_host_for_anything() {
        let state = Launcher::state();
        let palette = PaletteState::modal(&state, PaletteDraft::default());

        let submitted =
            palette.activate(&CommandBarResultItem::Navigate { url: String::new() }, &[]);

        assert!(submitted.close);
        assert_eq!(submitted.action, None);
    }

    #[test]
    fn the_send_button_prompts_the_composer_agent_when_no_row_answers_the_text() {
        let state = Launcher::state();
        let palette = PaletteState::start(
            &state,
            PaletteDraft::typed("fix the failing test").targeting("vmux://agent/codex/cli"),
        );

        assert_eq!(
            palette.submit_action(&[]).action,
            Some(CommandBarActionEvent::prompt(
                "fix the failing test",
                "vmux://agent/codex/cli",
                &[]
            ))
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

        assert_eq!(empty.submit_action(&[]), Submission::default());
    }

    #[test]
    fn the_composer_reads_the_model_of_the_targeted_agent_only() {
        let mut state = Launcher::state();
        state.agent_models = vec![AgentModels {
            agent_key: "vibe".into(),
            url: "vmux://agent/vibe/".into(),
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
            PaletteDraft::typed("fix it").targeting("vmux://agent/codex/cli"),
        );
        assert!(codex.composer.model_name.is_empty());
        assert!(codex.composer.model_options.is_empty());
    }

    #[test]
    fn the_composer_prefers_the_active_project_over_the_working_directory() {
        let rooted = CommandBarPromptContext {
            cwd: "/tmp/scratch".into(),
            projects: vec![
                ProjectRow {
                    path: "/work/one".into(),
                    is_active: false,
                    ..ProjectRow::default()
                },
                ProjectRow {
                    path: "/work/two".into(),
                    is_active: true,
                    ..ProjectRow::default()
                },
            ],
            ..CommandBarPromptContext::default()
        };
        assert_eq!(ActiveProject::of(&rooted), "/work/two");

        let unrooted = CommandBarPromptContext {
            cwd: "/tmp/scratch".into(),
            ..CommandBarPromptContext::default()
        };
        assert_eq!(ActiveProject::of(&unrooted), "/tmp/scratch");
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

        assert_eq!(urls, vec!["vmux://agent/vibe/", "vmux://agent/codex/cli"]);
    }
}
