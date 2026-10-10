use super::driver::{CompletionQuery, PaletteDecision, PaletteDraft, PaletteRows, PaletteState};
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use std::time::{Duration, Instant};
use vmux_api::command_bar::{
    CommandBarOpenEvent, CommandBarUiState, CommandBarUiStatePatch, CommandPaletteActivateRequest,
    CommandPaletteDraftRequest, CommandPaletteHighlightRequest, CommandPaletteHistoryMoveRequest,
    CommandPaletteRemoveAttachmentRequest, CommandPaletteSubmitRequest, CommandPaletteUiState,
    OpenId,
};
use vmux_ecs::UiStateWrite;
use vmux_ecs::launcher::{
    CommandBarContribution, CommandBarContributionActivated, CommandBarQueryChanged, HostsLauncher,
    RendersLauncherPanel,
};
#[cfg(test)]
use vmux_ecs::manifest::FeaturePlugin;

use crate::{CommandDispatch, CommandPaletteSurface, CommandRuntimePlugin, ResolvedLocale};

use self::menu::{
    AgentMenuOpen, BranchMenuOpen, ModelMenuOpen, PaletteMenuCursor, PermissionMenuOpen,
    ProjectMenuOpen,
};
use self::request_driver::{OpenVersion, RequestDelay, RequestGeneration};
use super::CommandBarDismiss;

mod branch;
mod media;
mod menu;
mod prompt;
mod prompt_driver;
mod request_driver;
mod search;
mod search_driver;

const START_RESULTS_DEBOUNCE: Duration = Duration::from_millis(120);

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
            .add_observer(apply)
            .add_observer(next)
            .add_observer(previous)
            .add_observer(complete)
            .add_observer(dismiss)
            .add_observer(submit)
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
struct PaletteSnapshot(CommandPaletteUiState);

