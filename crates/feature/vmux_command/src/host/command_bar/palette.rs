use self::menu::{
    AgentMenuOpen, BranchMenuOpen, ModelMenuOpen, PaletteMenuCursor, PermissionMenuOpen,
    ProjectMenuOpen,
};
use self::request_driver::{OpenVersion, RequestDelay, RequestGeneration};
use self::rows::{PaletteRow, PaletteRowKind, PaletteRowsSnapshot};
use super::CommandBarDismiss;
use crate::{CommandDispatch, CommandPaletteSurface, CommandRuntimePlugin, ResolvedLocale};
use bevy::prelude::*;
use bevy::prelude::{Changed, Commands};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use std::time::{Duration, Instant};
use vmux_api::command_bar::{
    CommandBarOpenEvent, CommandBarPick, CommandBarUiState, CommandBarUiStatePatch,
    CommandPaletteActivateRequest, CommandPaletteDraftRequest, CommandPaletteHighlightRequest,
    CommandPaletteHistoryMoveRequest, CommandPaletteProjection,
    CommandPaletteRemoveAttachmentRequest, CommandPaletteSubmitRequest, CommandPaletteUiState,
    ExRequest, HistoryEntry, InvokeRequest, OpenId, OpenRequest, PaletteMode, PathEntry,
    PickRequest, PromptRequest, SwitchTabRequest,
};
use vmux_api::prompt_media::{
    ChatAttachPaths, ChatAttachment, ChatMediaEntry, ChatSubmitAttachment,
    PromptComposerAttachment, PromptMediaOption,
};
use vmux_api::space::ProjectBranch;
use vmux_ecs::UiStateWrite;
use vmux_ecs::launcher::{
    CommandBarContribution, CommandBarContributionActivated, CommandBarQueryChanged, HostsLauncher,
    RendersLauncherPanel,
};
#[cfg(test)]
use vmux_feature::manifest::FeaturePlugin;

mod branch;
mod media;
mod menu;
mod prompt;
mod request_driver;
mod rows;
mod search;

pub(super) struct PalettePlugin;

impl Plugin for PalettePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_plugins(UiEventPlugin::<(
            CommandPaletteDraftRequest,
            CommandPaletteHighlightRequest,
            CommandPaletteHistoryMoveRequest,
            CommandPaletteSubmitRequest,
            CommandPaletteActivateRequest,
            CommandPaletteRemoveAttachmentRequest,
        )>::default())
            .add_plugins((
                menu::MenuPlugin,
                search::PaletteSearchPlugin,
                prompt::PalettePromptPlugin,
                branch::PaletteBranchPlugin,
                media::PaletteMediaPlugin,
            ))
            .add_observer(open)
            .add_observer(update)
            .add_observer(highlight)
            .add_observer(history)
            .add_observer(submit_input)
            .add_observer(activate)
            .add_observer(next)
            .add_observer(previous)
            .add_observer(complete)
            .add_observer(dismiss)
            .add_observer(submit)
            .add_observer(attach_launcher)
            .add_observer(attach_panel)
            .add_systems(PreUpdate, (attach, detach))
            .add_systems(Update, settle_results)
            .add_systems(
                PostUpdate,
                (query, queue_results, rows, project, publish).chain(),
            )
            .add_systems(Last, wake);
    }
}

#[derive(Component, Default)]
#[require(
    PaletteOpen,
    PaletteContext,
    PaletteDraftInput,
    PublishedQuery,
    PaletteRowsSnapshot,
    PaletteContributionRows
)]
struct PaletteSnapshot {
    open_id: OpenId,
    projection: CommandPaletteProjection,
    completions: Vec<PathEntry>,
    completions_partial: bool,
    completions_total: u32,
    history: Vec<HistoryEntry>,
    prompt_history: Vec<String>,
    branch_project: String,
    branches: Vec<ProjectBranch>,
    media_query: Option<String>,
    media_entries: Vec<ChatMediaEntry>,
    media_options: Vec<PromptMediaOption>,
    media_loading: bool,
    media_selected: u32,
    attachments: Vec<ChatAttachment>,
    composer_attachments: Vec<PromptComposerAttachment>,
    attachment_sequence: u64,
}

#[derive(Component, Default)]
pub(super) struct PaletteOpen(pub(super) CommandBarOpenEvent);

#[derive(Component, Clone, Default, PartialEq, Eq)]
struct PaletteContext {
    open_id: OpenId,
    agent: String,
    cwd: String,
    project: String,
}

#[derive(Component, Default)]
struct PaletteDraftInput {
    initialized: bool,
    open_id: OpenId,
    query: String,
    surface: CommandPaletteSurface,
    target_url: String,
    selected: usize,
    navigating: bool,
    input_revision: u64,
    close_revision: u64,
    history_cursor: Option<usize>,
    history_scratch: String,
    result_query: String,
    result_pending: Option<String>,
    result_due: Option<Instant>,
}

impl PaletteDraftInput {
    const START_RESULTS_DEBOUNCE: Duration = Duration::from_millis(120);

    fn replace_query(&mut self, query: &str) {
        self.query.clear();
        self.query.push_str(query);
        self.selected = 0;
        self.navigating = false;
        self.input_revision = self.input_revision.wrapping_add(1).max(1);
    }

    fn bump_close_revision(&mut self) {
        self.close_revision = self.close_revision.wrapping_add(1).max(1);
    }

    fn synchronize_results(&mut self, now: Instant) {
        if !self.surface.is_start() {
            self.result_query.clone_from(&self.query);
            self.result_pending = None;
            self.result_due = None;
            return;
        }
        if self.result_query == self.query {
            self.result_pending = None;
            self.result_due = None;
            return;
        }
        if self.result_pending.as_ref() == Some(&self.query) {
            return;
        }
        self.result_pending = Some(self.query.clone());
        self.result_due = Some(now + Self::START_RESULTS_DEBOUNCE);
    }

    fn settle_results(&mut self, now: Instant) -> bool {
        let Some(due) = self.result_due else {
            return false;
        };
        if now < due {
            return false;
        }
        let Some(query) = self.result_pending.take() else {
            self.result_due = None;
            return false;
        };
        self.result_query = query;
        self.result_due = None;
        true
    }

