use vmux_api::VmuxRoute;
use vmux_api::command_bar::{
    AgentModels, AgentModes, CommandBarOpenEvent, CommandBarPick, CommandBarPicker,
    CommandPaletteComposer, CommandPaletteProjection, ExRequest, InvokeRequest, OpenRequest,
    PaletteGlyph, PaletteMode, PickRequest, PromptRequest, SwitchTabRequest,
};
use vmux_api::open_target::OpenTarget;
use vmux_api::prompt_media::{ChatAttachment, ChatSubmitAttachment};
use vmux_ui::i18n::translate;

use crate::CommandPaletteSurface;

use super::results::{CommandBarResultItem, PageRows, PickerRows};
use super::{CompletionQuery, Composer, ExLine, Glyph, PaletteDraft, PaletteQuery, PaletteRows};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum PaletteDecision {
    #[default]
    None,
    Close,
    Retype(String),
    Attach(String),
    Prompt {
        close: bool,
        request: PromptRequest,
    },
    Open {
        close: bool,
        request: OpenRequest,
    },
    Invoke(InvokeRequest),
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

    fn invoke(id: String, open: Option<OpenTarget>) -> Self {
        Self::Invoke(InvokeRequest { id, open })
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
            Self::Close | Self::Invoke(_) | Self::SwitchTab(_) | Self::Ex(_) | Self::Pick(_) => {
                true
            }
            Self::None | Self::Retype(_) | Self::Attach(_) => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaletteState {
    pub surface: CommandPaletteSurface,
    pub query: String,
    pub rows: Vec<CommandBarResultItem>,
    pub selected: usize,
    pub ghost: String,
    pub row_text: Option<String>,
    pub placeholder: String,
    pub glyph: Option<PaletteGlyph>,
    pub mode: PaletteMode,
    pub mode_label: String,
    pub picker_typed: bool,
    pub start_prompt_mode: bool,
    pub numbered: bool,
    pub nav_mode: bool,
    pub open_target: Option<OpenTarget>,
    pub context_label: String,
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
        surface: CommandPaletteSurface,
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
        surface: CommandPaletteSurface,
    ) -> Self {
        let selected = rows.selected(draft.selected);
        let active = rows.items.get(selected);
        let navigating = if draft.nav_mode { active } else { None };
        let effective_target = active
            .filter(|item| PageRows::prompt_target_url(item).is_some())
            .or(rows.default_target.as_ref())
            .cloned();
        let composer = Composer::build(state, &rows.prompt_targets, effective_target.as_ref());
        let accent_agent = [
            composer.model_agent_key.as_str(),
            composer.permission_agent_key.as_str(),
        ]
        .into_iter()
        .find(|key| !key.is_empty())
        .map(str::to_string)
        .or_else(|| {
            let route = VmuxRoute::parse(&composer.agent_url)?;
            url::form_urlencoded::parse(route.query()?.as_bytes()).find_map(|(key, value)| {
                (key == "agent" && !value.is_empty()).then(|| value.into_owned())
            })
        });
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
            placeholder: Placeholder::resolve(&rows.mode, state, surface),
            glyph: Glyph::resolve(navigating, &rows.mode, rows.numbered),
            mode: rows.mode.clone(),
            mode_label: state.picker_label.clone(),
            picker_typed: state.picker_typed,
            start_prompt_mode: rows.start_prompt_mode,
            numbered: rows.numbered,
            nav_mode: draft.nav_mode,
            open_target: state.target,
            context_label: state.context_label.clone(),
            prompt_targets: rows.prompt_targets.clone(),
            default_target: rows.default_target.clone(),
            composer,
            effective_target,
            accent_agent,
        }
    }

    pub fn row(&self, index: usize) -> Option<&CommandBarResultItem> {
        self.rows.get(index)
    }

    pub fn projection(&self) -> CommandPaletteProjection {
        CommandPaletteProjection {
            query: self.query.clone(),
            rows: self
                .rows
                .iter()
                .map(CommandBarResultItem::projection)
                .collect(),
            selected: self.selected as u32,
            navigating: self.nav_mode,
            row_text: self.row_text.clone(),
            placeholder: self.placeholder.clone(),
            glyph: self.glyph,
            numbered: self.numbered,
            numbered_count: 0,
            context_label: self.context_label.clone(),
            accent_agent: self.accent_agent.clone(),
            composer: self.composer.clone(),
            prompt_targets: self
                .prompt_targets
                .iter()
                .map(CommandBarResultItem::projection)
                .collect(),
            default_target: self
                .default_target
                .as_ref()
                .map(CommandBarResultItem::projection),
            ghost: self.ghost.clone(),
            start_prompt_mode: self.start_prompt_mode,
            mode: self.mode.clone(),
            mode_label: self.mode_label.clone(),
            ..Default::default()
        }
    }

    pub fn accepts_typed(&self, item: &CommandBarResultItem) -> bool {
        self.nav_mode || PageRows::prompt_target_matches(item, &self.query)
    }

    pub fn activate(
        &self,
        item: &CommandBarResultItem,
        attachments: &[ChatAttachment],
    ) -> PaletteDecision {
        if self.surface.is_start()
            && CompletionQuery::only_files(&self.query)
            && let CommandBarResultItem::File { path, is_dir, .. } = item
        {
            if *is_dir {
                return PaletteDecision::retyping(format!("@{path}"));
            }
            return PaletteDecision::Attach(path.clone());
        }
        if (PaletteQuery::new(&self.query).is_prompt() || !attachments.is_empty())
            && let Some(target_url) = PageRows::prompt_target_url(item)
        {
            return if PageRows::prompt_target_matches(item, &self.query) && attachments.is_empty() {
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
            CommandBarResultItem::File { path, .. } => Some(PaletteDecision::open(
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
            CommandBarResultItem::Pick { pick, .. } => Some(PaletteDecision::pick(pick.clone())),
            CommandBarResultItem::Page { url, .. }
            | CommandBarResultItem::Navigate { url, .. }
            | CommandBarResultItem::History { url, .. } => {
                (!url.is_empty()).then(|| PaletteDecision::open(true, url, self.open_target))
            }
            CommandBarResultItem::Search { engine, query } => Some(PaletteDecision::open(
                true,
                &engine.query_url(query),
                self.open_target,
            )),
            CommandBarResultItem::PartialIndex | CommandBarResultItem::MoreMatches { .. } => None,
        }
    }

    pub fn submit_modal(&self, attachments: &[ChatAttachment]) -> PaletteDecision {
        if let PaletteMode::Picking(picker) = &self.mode {
            return self.submit_picked(picker, attachments);
        }
        if matches!(self.mode, PaletteMode::Ex) {
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
        picker: &CommandBarPicker,
        attachments: &[ChatAttachment],
    ) -> PaletteDecision {
        if self.picker_typed {
            return PaletteDecision::pick(PickerRows::typed(picker.clone(), &self.query));
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
        if self.numbered {
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
        if CompletionQuery::only_files(&self.query) {
            return PaletteDecision::default();
        }
        if !self.query.is_empty() {
            return PaletteDecision::open(false, &self.query, self.open_target);
        }
        PaletteDecision::default()
    }
}

pub(super) struct TypedRow;

impl TypedRow {
    pub fn beats_a_guessed_url(row: Option<&CommandBarResultItem>, query: &str) -> bool {
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
    fn resolve(
        mode: &PaletteMode,
        state: &CommandBarOpenEvent,
        surface: CommandPaletteSurface,
    ) -> String {
        if matches!(mode, PaletteMode::Picking(_)) {
            return state.picker_placeholder.clone();
        }
        if matches!(mode, PaletteMode::Ex) {
            return translate("command-ex-placeholder");
        }
        match surface {
            CommandPaletteSurface::Start => translate("command-search-ask"),
            CommandPaletteSurface::Modal => {
                if matches!(state.target, Some(OpenTarget::InNewStack)) {
                    translate("command-new-tab-placeholder")
                } else {
                    translate("command-placeholder")
                }
            }
        }
    }
}

pub(super) struct RowText;

impl RowText {
    pub fn over(item: Option<&CommandBarResultItem>, query: &str) -> Option<String> {
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
        matches!(item, CommandBarResultItem::File { .. })
    }

    fn resolve(item: &CommandBarResultItem, query: &str) -> String {
        match item {
            CommandBarResultItem::Command { name, .. } => format!("> {name}"),
            CommandBarResultItem::Slash { name, .. } => format!("/{name} "),
            CommandBarResultItem::Pick { label, .. } => label.clone(),
            CommandBarResultItem::Navigate { url, .. } => url.clone(),
            CommandBarResultItem::Search { query, .. } => query.clone(),
            CommandBarResultItem::Stack { url, .. } => url.clone(),
            CommandBarResultItem::Page { title, .. } => title.clone(),
            CommandBarResultItem::History { title, url, .. } => Self::titled(title, url),
            CommandBarResultItem::File { path, .. } => path.clone(),
            CommandBarResultItem::PartialIndex | CommandBarResultItem::MoreMatches { .. } => {
                query.to_string()
            }
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

pub(super) struct SelectedAgentModels;

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

pub(super) struct SelectedAgentModes;

impl SelectedAgentModes {
    pub fn find<'a>(rows: &'a [AgentModes], target_url: &str) -> Option<&'a AgentModes> {
        if target_url.is_empty() {
            return None;
        }
        rows.iter()
            .find(|row| AgentCatalogUrl::matches(&row.url, target_url))
    }

    pub fn name(row: Option<&AgentModes>) -> String {
        let Some(row) = row else {
            return String::new();
        };
        row.modes
            .iter()
            .find(|mode| mode.id == row.selected)
            .map(|mode| mode.name.clone())
            .unwrap_or_else(|| row.selected.clone())
    }

    pub fn title(row: Option<&AgentModes>) -> String {
        row.and_then(|row| row.modes.iter().find(|mode| mode.id == row.selected))
            .and_then(|mode| mode.description.clone())
            .filter(|description| !description.is_empty())
            .unwrap_or_else(|| translate("composer-permission-change"))
    }
}