#[derive(Component, Default)]
struct PaletteOpen(CommandBarOpenEvent);

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
    start: bool,
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
    fn synchronize_results(&mut self, now: Instant) {
        if !self.start {
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
        self.result_due = Some(now + START_RESULTS_DEBOUNCE);
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

type NewPalette<T> = (
    Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
    Without<T>,
);
type ProjectionRow = (
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
);
type DetachedPalette = (
    With<PaletteSnapshot>,
    Without<RendersLauncherPanel>,
    Without<HostsLauncher>,
);

#[derive(Component, Clone, Debug, Default, PartialEq)]
struct PaletteRowsSnapshot(PaletteRows);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
struct PaletteContributionRows(Vec<Entity>);

#[vmux_command::command]
struct CommandBarNextBinding;

#[vmux_command::command]
struct CommandBarPreviousBinding;

#[vmux_command::command]
struct CommandBarCompleteBinding;

#[vmux_command::command]
struct CommandBarDismissBinding;

#[vmux_command::command]
struct CommandBarSubmitBinding;

#[derive(EntityEvent)]
struct PaletteDecisionReady {
    #[event_target]
    target: Entity,
    decision: PaletteDecision,
}

fn attach(pages: Query<Entity, NewPalette<PaletteSnapshot>>, mut commands: Commands) {
    for page in &pages {
        commands.entity(page).insert((
            PaletteSnapshot::default(),
            PaletteOpen::default(),
            PaletteContext::default(),
            PaletteDraftInput::default(),
            PublishedQuery::default(),
            PaletteRowsSnapshot::default(),
            PaletteContributionRows::default(),
        ));
    }
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
    let Ok((mut current, mut draft, mut snapshot, start)) =
        palettes.get_mut(trigger.event().webview())
    else {
        return;
    };
    current.0.clone_from(opened);
    if draft.initialized && draft.open_id == opened.open_id {
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
    commands
        .entity(trigger.event().webview())
        .remove::<menu::OpenMenu>();
    draft.input_revision = draft.input_revision.wrapping_add(1).max(1);
    draft.close_revision = 0;
    snapshot.0.open_id = opened.open_id;
    snapshot.0.projection = Default::default();
    commands.trigger(UiInput {
        webview: trigger.event().webview(),
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
        commands.entity(target).remove::<menu::OpenMenu>();
    }
    draft.start = request.start;
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
    let rows = snapshot.0.projection.rows.len();
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
    let history = &snapshot.0.prompt_history;
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

fn submit_input(
    trigger: On<UiInput<CommandPaletteSubmitRequest>>,
    palettes: Query<(
        &PaletteOpen,
        &PaletteDraftInput,
        &PaletteSnapshot,
        &PaletteRowsSnapshot,
        &PaletteContributionRows,
    )>,
    contributions: Query<&CommandBarContribution>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, input, snapshot, rows, contributed)) = palettes.get(target) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    let selected = snapshot.0.projection.selected as usize;
    if let Some(entity) = selected
        .checked_sub(rows.0.items.len())
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
    let draft = PaletteDraft {
        query: input.query.clone(),
        selected,
        nav_mode: input.navigating,
        target_url: input.target_url.clone(),
        ..Default::default()
    };
    let surface = match input.start {
        true => CommandPaletteSurface::Start,
        false => CommandPaletteSurface::Modal,
    };
    let palette = PaletteState::from_rows(&rows.0, &opened.0, &draft, surface);
    let decision = match surface {
        CommandPaletteSurface::Start => palette.submit_start(&snapshot.0.attachments),
        CommandPaletteSurface::Modal => palette.submit_modal(&snapshot.0.attachments),
    };
    commands.trigger(PaletteDecisionReady { target, decision });
}

fn activate(
    trigger: On<UiInput<CommandPaletteActivateRequest>>,
    palettes: Query<(
        &PaletteOpen,
        &PaletteDraftInput,
        &PaletteSnapshot,
        &PaletteRowsSnapshot,
        &PaletteContributionRows,
    )>,
    contributions: Query<&CommandBarContribution>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, input, snapshot, rows, contributed)) = palettes.get(target) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    let index = trigger.event().payload.index as usize;
    if let Some(entity) = index
        .checked_sub(rows.0.items.len())
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
    let draft = PaletteDraft {
        query: input.query.clone(),
        selected: input.selected,
        nav_mode: input.navigating,
        target_url: input.target_url.clone(),
        ..Default::default()
    };
    let surface = match input.start {
        true => CommandPaletteSurface::Start,
        false => CommandPaletteSurface::Modal,
    };
    let palette = PaletteState::from_rows(&rows.0, &opened.0, &draft, surface);
    let Some(row) = palette.row(index) else {
        return;
    };
    commands.trigger(PaletteDecisionReady {
        target,
        decision: palette.activate(row, &snapshot.0.attachments),
    });
}

fn apply(
    trigger: On<PaletteDecisionReady>,
    mut inputs: Query<&mut PaletteDraftInput>,
    mut commands: Commands,
) {
    let target = trigger.event().target;
    let decision = &trigger.event().decision;
    if decision.closes()
        && let Ok(mut input) = inputs.get_mut(target)
    {
        input.close_revision = input.close_revision.wrapping_add(1).max(1);
        commands.trigger(CommandBarDismiss::new(target, true));
    }
    match decision {
        PaletteDecision::None => {}
        PaletteDecision::Close => {}
        PaletteDecision::Retype(query) => {
            let Ok(mut input) = inputs.get_mut(target) else {
                return;
            };
            input.query.clone_from(query);
            input.selected = 0;
            input.navigating = false;
            input.input_revision = input.input_revision.wrapping_add(1).max(1);
        }
        PaletteDecision::Prompt { request, .. } => {
            commands.trigger(UiInput {
                webview: target,
                payload: request.clone(),
            });
        }
        PaletteDecision::Open { request, .. } => {
            commands.trigger(UiInput {
                webview: target,
                payload: request.clone(),
            });
        }
        PaletteDecision::Invoke(request) => {
            commands.trigger(UiInput {
                webview: target,
                payload: request.clone(),
            });
        }
        PaletteDecision::SwitchTab(request) => {
            commands.trigger(UiInput {
                webview: target,
                payload: request.clone(),
            });
        }
        PaletteDecision::Ex(request) => {
            commands.trigger(UiInput {
                webview: target,
                payload: request.clone(),
            });
        }
        PaletteDecision::Pick(request) => {
            commands.trigger(UiInput {
                webview: target,
                payload: request.clone(),
            });
        }
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
    let rows = snapshot.0.projection.rows.len();
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
    if snapshot.0.projection.ghost.is_empty() {
        return;
    }
    input.query.push_str(&snapshot.0.projection.ghost);
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
    input.close_revision = input.close_revision.wrapping_add(1).max(1);
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
        let next = PublishedQuery {
            open_id: opened.0.open_id,
            query: input.query.clone(),
            start: input.start,
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
        if input.open_id != opened.0.open_id || snapshot.0.open_id != opened.0.open_id {
            continue;
        }
        let draft = PaletteDraft {
            query: input.result_query.clone(),
            selected: input.selected,
            nav_mode: input.navigating,
            target_url: input.target_url.clone(),
            completions: snapshot.0.completions.clone(),
            completions_partial: snapshot.0.completions_partial,
            completions_total: snapshot.0.completions_total as usize,
            history: snapshot.0.history.clone(),
        };
        let surface = match input.start {
            true => CommandPaletteSurface::Start,
            false => CommandPaletteSurface::Modal,
        };
        let next = PaletteRows::build(&opened.0, &draft, surface);
        if projected.0 != next {
            projected.0 = next;
        }
    }
}

fn project(
    mut palettes: Query<ProjectionRow>,
    contributions: Query<(Entity, &CommandBarContribution, Option<&ChildOf>)>,
    locale: Option<Res<ResolvedLocale>>,
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
        let current = snapshot.bypass_change_detection();
        if input.open_id != opened.0.open_id || current.0.open_id != opened.0.open_id {
            continue;
        }
        let draft = PaletteDraft {
            query: input.query.clone(),
            selected: input.selected,
            nav_mode: input.navigating,
            target_url: input.target_url.clone(),
            completions: current.0.completions.clone(),
            completions_partial: current.0.completions_partial,
            completions_total: current.0.completions_total as usize,
            history: current.0.history.clone(),
        };
        let surface = match input.start {
            true => CommandPaletteSurface::Start,
            false => CommandPaletteSurface::Modal,
        };
        let rows = &projected_rows.0;
        let palette = PaletteState::from_rows(rows, &opened.0, &draft, surface);
        let mut projection = palette.projection();
        projection.history_recalling = input.history_cursor.is_some();
        let mut contribution_entities = Vec::new();
        let mut contributed = contributions
            .iter()
            .filter(|(_, contribution, parent)| {
                parent.is_none_or(|parent| parent.parent() == target)
                    && match (&opened.0.picker, &contribution.picker) {
                        (Some(opened), Some(contributed)) => opened == contributed,
                        (None, None) => true,
                        _ => false,
                    }
                    && !CompletionQuery::only_files(&input.result_query)
                    && contribution.matches(&input.result_query)
            })
            .collect::<Vec<_>>();
        contributed.sort_by(|(left_entity, left, _), (right_entity, right, _)| {
            left.rank
                .cmp(&right.rank)
                .then_with(|| left.row.title.cmp(&right.row.title))
                .then_with(|| left_entity.cmp(right_entity))
        });
        let mut preferred = None;
        let mut numbered_count = 0;
        for (entity, contribution, _) in contributed {
            if contribution.preferred && preferred.is_none() {
                preferred = Some(projection.rows.len());
            }
            if contribution.numbered {
                numbered_count += 1;
            }
            contribution_entities.push(entity);
            let mut row = contribution.row.clone();
            if !contribution.title_message_id.is_empty() {
                row.title = locale.as_ref().map_or_else(
                    || vmux_ui::i18n::translate(&contribution.title_message_id),
                    |locale| locale.0.translate(&contribution.title_message_id),
                );
            }
            if !contribution.subtitle_message_id.is_empty() {
                row.subtitle = locale.as_ref().map_or_else(
                    || vmux_ui::i18n::translate(&contribution.subtitle_message_id),
                    |locale| locale.0.translate(&contribution.subtitle_message_id),
                );
            }
            projection.rows.push(row);
        }
        projection.numbered_count = numbered_count;
        let selected = if contribution_entities.is_empty() {
            rows.selected(input.selected)
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
        let agent = if !palette.composer.model_agent_key.is_empty() {
            palette.composer.model_agent_key.clone()
        } else {
            palette.composer.permission_agent_key.clone()
        };
        let next_context = PaletteContext {
            open_id: opened.0.open_id,
            agent,
            cwd: palette.composer.cwd,
            project: palette.composer.project,
        };
        if *context != next_context {
            *context = next_context;
        }
        if projected_contributions.0 != contribution_entities {
            projected_contributions.0 = contribution_entities;
        }
        if current.0.projection == projection {
            continue;
        }
        snapshot.0.projection = projection;
    }
}

fn publish(
    snapshots: Query<(Entity, &PaletteSnapshot), Changed<PaletteSnapshot>>,
    mut commands: Commands,
) {
    for (target, snapshot) in &snapshots {
        commands.trigger(UiStateWrite::<CommandBarUiState>::from_event(
            target,
            &snapshot.0,
        ));
    }
}

fn detach(pages: Query<Entity, DetachedPalette>, mut commands: Commands) {
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
        page.remove::<menu::OpenMenu>();
        page.remove::<(
            search::PaletteSearch,
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
    use vmux_api::command_bar::{CommandBarCommandEntry, CommandBarPage, InvokeRequest};

    #[derive(Component, Default)]
    struct CapturedInvocations(Vec<InvokeRequest>);

    #[derive(Component, Default)]
    struct CapturedDismissals(u32);

    fn capture_invocation(
        trigger: On<UiInput<InvokeRequest>>,
        mut captured: Query<&mut CapturedInvocations>,
    ) {
        let Ok(mut captured) = captured.get_mut(trigger.event().webview) else {
            return;
        };
        captured.0.push(trigger.event().payload.clone());
    }

    fn capture_dismissal(
        trigger: On<CommandBarDismiss>,
        mut captured: Query<&mut CapturedDismissals>,
    ) {
        let Ok(mut captured) = captured.get_mut(trigger.event().webview) else {
            return;
        };
        captured.0 += 1;
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
            start: true,
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
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_plugins(PalettePlugin);
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
        assert!(draft.start);
        assert_eq!(snapshot.0.projection.composer.agents.len(), 1);
        assert_eq!(
            snapshot.0.projection.composer.agents[0].url,
            "vmux://sessions/?agent=codex-acp"
        );
    }

    #[test]
    fn page_entity_projects_palette_rows() {
        let open_id = OpenId(7);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_plugins(PalettePlugin);
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
            PaletteSnapshot(CommandPaletteUiState {
                open_id,
                ..Default::default()
            }),
        ));

        app.update();

        let snapshot = app.world().get::<PaletteSnapshot>(page).unwrap();
        assert!(
            snapshot
                .0
                .projection
                .rows
                .iter()
                .any(|row| row.title == "Close Tab")
        );
    }

    #[test]
    fn palette_key_commands_update_host_selection_and_menu_state() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_plugins(PalettePlugin);
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
            PaletteSnapshot(CommandPaletteUiState {
                open_id,
                ..Default::default()
            }),
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
            .0
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
        let mut app = App::new();
        app.add_observer(history);
        let page = app
            .world_mut()
            .spawn((
                PaletteOpen(CommandBarOpenEvent {
                    open_id,
                    ..Default::default()
                }),
                PaletteDraftInput {
                    open_id,
                    query: "unfinished".to_string(),
                    ..Default::default()
                },
                PaletteSnapshot(CommandPaletteUiState {
                    open_id,
                    prompt_history: vec!["first".to_string(), "second".to_string()],
                    ..Default::default()
                }),
            ))
            .id();

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
    fn submission_dispatches_the_projected_row_in_host_ecs() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_plugins(PalettePlugin)
            .add_observer(capture_invocation)
            .add_observer(capture_dismissal);
        let page = app
            .world_mut()
            .spawn((
                HostsLauncher,
                CapturedInvocations::default(),
                CapturedDismissals::default(),
            ))
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
            PaletteSnapshot(CommandPaletteUiState {
                open_id,
                ..Default::default()
            }),
        ));
        app.update();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: CommandPaletteSubmitRequest { open_id },
        });
        app.update();

        assert_eq!(
            app.world().get::<CapturedInvocations>(page).unwrap().0,
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
        assert_eq!(app.world().get::<CapturedDismissals>(page).unwrap().0, 1);
    }
}