    fn prompt_request(
        text: &str,
        target_url: &str,
        attachments: &[ChatAttachment],
    ) -> PromptRequest {
        let mut submitted = Vec::with_capacity(attachments.len());
        for attachment in attachments {
            submitted.push(ChatSubmitAttachment::from(attachment));
        }
        PromptRequest {
            text: text.to_string(),
            target_url: (!target_url.is_empty()).then(|| target_url.to_string()),
            attachments: submitted,
        }
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
struct PublishedQuery {
    open_id: OpenId,
    query: String,
    start: bool,
    open: Option<vmux_api::open_target::OpenTarget>,
    picker: Option<vmux_api::command_bar::CommandBarPicker>,
    numbered: bool,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
struct PaletteContributionRows(Vec<Entity>);

impl PaletteContributionRows {
    fn append_matching(
        &mut self,
        contributed: &mut Vec<(Entity, &CommandBarContribution)>,
        projection: &mut CommandPaletteProjection,
        locale: Option<&ResolvedLocale>,
    ) -> Option<usize> {
        contributed.sort_by(|(left_entity, left), (right_entity, right)| {
            left.rank
                .cmp(&right.rank)
                .then_with(|| left.row.title.cmp(&right.row.title))
                .then_with(|| left_entity.cmp(right_entity))
        });
        let mut entities = Vec::with_capacity(contributed.len());
        let mut preferred = None;
        let mut numbered_count = 0;
        for (entity, contribution) in contributed.iter() {
            if contribution.preferred && preferred.is_none() {
                preferred = Some(projection.rows.len());
            }
            if contribution.numbered {
                numbered_count += 1;
            }
            entities.push(*entity);
            let mut row = contribution.row.clone();
            if !contribution.title_message_id.is_empty() {
                row.title = locale.map_or_else(
                    || vmux_ui::i18n::translate(&contribution.title_message_id),
                    |locale| locale.0.translate(&contribution.title_message_id),
                );
            }
            if !contribution.subtitle_message_id.is_empty() {
                row.subtitle = locale.map_or_else(
                    || vmux_ui::i18n::translate(&contribution.subtitle_message_id),
                    |locale| locale.0.translate(&contribution.subtitle_message_id),
                );
            }
            projection.rows.push(row);
        }
        projection.numbered_count = numbered_count;
        if self.0 != entities {
            self.0 = entities;
        }
        preferred
    }
}

#[cfg(test)]
#[derive(Resource, Default)]
struct ProjectionBuilds(u32);

#[crate::command]
struct CommandBarNextBinding;

#[crate::command]
struct CommandBarPreviousBinding;

#[crate::command]
struct CommandBarCompleteBinding;

#[crate::command]
struct CommandBarDismissBinding;

#[crate::command]
struct CommandBarSubmitBinding;

type NewPalettePages<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
        Without<PaletteSnapshot>,
    ),
>;

type DetachedPalettePages<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<PaletteSnapshot>,
        Without<RendersLauncherPanel>,
        Without<HostsLauncher>,
    ),
>;

fn attach(pages: NewPalettePages, mut commands: Commands) {
    for page in &pages {
        commands.entity(page).insert(PaletteSnapshot::default());
    }
}

fn attach_launcher(
    trigger: On<Add, HostsLauncher>,
    existing: Query<(), With<PaletteSnapshot>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    if existing.contains(entity) {
        return;
    }
    commands.entity(entity).insert(PaletteSnapshot::default());
}

fn attach_panel(
    trigger: On<Add, RendersLauncherPanel>,
    existing: Query<(), With<PaletteSnapshot>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    if existing.contains(entity) {
        return;
    }
    commands.entity(entity).insert(PaletteSnapshot::default());
}

fn open(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(
        &mut PaletteOpen,
        &mut PaletteDraftInput,
        &mut PaletteSnapshot,
        Has<HostsLauncher>,
    )>,
    mut commands: Commands,
) {
    let Some(opened) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<CommandBarOpenEvent>>::payload(
            trigger.event().update(),
        )
    else {
        return;
    };
    let webview = trigger.event().webview();
    let Ok((mut current, mut draft, mut snapshot, start)) = palettes.get_mut(webview) else {
        return;
    };
    current.0.clone_from(opened);
    if draft.initialized && draft.open_id == opened.open_id {
        commands.trigger(UiInput {
            webview,
            payload: CommandPaletteDraftRequest {
                open_id: opened.open_id,
                query: draft.query.clone(),
                start,
            },
        });
        return;
    }
    draft.initialized = true;
    draft.open_id = opened.open_id;
    draft.query.clone_from(&opened.url);
    draft.result_query.clone_from(&opened.url);
    draft.result_pending = None;
    draft.result_due = None;
    draft.target_url.clear();
    draft.selected = 0;
    draft.navigating = false;
    draft.history_cursor = None;
    draft.history_scratch.clear();
    commands.entity(webview).remove::<(
        AgentMenuOpen,
        ModelMenuOpen,
        PermissionMenuOpen,
        ProjectMenuOpen,
        BranchMenuOpen,
        PaletteMenuCursor,
    )>();
    draft.input_revision = draft.input_revision.wrapping_add(1).max(1);
    draft.close_revision = 0;
    snapshot.open_id = opened.open_id;
    snapshot.projection = Default::default();
    commands.trigger(UiInput {
        webview,
        payload: CommandPaletteDraftRequest {
            open_id: opened.open_id,
            query: opened.url.clone(),
            start,
        },
    });
}

fn update(
    trigger: On<UiInput<CommandPaletteDraftRequest>>,
    mut palettes: Query<(&PaletteOpen, &mut PaletteDraftInput)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, mut draft)) = palettes.get_mut(target) else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    draft.open_id = request.open_id;
    if draft.query != request.query {
        draft.history_cursor = None;
        draft.history_scratch.clear();
        draft.query.clone_from(&request.query);
        draft.selected = 0;
        draft.navigating = false;
        commands.entity(target).remove::<(
            AgentMenuOpen,
            ModelMenuOpen,
            PermissionMenuOpen,
            ProjectMenuOpen,
            BranchMenuOpen,
            PaletteMenuCursor,
        )>();
    }
    draft.surface = if request.start {
        CommandPaletteSurface::Start
    } else {
        CommandPaletteSurface::Modal
    };
}

