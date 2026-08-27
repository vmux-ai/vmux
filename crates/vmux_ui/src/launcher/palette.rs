use vmux_wire::agent::supports_inline_agent_transition;
use vmux_wire::command_bar::{
    AgentModels, CommandBarActionEvent, CommandBarOpenEvent, CommandBarPromptContext,
    CommandBarQuery, HistoryEntry, PathEntry, is_data_uri,
};
use vmux_wire::open_target::OpenTarget;
use vmux_wire::prompt_media::ChatAttachment;
use vmux_wire::room::ModelOptionEntry;
use vmux_wire::space::ProjectRow;

use crate::components::agent_menu::ComposerAgentOption;
use crate::i18n::translate;
use crate::launcher::results::{
    CommandBarResultItem, active_space_index, filter_results, open_session_results,
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

#[derive(Clone, Debug, Default)]
pub struct PaletteDraft {
    pub query: String,
    pub selected: usize,
    pub nav_mode: bool,
    pub target_url: String,
    pub completions: Vec<PathEntry>,
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
        self.completions = completions;
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
}

impl PaletteRows {
    pub fn of(state: &CommandBarOpenEvent, draft: &PaletteDraft, surface: PaletteSurface) -> Self {
        let query = draft.query.as_str();
        let is_start = surface.is_start();
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

        let mut items = Self::listed(state, draft, surface, start_prompt_mode);
        if start_prompt_mode {
            prepend_prompt_targets(&mut items, default_target.as_ref(), &prompt_targets, query);
        }

        Self {
            items,
            prompt_targets,
            default_target,
            ghost: Self::ghost_of(query, &draft.completions),
            start_prompt_mode,
        }
    }

    fn listed(
        state: &CommandBarOpenEvent,
        draft: &PaletteDraft,
        surface: PaletteSurface,
        start_prompt_mode: bool,
    ) -> Vec<CommandBarResultItem> {
        let query = draft.query.as_str();
        let is_start = surface.is_start();
        if state.space_switch {
            return space_switch_results(&state.spaces, &state.pages, query);
        }
        if is_start && query.trim().is_empty() {
            return open_session_results(&state.tabs, &state.pages);
        }
        if start_prompt_mode {
            return start_page_results(
                &state.pages,
                &state.work_dirs,
                &state.recent_files,
                &state.search_engines,
                query,
            );
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
        let completions: &[PathEntry] = if CompletionQuery::of(query).is_some() {
            &draft.completions
        } else {
            &[]
        };
        let matched = FileRows::merge(query, completions, matched);
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
    fn of(navigating: Option<&CommandBarResultItem>, query: &str) -> Self {
        let Some(item) = navigating else {
            return Self::typed(query);
        };
        match item {
            CommandBarResultItem::Command { .. } => Self::Command,
            CommandBarResultItem::Terminal { path } if path.is_empty() => Self::Command,
            CommandBarResultItem::Terminal { .. }
            | CommandBarResultItem::Editor { .. }
            | CommandBarResultItem::File { .. }
            | CommandBarResultItem::WorkDir { .. }
            | CommandBarResultItem::RecentFile { .. } => Self::Path,
            CommandBarResultItem::Stack { .. } | CommandBarResultItem::History { .. } => Self::Url,
            CommandBarResultItem::Navigate { url } => {
                let is_url = url.contains("://") || (url.contains('.') && !url.contains(' '));
                if is_url { Self::Url } else { Self::Search }
            }
            CommandBarResultItem::Space { .. }
            | CommandBarResultItem::Page { .. }
            | CommandBarResultItem::Search { .. } => Self::Search,
        }
    }

    fn typed(query: &str) -> Self {
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
    pub glyph: PaletteGlyph,
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
            placeholder: Placeholder::of(state, surface),
            glyph: PaletteGlyph::of(navigating, &draft.query),
            start_prompt_mode: rows.start_prompt_mode,
            space_switch: state.space_switch,
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
        }
    }

    pub fn submit_modal(&self, attachments: &[ChatAttachment]) -> Submission {
        if self.space_switch {
            let Some(item) = self.row(self.selected) else {
                return Submission::default();
            };
            return self.activate(item, attachments);
        }
        self.submit_typed(attachments)
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
        let trimmed = self.query.trim();
        let prefer_page = matches!(
            self.row(self.selected),
            Some(CommandBarResultItem::Page { url, .. })
                if trimmed.starts_with("vmux://") && url.starts_with(trimmed)
        );
        if !prefer_page
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
        if state.space_switch {
            active_space_index(&state.spaces)
        } else {
            0
        }
    }
}

struct Placeholder;

impl Placeholder {
    fn of(state: &CommandBarOpenEvent, surface: PaletteSurface) -> String {
        if state.space_switch {
            return translate("command-switch-space");
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
            None => query.to_string(),
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
        if trimmed.is_empty()
            || trimmed.contains(' ')
            || trimmed.contains("://")
            || is_data_uri(trimmed)
        {
            return None;
        }
        Some(trimmed.to_string())
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

pub struct FileRows;

impl FileRows {
    const LEADING: usize = 8;
    const TRAILING: usize = 5;

    pub fn merge(
        query: &str,
        completions: &[PathEntry],
        matched: Vec<CommandBarResultItem>,
    ) -> Vec<CommandBarResultItem> {
        if completions.is_empty() {
            return matched;
        }
        let leads = CompletionQuery::looks_like_path(query.trim());
        let take = if leads { Self::LEADING } else { Self::TRAILING };
        let mut files = Vec::with_capacity(take);
        let mut listed = Vec::with_capacity(take);
        for entry in completions.iter().take(take) {
            files.push(CommandBarResultItem::File {
                path: entry.full_path.clone(),
                is_dir: entry.is_dir,
            });
            listed.push(entry.full_path.as_str());
        }
        let mut rest = Vec::with_capacity(matched.len());
        for item in matched {
            if let CommandBarResultItem::Editor { path } = &item
                && listed.contains(&path.as_str())
            {
                continue;
            }
            rest.push(item);
        }
        if leads {
            files.extend(rest);
            return files;
        }
        rest.extend(files);
        rest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_wire::command_bar::{
        CommandBarCommandEntry, CommandBarPage, CommandBarSpace, CommandBarTab, SearchEngine,
    };

    impl FileRows {
        fn hits(paths: &[&str]) -> Vec<PathEntry> {
            let mut entries = Vec::new();
            for path in paths {
                entries.push(PathEntry {
                    name: (*path).to_string(),
                    is_dir: false,
                    full_path: format!("/root/{path}"),
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
                space_switch: true,
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
        let merged = FileRows::merge(
            "settings",
            &FileRows::hits(&["src/settings.rs"]),
            vec![FileRows::a_command()],
        );
        assert!(matches!(merged[0], CommandBarResultItem::Command { .. }));
        assert!(matches!(merged[1], CommandBarResultItem::File { .. }));
    }

    #[test]
    fn a_typed_path_puts_its_files_first() {
        let merged = FileRows::merge(
            "~/src",
            &FileRows::hits(&["src/settings.rs"]),
            vec![FileRows::a_command()],
        );
        assert!(matches!(merged[0], CommandBarResultItem::File { .. }));
        assert!(matches!(merged[1], CommandBarResultItem::Command { .. }));
    }

    #[test]
    fn an_editor_row_for_an_already_listed_file_is_dropped() {
        let merged = FileRows::merge(
            "~/src",
            &FileRows::hits(&["src/settings.rs"]),
            vec![CommandBarResultItem::Editor {
                path: "/root/src/settings.rs".to_string(),
            }],
        );
        assert_eq!(merged.len(), 1);
        assert!(matches!(merged[0], CommandBarResultItem::File { .. }));
    }

    #[test]
    fn a_bare_word_reaches_the_host_but_prose_and_urls_do_not() {
        assert_eq!(CompletionQuery::of("handler").as_deref(), Some("handler"));
        assert_eq!(CompletionQuery::of("how do i").as_deref(), None);
        assert_eq!(CompletionQuery::of("https://example.com").as_deref(), None);
        assert_eq!(CompletionQuery::of("file://~/x").as_deref(), Some("~/x"));
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
            PaletteGlyph::Command
        );
        assert_eq!(
            PaletteState::modal(&state, PaletteDraft::typed("~/src")).glyph,
            PaletteGlyph::Path
        );
        assert_eq!(
            PaletteState::modal(&state, PaletteDraft::typed("example.com")).glyph,
            PaletteGlyph::Url
        );
        assert_eq!(
            PaletteState::modal(&state, PaletteDraft::typed("how do i")).glyph,
            PaletteGlyph::Search
        );

        let navigated =
            PaletteState::modal(&state, PaletteDraft::typed("close").at(0).navigating());
        assert_eq!(
            navigated.glyph,
            PaletteGlyph::of(navigated.row(0), "close"),
            "navigating reads the row, not the text"
        );
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