fn highlight(
    trigger: On<UiInput<CommandPaletteHighlightRequest>>,
    mut palettes: Query<(&PaletteOpen, &mut PaletteDraftInput, &PaletteSnapshot)>,
) {
    let target = trigger.event().webview;
    let Ok((opened, mut input, snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    let rows = snapshot.projection.rows.len();
    input.selected = (request.index as usize).min(rows.saturating_sub(1));
    input.navigating = true;
}

fn history(
    trigger: On<UiInput<CommandPaletteHistoryMoveRequest>>,
    mut palettes: Query<(&PaletteOpen, &mut PaletteDraftInput, &PaletteSnapshot)>,
) {
    let target = trigger.event().webview;
    let Ok((opened, mut input, snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    let history = &snapshot.prompt_history;
    if history.is_empty() {
        return;
    }
    let current = input.query.clone();
    let (query, cursor, scratch) = if request.older {
        let next = input
            .history_cursor
            .map_or(history.len() - 1, |index| index.saturating_sub(1));
        let scratch = match input.history_cursor {
            Some(_) => input.history_scratch.clone(),
            None => current,
        };
        (history[next].clone(), Some(next), scratch)
    } else {
        match input.history_cursor {
            Some(index) if index + 1 < history.len() => (
                history[index + 1].clone(),
                Some(index + 1),
                input.history_scratch.clone(),
            ),
            Some(_) => (
                input.history_scratch.clone(),
                None,
                input.history_scratch.clone(),
            ),
            None => return,
        }
    };
    input.query = query;
    input.history_cursor = cursor;
    input.history_scratch = scratch;
    input.selected = 0;
    input.navigating = false;
    input.input_revision = input.input_revision.wrapping_add(1).max(1);
}

type SubmitPalettes<'w, 's> = Query<
    'w,
    's,
    (
        &'static PaletteOpen,
        &'static mut PaletteDraftInput,
        &'static PaletteSnapshot,
        &'static PaletteRowsSnapshot,
        &'static PaletteContributionRows,
    ),
>;

fn submit_input(
    trigger: On<UiInput<CommandPaletteSubmitRequest>>,
    mut palettes: SubmitPalettes,
    contributions: Query<&CommandBarContribution>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, mut input, snapshot, rows, contributed)) = palettes.get_mut(target) else {
        return;
    };
    let open_id = trigger.event().payload.open_id;
    if open_id != opened.0.open_id {
        return;
    }
    let selected = snapshot.projection.selected as usize;
    if let Some(entity) = selected
        .checked_sub(rows.items.len())
        .and_then(|index| contributed.0.get(index).copied())
        && let Ok(contribution) = contributions.get(entity)
    {
        commands.trigger(CommandBarContributionActivated {
            target: entity,
            webview: target,
            query: input.query.clone(),
            open: opened.0.target,
        });
        if contribution.close {
            commands.trigger(CommandBarDismiss::new(target, true));
        }
        return;
    }
    let clamped = rows.selected(selected);
    let attachments = &snapshot.attachments;
    if input.surface.is_start() && input.query.trim().is_empty() && !attachments.is_empty() {
        let target_url = rows.default_prompt_target();
        if !target_url.is_empty() {
            input.bump_close_revision();
            commands.trigger(CommandBarDismiss::new(target, true));
        }
        let payload = PaletteDraftInput::prompt_request("", target_url, attachments);
        commands.trigger(UiInput {
            webview: target,
            payload,
        });
        return;
    }
    if rows.should_activate_selected(opened, &input, clamped) {
        let payload = CommandPaletteActivateRequest {
            open_id,
            index: clamped as u32,
        };
        commands.trigger(UiInput {
            webview: target,
            payload,
        });
        return;
    }
    if input.surface.is_start() {
        if rows.mode == PaletteMode::Slash && rows.items.is_empty() || rows.numbered {
            return;
        }
        if rows.start_prompt_mode {
            input.bump_close_revision();
            commands.trigger(CommandBarDismiss::new(target, true));
            let payload = PaletteDraftInput::prompt_request(
                input.query.trim(),
                rows.default_prompt_target(),
                attachments,
            );
            commands.trigger(UiInput {
                webview: target,
                payload,
            });
            return;
        }
    }
    submit_typed_fallback(target, opened, &mut input, rows, clamped, &mut commands);
}

fn submit_typed_fallback(
    target: Entity,
    opened: &PaletteOpen,
    input: &mut PaletteDraftInput,
    rows: &PaletteRowsSnapshot,
    clamped: usize,
    commands: &mut Commands,
) {
    if let PaletteMode::Picking(picker) = &rows.mode {
        if opened.0.picker_typed {
            input.bump_close_revision();
            commands.trigger(CommandBarDismiss::new(target, true));
            let payload = PickRequest {
                pick: CommandBarPick::Typed {
                    picker: picker.clone(),
                    value: input.query.trim().to_string(),
                },
            };
            commands.trigger(UiInput {
                webview: target,
                payload,
            });
        }
        return;
    }
    if matches!(rows.mode, PaletteMode::Ex) {
        if let Some(line) = PaletteOpen::parse_ex_line(&input.query) {
            input.bump_close_revision();
            commands.trigger(CommandBarDismiss::new(target, true));
            commands.trigger(UiInput {
                webview: target,
                payload: ExRequest { line },
            });
        }
        return;
    }
    let closes = !PaletteRowsSnapshot::row_beats_guessed_url(rows.items.get(clamped), &input.query)
        && PaletteDraftInput::opens_typed_url_on_enter(
            &input.query,
            opened.0.target,
            input.navigating,
        );
    if !closes && (PaletteDraftInput::only_files(&input.query) || input.query.is_empty()) {
        return;
    }
    if closes {
        input.bump_close_revision();
        commands.trigger(CommandBarDismiss::new(target, true));
    }
    let payload = OpenRequest {
        value: input.query.clone(),
        open: opened.0.target,
    };
    commands.trigger(UiInput {
        webview: target,
        payload,
    });
}

fn activate(
    trigger: On<UiInput<CommandPaletteActivateRequest>>,
    mut palettes: SubmitPalettes,
    contributions: Query<&CommandBarContribution>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, mut input, snapshot, rows, contributed)) = palettes.get_mut(target) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    let index = trigger.event().payload.index as usize;
    if let Some(entity) = index
        .checked_sub(rows.items.len())
        .and_then(|offset| contributed.0.get(offset).copied())
        && let Ok(contribution) = contributions.get(entity)
    {
        commands.trigger(CommandBarContributionActivated {
            target: entity,
            webview: target,
            query: input.query.clone(),
            open: opened.0.target,
        });
        if contribution.close {
            commands.trigger(CommandBarDismiss::new(target, true));
        }
        return;
    }
    let Some(row) = rows.items.get(index) else {
        return;
    };
    let attachments = &snapshot.attachments;
    if input.surface.is_start()
        && PaletteDraftInput::only_files(&input.query)
        && row.kind == PaletteRowKind::File
    {
        if row.display.directory {
            input.replace_query(&format!("@{}", row.display.file_path));
        } else {
            let path = row.display.file_path.clone();
            input.replace_query("");
            commands.trigger(UiInput {
                webview: target,
                payload: ChatAttachPaths { paths: vec![path] },
            });
        }
        return;
    }
    if (PaletteDraftInput::is_prompt(&input.query) || !attachments.is_empty())
        && let Some(target_url) = PaletteRowsSnapshot::prompt_target_url(row)
    {
        input.bump_close_revision();
        commands.trigger(CommandBarDismiss::new(target, true));
        if PaletteRowsSnapshot::prompt_target_matches(row, &input.query) && attachments.is_empty() {
            commands.trigger(UiInput {
                webview: target,
                payload: OpenRequest {
                    value: target_url.to_string(),
                    open: opened.0.target,
                },
            });
        } else {
            commands.trigger(UiInput {
                webview: target,
                payload: PaletteDraftInput::prompt_request(
                    input.query.trim(),
                    target_url,
                    attachments,
                ),
            });
        }
        return;
    }
    if row.kind == PaletteRowKind::Slash {
        input.replace_query(&format!("/{} ", row.slash_name));
        return;
    }
    input.bump_close_revision();
    commands.trigger(CommandBarDismiss::new(target, true));
    dispatch_row_payload(target, row, opened.0.target, &mut commands);
}

fn dispatch_row_payload(
    target: Entity,
    row: &PaletteRow,
    open_target: Option<vmux_api::open_target::OpenTarget>,
    commands: &mut Commands,
) {
    match row.kind {
        PaletteRowKind::File => {
            commands.trigger(UiInput {
                webview: target,
                payload: OpenRequest {
                    value: format!("file://{}", row.display.file_path),
                    open: open_target,
                },
            });
        }
        PaletteRowKind::Stack => {
            if let Some((pane, index)) = row.tab {
                commands.trigger(UiInput {
                    webview: target,
                    payload: SwitchTabRequest { pane, index },
                });
            }
        }
        PaletteRowKind::Command => {
            commands.trigger(UiInput {
                webview: target,
                payload: InvokeRequest {
                    id: row.command_id.clone(),
                    open: open_target,
                },
            });
        }
        PaletteRowKind::Pick => {
            if let Some(pick) = row.pick.clone() {
                commands.trigger(UiInput {
                    webview: target,
                    payload: PickRequest { pick },
                });
            }
        }
        PaletteRowKind::Page
        | PaletteRowKind::Navigate
        | PaletteRowKind::History
        | PaletteRowKind::Search => {
            if !row.display.url.is_empty() {
                commands.trigger(UiInput {
                    webview: target,
                    payload: OpenRequest {
                        value: row.display.url.clone(),
                        open: open_target,
                    },
                });
            }
        }
        PaletteRowKind::Slash | PaletteRowKind::Disabled => {}
    }
}

fn next(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<CommandBarNextBinding>>,
    mut palettes: Query<(&mut PaletteDraftInput, &PaletteSnapshot)>,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let target = trigger.event().invocation().caller;
    let Ok((mut input, snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let rows = snapshot.projection.rows.len();
    input.selected = (input.selected + 1).min(rows.saturating_sub(1));
    input.navigating = true;
    input.input_revision = input.input_revision.wrapping_add(1).max(1);
}

fn previous(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<CommandBarPreviousBinding>>,
    mut palettes: Query<&mut PaletteDraftInput>,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let target = trigger.event().invocation().caller;
    let Ok(mut input) = palettes.get_mut(target) else {
        return;
    };
    input.selected = input.selected.saturating_sub(1);
    input.navigating = true;
    input.input_revision = input.input_revision.wrapping_add(1).max(1);
}

fn complete(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<CommandBarCompleteBinding>>,
    mut palettes: Query<(&mut PaletteDraftInput, &PaletteSnapshot)>,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let target = trigger.event().invocation().caller;
    let Ok((mut input, snapshot)) = palettes.get_mut(target) else {
        return;
    };
    if snapshot.projection.ghost.is_empty() {
        return;
    }
    input.query.push_str(&snapshot.projection.ghost);
    input.selected = 0;
    input.navigating = false;
    input.input_revision = input.input_revision.wrapping_add(1).max(1);
}

fn dismiss(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<CommandBarDismissBinding>>,
    mut palettes: Query<&mut PaletteDraftInput>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let target = trigger.event().invocation().caller;
    let Ok(mut input) = palettes.get_mut(target) else {
        return;
    };
    input.bump_close_revision();
    commands.trigger(CommandBarDismiss::new(target, true));
}

fn submit(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<CommandBarSubmitBinding>>,
    palettes: Query<&PaletteOpen>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let target = trigger.event().invocation().caller;
    let Ok(opened) = palettes.get(target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: target,
        payload: CommandPaletteSubmitRequest {
            open_id: opened.0.open_id,
        },
    });
}

fn query(
    mut palettes: Query<(
        Entity,
        &PaletteOpen,
        &PaletteDraftInput,
        &mut PublishedQuery,
    )>,
    mut commands: Commands,
) {
    for (target, opened, input, mut published) in &mut palettes {
        let query = if input.surface.is_start() {
            input.result_query.clone()
        } else {
            input.query.clone()
        };
        let next = PublishedQuery {
            open_id: opened.0.open_id,
            query,
            start: input.surface.is_start(),
            open: opened.0.target,
            picker: opened.0.picker.clone(),
            numbered: opened.0.picker_numbered,
        };
        if *published == next {
            continue;
        }
        *published = next.clone();
        commands.trigger(CommandBarQueryChanged {
            target,
            open_id: next.open_id,
            query: next.query,
            start: next.start,
            open: next.open,
            picker: next.picker,
            numbered: next.numbered,
        });
    }
}

fn rows(
    mut palettes: Query<(
        &PaletteOpen,
        &PaletteDraftInput,
        &PaletteSnapshot,
        &mut PaletteRowsSnapshot,
    )>,
) {
    for (opened, input, snapshot, mut projected) in &mut palettes {
        if !input.surface.is_start() && !opened.0.open_id.is_open() {
            continue;
        }
        if input.open_id != opened.0.open_id || snapshot.open_id != opened.0.open_id {
            continue;
        }
        let next = PaletteRowsSnapshot::build(opened, input, snapshot, &input.result_query);
        if *projected != next {
            *projected = next;
        }
    }
}

type ProjectionTargets<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PaletteOpen,
        &'static mut PaletteDraftInput,
        Option<&'static PaletteMenuCursor>,
        &'static mut PaletteContext,
        &'static mut PaletteSnapshot,
        &'static PaletteRowsSnapshot,
        &'static mut PaletteContributionRows,
        Has<AgentMenuOpen>,
        Has<ModelMenuOpen>,
        Has<PermissionMenuOpen>,
        Has<ProjectMenuOpen>,
        Has<BranchMenuOpen>,
    ),
>;

fn project(
    mut palettes: ProjectionTargets,
    contributions: Query<(Entity, &CommandBarContribution, Option<&ChildOf>)>,
    locale: Option<Res<ResolvedLocale>>,
    #[cfg(test)] mut builds: Option<ResMut<ProjectionBuilds>>,
) {
    for (
        target,
        opened,
        mut input,
        cursor,
        mut context,
        mut snapshot,
        projected_rows,
        mut projected_contributions,
        agent_menu,
        model_menu,
        permission_menu,
        project_menu,
        branch_menu,
    ) in &mut palettes
    {
        if !input.surface.is_start() && !opened.0.open_id.is_open() {
            continue;
        }
        #[cfg(test)]
        if let Some(builds) = builds.as_deref_mut() {
            builds.0 += 1;
        }
        let current = snapshot.bypass_change_detection();
        if input.open_id != opened.0.open_id || current.open_id != opened.0.open_id {
            continue;
        }
        let (mut projection, composer) = projected_rows.project_ui(opened, &input);
        projection.history_recalling = input.history_cursor.is_some();

        let mut contributed: Vec<(Entity, &CommandBarContribution)> = Vec::new();
        for (entity, contribution, parent) in &contributions {
            if parent.is_none_or(|parent| parent.parent() == target)
                && opened.0.picker == contribution.picker
                && !PaletteDraftInput::only_files(&input.result_query)
                && contribution.matches(&input.result_query)
            {
                contributed.push((entity, contribution));
            }
        }
        let preferred = projected_contributions.append_matching(
            &mut contributed,
            &mut projection,
            locale.as_deref(),
        );
        let selected = if projected_contributions.0.is_empty() {
            projected_rows.selected(input.selected)
        } else if !input.navigating {
            preferred.unwrap_or(input.selected.min(projection.rows.len().saturating_sub(1)))
        } else {
            input.selected.min(projection.rows.len().saturating_sub(1))
        };
        if input.selected != selected {
            input.selected = selected;
        }
        projection.selected = selected as u32;
        projection.navigating = input.navigating;
        projection.menus.agent = agent_menu;
        projection.menus.model = model_menu;
        projection.menus.permission = permission_menu;
        projection.menus.project = project_menu;
        projection.menus.branch = branch_menu;
        projection.menu_cursor = cursor.map_or(0, |cursor| cursor.0 as u32);
        projection.input_revision = input.input_revision;
        projection.close_revision = input.close_revision;
        let agent = if !composer.model_agent_key.is_empty() {
            composer.model_agent_key
        } else {
            composer.permission_agent_key
        };
        let next_context = PaletteContext {
            open_id: opened.0.open_id,
            agent,
            cwd: composer.cwd,
            project: composer.project,
        };
        if *context != next_context {
            *context = next_context;
        }
        if current.projection != projection {
            snapshot.projection = projection;
        }
    }
}

fn publish(
    snapshots: Query<(Entity, &PaletteSnapshot), Changed<PaletteSnapshot>>,
    mut commands: Commands,
) {
    for (target, snapshot) in &snapshots {
        let ui_state = CommandPaletteUiState {
            open_id: snapshot.open_id,
            projection: snapshot.projection.clone(),
            completions: snapshot.completions.clone(),
            completions_partial: snapshot.completions_partial,
            completions_total: snapshot.completions_total,
            history: snapshot.history.clone(),
            prompt_history: snapshot.prompt_history.clone(),
            branch_project: snapshot.branch_project.clone(),
            branches: snapshot.branches.clone(),
            media_query: snapshot.media_query.clone(),
            media_entries: snapshot.media_entries.clone(),
            media_options: snapshot.media_options.clone(),
            media_loading: snapshot.media_loading,
            media_selected: snapshot.media_selected,
            attachments: snapshot.attachments.clone(),
            composer_attachments: snapshot.composer_attachments.clone(),
            attachment_sequence: snapshot.attachment_sequence,
        };
        commands.trigger(UiStateWrite::<CommandBarUiState>::from_event(
            target, &ui_state,
        ));
    }
}

fn detach(pages: DetachedPalettePages, mut commands: Commands) {
    for page in &pages {
        let mut page = commands.entity(page);
        page.remove::<(
            PaletteSnapshot,
            PaletteOpen,
            PaletteContext,
            PaletteDraftInput,
            PublishedQuery,
            PaletteRowsSnapshot,
            PaletteContributionRows,
        )>();
        page.remove::<(
            AgentMenuOpen,
            ModelMenuOpen,
            PermissionMenuOpen,
            ProjectMenuOpen,
            BranchMenuOpen,
            PaletteMenuCursor,
        )>();
        page.remove::<(
            search::PaletteSearch,
            vmux_ecs::search::State<search::CompletionSearch>,
            vmux_ecs::search::State<search::HistorySearch>,
            prompt::PalettePrompt,
            branch::PaletteBranch,
            media::PaletteMedia,
        )>();
    }
}

#[derive(Component)]
struct PendingPaletteRequest;

fn queue_results(mut palettes: Query<(&PaletteOpen, &mut PaletteDraftInput)>) {
    let now = Instant::now();
    for (opened, mut input) in &mut palettes {
        if input.open_id != opened.0.open_id {
            continue;
        }
        input.synchronize_results(now);
    }
}

fn settle_results(mut palettes: Query<&mut PaletteDraftInput>) {
    let now = Instant::now();
    for mut input in &mut palettes {
        input.settle_results(now);
    }
}

fn wake(
    pending: Query<(), With<PendingPaletteRequest>>,
    inputs: Query<&PaletteDraftInput>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    if pending.is_empty() && !inputs.iter().any(|input| input.result_due.is_some()) {
        return;
    }
    let Some(proxy) = proxy else {
        return;
    };
    let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandInvocation;
    use vmux_api::command_bar::{
        CommandBarCommandEntry, CommandBarPage, CommandBarPickRow, CommandBarPicker, CommandBarTab,
        PathEntry,
    };

    #[derive(Component, Default)]
    struct CapturedPaletteOutputs {
        invocations: Vec<InvokeRequest>,
        opens: Vec<OpenRequest>,
        prompts: Vec<PromptRequest>,
        switches: Vec<SwitchTabRequest>,
        exes: Vec<ExRequest>,
        picks: Vec<PickRequest>,
        attachments: Vec<Vec<String>>,
        dismissals: u32,
    }

    impl CapturedPaletteOutputs {
        fn capture_invoke(trigger: On<UiInput<InvokeRequest>>, mut captured: Query<&mut Self>) {
            if let Ok(mut captured) = captured.get_mut(trigger.event().webview) {
                captured.invocations.push(trigger.event().payload.clone());
            }
        }

        fn capture_open(trigger: On<UiInput<OpenRequest>>, mut captured: Query<&mut Self>) {
            if let Ok(mut captured) = captured.get_mut(trigger.event().webview) {
                captured.opens.push(trigger.event().payload.clone());
            }
        }

        fn capture_prompt(trigger: On<UiInput<PromptRequest>>, mut captured: Query<&mut Self>) {
            if let Ok(mut captured) = captured.get_mut(trigger.event().webview) {
                captured.prompts.push(trigger.event().payload.clone());
            }
        }

        fn capture_switch(trigger: On<UiInput<SwitchTabRequest>>, mut captured: Query<&mut Self>) {
            if let Ok(mut captured) = captured.get_mut(trigger.event().webview) {
                captured.switches.push(trigger.event().payload.clone());
            }
        }

        fn capture_ex(trigger: On<UiInput<ExRequest>>, mut captured: Query<&mut Self>) {
            if let Ok(mut captured) = captured.get_mut(trigger.event().webview) {
                captured.exes.push(trigger.event().payload.clone());
            }
        }

        fn capture_pick(trigger: On<UiInput<PickRequest>>, mut captured: Query<&mut Self>) {
            if let Ok(mut captured) = captured.get_mut(trigger.event().webview) {
                captured.picks.push(trigger.event().payload.clone());
            }
        }

        fn capture_attach(trigger: On<UiInput<ChatAttachPaths>>, mut captured: Query<&mut Self>) {
            if let Ok(mut captured) = captured.get_mut(trigger.event().webview) {
                captured
                    .attachments
                    .push(trigger.event().payload.paths.clone());
            }
        }

        fn capture_dismiss(trigger: On<CommandBarDismiss>, mut captured: Query<&mut Self>) {
            if let Ok(mut captured) = captured.get_mut(trigger.event().webview) {
                captured.dismissals += 1;
            }
        }

        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
                .add_plugins(PalettePlugin)
                .add_observer(Self::capture_invoke)
                .add_observer(Self::capture_open)
                .add_observer(Self::capture_prompt)
                .add_observer(Self::capture_switch)
                .add_observer(Self::capture_ex)
                .add_observer(Self::capture_pick)
                .add_observer(Self::capture_attach)
                .add_observer(Self::capture_dismiss);
            app
        }
    }

    #[test]
    fn open_versions_reject_older_inputs() {
        let mut version = OpenVersion::default();

        assert_eq!(version.accept(OpenId(4)), Some(true));
        assert_eq!(version.accept(OpenId(4)), Some(false));
        assert_eq!(version.accept(OpenId(3)), None);
        assert_eq!(version.accept(OpenId(5)), Some(true));
    }

    #[test]
    fn request_generations_reject_previous_responses() {
        let mut generation = RequestGeneration::default();
        let first = generation.advance();
        let second = generation.advance();

        assert!(!generation.matches(first));
        assert!(generation.matches(second));
    }

    #[test]
    fn start_results_wait_for_the_latest_query_to_settle() {
        let open_id = OpenId(7);
        let now = Instant::now();
        let mut input = PaletteDraftInput {
            open_id,
            surface: CommandPaletteSurface::Start,
            ..Default::default()
        };

        input.synchronize_results(now);
        input.query = "co".to_string();
        input.synchronize_results(now);
        input.query = "codex".to_string();
        input.synchronize_results(now + Duration::from_millis(1));

        assert_eq!(input.result_query, "");
        assert!(!input.settle_results(now + Duration::from_millis(120)));
        assert!(input.settle_results(now + Duration::from_millis(121)));
        assert_eq!(input.result_query, "codex");
    }

    #[test]
    fn modal_results_follow_input_without_debounce() {
        let open_id = OpenId(7);
        let mut input = PaletteDraftInput {
            open_id,
            ..Default::default()
        };

        input.query = ">close".to_string();
        input.synchronize_results(Instant::now());

        assert_eq!(input.result_query, ">close");
        assert!(input.result_pending.is_none());
    }

    #[test]
    fn start_surface_initializes_with_the_none_open_id() {
        let mut app = CapturedPaletteOutputs::app();
        let page = app.world_mut().spawn(HostsLauncher).id();
        app.update();

        app.world_mut()
            .trigger(UiStateWrite::<CommandBarUiState>::from_event(
                page,
                &CommandBarOpenEvent {
                    pages: vec![CommandBarPage {
                        url: "vmux://sessions/?agent=codex-acp".into(),
                        title: "Codex".into(),
                        prompt_target: true,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ));
        app.update();

        let draft = app.world().get::<PaletteDraftInput>(page).unwrap();
        let snapshot = app.world().get::<PaletteSnapshot>(page).unwrap();
        assert!(draft.initialized);
        assert!(draft.surface.is_start());
        assert_eq!(snapshot.projection.composer.agents.len(), 1);
        assert_eq!(
            snapshot.projection.composer.agents[0].url,
            "vmux://sessions/?agent=codex-acp"
        );
    }

    #[test]
    fn closed_panels_stop_building_palettes_and_reopening_restores_the_composer() {
        let mut app = CapturedPaletteOutputs::app();
        app.init_resource::<ProjectionBuilds>();
        let page = app.world_mut().spawn(RendersLauncherPanel).id();
        app.update();
        assert_eq!(app.world().resource::<ProjectionBuilds>().0, 0);
        let mut opened = CommandBarOpenEvent {
            open_id: OpenId(1),
            pages: vec![CommandBarPage {
                url: "vmux://sessions/?agent=codex".into(),
                title: "Codex".into(),
                prompt_target: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        app.world_mut()
            .trigger(UiStateWrite::<CommandBarUiState>::from_event(page, &opened));
        app.update();
        let builds = app.world().resource::<ProjectionBuilds>().0;
        assert!(builds > 0);
        app.world_mut()
            .trigger(UiStateWrite::<CommandBarUiState>::from_event(
                page,
                &CommandBarOpenEvent::default(),
            ));
        app.update();
        app.update();
        assert_eq!(app.world().resource::<ProjectionBuilds>().0, builds);
        opened.open_id = OpenId(2);
        app.world_mut()
            .trigger(UiStateWrite::<CommandBarUiState>::from_event(page, &opened));
        app.update();
        assert_eq!(
            app.world()
                .get::<PaletteSnapshot>(page)
                .unwrap()
                .projection
                .composer
                .agents[0]
                .title,
            "Codex"
        );
    }

    #[test]
    fn start_surface_refreshes_a_none_open_id_after_catalog_arrives() {
        let mut app = CapturedPaletteOutputs::app();
        let page = app.world_mut().spawn(HostsLauncher).id();
        app.update();

        app.world_mut()
            .trigger(UiStateWrite::<CommandBarUiState>::from_event(
                page,
                &CommandBarOpenEvent::default(),
            ));
        app.update();
        app.world_mut()
            .trigger(UiStateWrite::<CommandBarUiState>::from_event(
                page,
                &CommandBarOpenEvent {
                    pages: vec![CommandBarPage {
                        url: "vmux://sessions/?agent=codex-acp".into(),
                        title: "Codex".into(),
                        prompt_target: true,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ));
        app.update();

        let snapshot = app.world().get::<PaletteSnapshot>(page).unwrap();
        assert_eq!(snapshot.projection.composer.agents.len(), 1);
        assert!(!snapshot.projection.composer.loading);
    }

    #[test]
    fn launcher_addition_attaches_palette_before_the_first_snapshot() {
        let mut app = CapturedPaletteOutputs::app();
        app.update();
        let page = app.world_mut().spawn_empty().id();
        app.world_mut().entity_mut(page).insert(HostsLauncher);
        app.world_mut()
            .trigger(UiStateWrite::<CommandBarUiState>::from_event(
                page,
                &CommandBarOpenEvent {
                    commands: vec![CommandBarCommandEntry {
                        id: "settings".into(),
                        name: "Settings".into(),
                        shortcut: String::new(),
                    }],
                    ..Default::default()
                },
            ));
        app.update();

        let snapshot = app.world().get::<PaletteSnapshot>(page).unwrap();
        assert!(!snapshot.projection.composer.loading);
    }

    #[test]
    fn page_entity_projects_palette_rows() {
        let open_id = OpenId(7);
        let mut app = CapturedPaletteOutputs::app();
        let page = app.world_mut().spawn(HostsLauncher).id();
        app.update();
        app.world_mut().entity_mut(page).insert((
            PaletteOpen(CommandBarOpenEvent {
                open_id,
                commands: vec![CommandBarCommandEntry {
                    id: "close_tab".to_string(),
                    name: "Close Tab".to_string(),
                    shortcut: String::new(),
                }],
                ..Default::default()
            }),
            PaletteDraftInput {
                open_id,
                query: ">close".to_string(),
                ..Default::default()
            },
            PaletteSnapshot {
                open_id,
                ..Default::default()
            },
        ));

        app.update();

        let snapshot = app.world().get::<PaletteSnapshot>(page).unwrap();
        assert!(
            snapshot
                .projection
                .rows
                .iter()
                .any(|row| row.title == "Close Tab")
        );
    }

    #[test]
    fn palette_key_commands_update_host_selection_and_menu_state() {
        let mut app = CapturedPaletteOutputs::app();
        let page = app.world_mut().spawn(HostsLauncher).id();
        app.update();

        let open_id = OpenId(7);
        app.world_mut().entity_mut(page).insert((
            PaletteOpen(CommandBarOpenEvent {
                open_id,
                commands: vec![
                    CommandBarCommandEntry {
                        id: "close_tab".to_string(),
                        name: "Close Tab".to_string(),
                        shortcut: String::new(),
                    },
                    CommandBarCommandEntry {
                        id: "close_window".to_string(),
                        name: "Close Window".to_string(),
                        shortcut: String::new(),
                    },
                ],
                ..Default::default()
            }),
            PaletteDraftInput {
                open_id,
                query: ">close".to_string(),
                ..Default::default()
            },
            PaletteSnapshot {
                open_id,
                ..Default::default()
            },
        ));
        app.update();
        app.world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .write(CommandInvocation::new(page, "command_bar_next"));
        app.update();

        let input = app.world().get::<PaletteDraftInput>(page).unwrap();
        assert_eq!(input.selected, 1);
        assert!(input.navigating);
        assert_eq!(input.input_revision, 1);

        app.world_mut()
            .get_mut::<PaletteSnapshot>(page)
            .unwrap()
            .projection
            .composer
            .agents = vec![
            vmux_api::command_bar::CommandPaletteAgent {
                url: "vmux://sessions/vibe/".to_string(),
                title: "Vibe".to_string(),
                icon: vmux_api::PageIcon::None,
            },
            vmux_api::command_bar::CommandPaletteAgent {
                url: "vmux://sessions/codex/".to_string(),
                title: "Codex".to_string(),
                icon: vmux_api::PageIcon::None,
            },
        ];
        app.world_mut()
            .entity_mut(page)
            .insert((AgentMenuOpen, PaletteMenuCursor(0)));
        app.world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .write(CommandInvocation::new(page, "command_bar_menu_next"));
        app.update();

        assert!(app.world().get::<AgentMenuOpen>(page).is_some());
        assert_eq!(app.world().get::<PaletteMenuCursor>(page).unwrap().0, 1);
    }

    #[test]
    fn palette_history_is_recalled_in_host_state() {
        let open_id = OpenId(11);
        let mut app = CapturedPaletteOutputs::app();
        let page = app.world_mut().spawn(HostsLauncher).id();
        app.update();
        app.world_mut().entity_mut(page).insert((
            PaletteOpen(CommandBarOpenEvent {
                open_id,
                ..Default::default()
            }),
            PaletteDraftInput {
                open_id,
                query: "unfinished".to_string(),
                ..Default::default()
            },
            PaletteSnapshot {
                open_id,
                prompt_history: vec!["first".to_string(), "second".to_string()],
                ..Default::default()
            },
        ));

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteHistoryMoveRequest {
                open_id,
                older: true,
            },
        });

        let input = app.world().get::<PaletteDraftInput>(page).unwrap();
        assert_eq!(input.query, "second");
        assert_eq!(input.history_cursor, Some(1));
        assert_eq!(input.history_scratch, "unfinished");
        assert_eq!(input.input_revision, 1);

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteHistoryMoveRequest {
                open_id,
                older: false,
            },
        });

        let input = app.world().get::<PaletteDraftInput>(page).unwrap();
        assert_eq!(input.query, "unfinished");
        assert_eq!(input.history_cursor, None);
        assert_eq!(input.input_revision, 2);
    }

    #[test]
    fn selecting_a_page_closes_its_inline_panel_before_any_ui_round_trip() {
        for keyboard in [false, true] {
            let mut app = CapturedPaletteOutputs::app();
            app.init_non_send::<bevy_cef::prelude::Browsers>()
                .add_plugins((
                    super::super::controller::ControllerPlugin,
                    super::super::panel::PanelPlugin,
                ));
            let stack = app.world_mut().spawn_empty().id();
            app.world_mut()
                .spawn(crate::host::snapshot::CommandBarWorkspaceSnapshot {
                    stack: Some(stack),
                    ..Default::default()
                });
            let page = app
                .world_mut()
                .spawn((RendersLauncherPanel, CapturedPaletteOutputs::default()))
                .id();
            let other = app
                .world_mut()
                .spawn((
                    RendersLauncherPanel,
                    super::super::panel::CommandBarPanelActive,
                ))
                .id();
            app.update();
            let open_id = OpenId(11);
            let url = "vmux://tools/lsp";
            app.world_mut()
                .trigger(UiStateWrite::<CommandBarUiState>::from_event(
                    page,
                    &CommandBarOpenEvent {
                        open_id,
                        url: "vmux://tools/".into(),
                        target: Some(vmux_api::open_target::OpenTarget::InPlace),
                        pages: vec![CommandBarPage {
                            url: url.into(),
                            title: "LSP Servers".into(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                ));
            app.update();
            assert!(
                app.world()
                    .get::<super::super::panel::CommandBarPanelActive>(page)
                    .is_some()
            );
            let index = app
                .world()
                .get::<PaletteSnapshot>(page)
                .unwrap()
                .projection
                .rows
                .iter()
                .position(|row| row.url == url)
                .expect("the provider page must be selectable");
            if keyboard {
                app.world_mut().trigger(UiInput {
                    webview: page,
                    payload: CommandPaletteHighlightRequest {
                        open_id,
                        index: index as u32,
                    },
                });
                app.update();
                app.world_mut().trigger(UiInput {
                    webview: page,
                    payload: CommandPaletteSubmitRequest { open_id },
                });
            } else {
                app.world_mut().trigger(UiInput {
                    webview: page,
                    payload: CommandPaletteActivateRequest {
                        open_id,
                        index: index as u32,
                    },
                });
            }
            app.world_mut().flush();
            assert!(
                app.world()
                    .get::<super::super::panel::CommandBarPanelActive>(page)
                    .is_none()
            );
            assert!(
                app.world()
                    .get::<super::super::panel::CommandBarPanelActive>(other)
                    .is_some()
            );
            assert!(
                !app.world()
                    .get::<PaletteOpen>(page)
                    .unwrap()
                    .0
                    .open_id
                    .is_open()
            );
            assert_eq!(
                app.world()
                    .get::<CapturedPaletteOutputs>(page)
                    .unwrap()
                    .opens,
                [OpenRequest {
                    value: url.into(),
                    open: Some(vmux_api::open_target::OpenTarget::InPlace),
                }]
            );
        }
    }

    #[test]
    fn submission_dispatches_the_projected_row_in_host_ecs() {
        let mut app = CapturedPaletteOutputs::app();
        let page = app
            .world_mut()
            .spawn((HostsLauncher, CapturedPaletteOutputs::default()))
            .id();
        app.update();

        let open_id = OpenId(11);
        app.world_mut().entity_mut(page).insert((
            PaletteOpen(CommandBarOpenEvent {
                open_id,
                commands: vec![CommandBarCommandEntry {
                    id: "close_tab".to_string(),
                    name: "Close Tab".to_string(),
                    shortcut: String::new(),
                }],
                ..Default::default()
            }),
            PaletteDraftInput {
                open_id,
                query: ">close".to_string(),
                navigating: true,
                ..Default::default()
            },
            PaletteSnapshot {
                open_id,
                ..Default::default()
            },
        ));
        app.update();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteSubmitRequest { open_id },
        });
        app.update();

        let captured = app.world().get::<CapturedPaletteOutputs>(page).unwrap();
        assert_eq!(
            captured.invocations,
            [InvokeRequest {
                id: "close_tab".to_string(),
                open: None,
            }]
        );
        assert_eq!(
            app.world()
                .get::<PaletteDraftInput>(page)
                .unwrap()
                .close_revision,
            1
        );
        assert_eq!(captured.dismissals, 1);
    }

    #[test]
    fn submission_dispatches_prompt_ex_pick_switch_and_file_attachment() {
        let mut app = CapturedPaletteOutputs::app();
        let page = app
            .world_mut()
            .spawn((HostsLauncher, CapturedPaletteOutputs::default()))
            .id();
        app.update();

        let open_id = OpenId(21);
        app.world_mut().entity_mut(page).insert((
            PaletteOpen(CommandBarOpenEvent {
                open_id,
                pages: vec![CommandBarPage {
                    url: "vmux://sessions/?agent=vibe".into(),
                    title: "Vibe".into(),
                    prompt_target: true,
                    ..Default::default()
                }],
                tabs: vec![CommandBarTab {
                    title: "Docs".into(),
                    url: "vmux://sessions/session-codex".into(),
                    pane_id: 8,
                    tab_index: 1,
                    is_active: false,
                    location: "space-1".into(),
                }],
                ..Default::default()
            }),
            PaletteDraftInput {
                open_id,
                surface: CommandPaletteSurface::Start,
                query: "fix the failing test".into(),
                result_query: "fix the failing test".into(),
                ..Default::default()
            },
            PaletteSnapshot {
                open_id,
                ..Default::default()
            },
        ));
        app.update();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteSubmitRequest { open_id },
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<CapturedPaletteOutputs>(page)
                .unwrap()
                .prompts,
            [PromptRequest {
                text: "fix the failing test".into(),
                target_url: Some("vmux://sessions/?agent=vibe".into()),
                attachments: Vec::new(),
            }]
        );

        let ex_id = OpenId(22);
        app.world_mut().entity_mut(page).insert((
            PaletteOpen(CommandBarOpenEvent {
                open_id: ex_id,
                ..Default::default()
            }),
            PaletteDraftInput {
                open_id: ex_id,
                surface: CommandPaletteSurface::Modal,
                query: ":noh".into(),
                result_query: ":noh".into(),
                ..Default::default()
            },
            PaletteSnapshot {
                open_id: ex_id,
                ..Default::default()
            },
        ));
        app.update();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteSubmitRequest { open_id: ex_id },
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<CapturedPaletteOutputs>(page)
                .unwrap()
                .exes,
            [ExRequest { line: "noh".into() }]
        );

        let pick_id = OpenId(23);
        let picker = CommandBarPicker::new("test.encoding-reopen");
        app.world_mut().entity_mut(page).insert((
            PaletteOpen(CommandBarOpenEvent {
                open_id: pick_id,
                picker: Some(picker.clone()),
                picks: vec![CommandBarPickRow {
                    label: "Shift_JIS".into(),
                    pick: CommandBarPick::Typed {
                        picker: picker.clone(),
                        value: "Shift_JIS".into(),
                    },
                }],
                ..Default::default()
            }),
            PaletteDraftInput {
                open_id: pick_id,
                surface: CommandPaletteSurface::Modal,
                query: "shift".into(),
                result_query: "shift".into(),
                ..Default::default()
            },
            PaletteSnapshot {
                open_id: pick_id,
                ..Default::default()
            },
        ));
        app.update();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteSubmitRequest { open_id: pick_id },
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<CapturedPaletteOutputs>(page)
                .unwrap()
                .picks,
            [PickRequest {
                pick: CommandBarPick::Typed {
                    picker,
                    value: "Shift_JIS".into(),
                },
            }]
        );

        let attach_id = OpenId(24);
        app.world_mut().entity_mut(page).insert((
            PaletteOpen(CommandBarOpenEvent {
                open_id: attach_id,
                ..Default::default()
            }),
            PaletteDraftInput {
                open_id: attach_id,
                surface: CommandPaletteSurface::Start,
                query: "@main.rs".into(),
                result_query: "@main.rs".into(),
                ..Default::default()
            },
            PaletteSnapshot {
                open_id: attach_id,
                completions: vec![PathEntry {
                    name: "src/main.rs".into(),
                    is_dir: false,
                    full_path: "/repo/src/main.rs".into(),
                    project: "repo".into(),
                }],
                completions_total: 1,
                ..Default::default()
            },
        ));
        app.update();
        let before_close = app
            .world()
            .get::<PaletteDraftInput>(page)
            .unwrap()
            .close_revision;
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteSubmitRequest { open_id: attach_id },
        });
        app.update();
        let input = app.world().get::<PaletteDraftInput>(page).unwrap();
        assert_eq!(input.query, "");
        assert_eq!(input.close_revision, before_close);
        assert_eq!(
            app.world()
                .get::<CapturedPaletteOutputs>(page)
                .unwrap()
                .attachments,
            [vec!["/repo/src/main.rs".to_string()]]
        );
    }
}
