use std::time::Duration;

use super::model::{
    AgentSegment, PaletteDecision, PaletteDraft, PaletteQuery, PaletteRows, PaletteState,
};
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::{
    CommandBarOpenEvent, CommandBarUiState, CommandBarUiStatePatch, CommandPaletteActivateRequest,
    CommandPaletteDraftRequest, CommandPaletteHighlightRequest, CommandPaletteHistoryMoveRequest,
    CommandPaletteRemoveAttachmentRequest, CommandPaletteSubmitRequest, CommandPaletteUiState,
    OpenId,
};
use vmux_api::mcp::{McpServerRequest, McpServers};
#[cfg(test)]
use vmux_ecs::host::manifest::FeaturePlugin;
use vmux_ecs::host::{UiState, UiStateWrite};
use vmux_ecs::launcher::{HostsLauncher, RendersLauncherPanel};
use vmux_tool::McpSnapshotRequest;

use crate::{
    BindCommands, CommandDispatch, CommandPaletteSurface, CommandRegistry, CommandRuntimePlugin,
};

use self::menu::{
    AgentMenuOpen, BranchMenuOpen, ModelMenuOpen, PaletteMenuCursor, PermissionMenuOpen,
    ProjectMenuOpen,
};
use super::CommandBarDismiss;

mod branch;
mod media;
mod menu;
mod prompt;
mod resume;
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
                vmux_ecs::host::UiStatePlugin::<CommandPaletteUiState>::default(),
                menu::MenuPlugin,
                search::PaletteSearchPlugin,
                prompt::PalettePromptPlugin,
                branch::PaletteBranchPlugin,
                media::PaletteMediaPlugin,
                resume::PaletteResumePlugin,
            ))
            .add_observer(open)
            .add_observer(receive_mcp)
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
            .add_systems(Startup, bind.in_set(BindCommands))
            .add_systems(PreUpdate, (attach, detach))
            .add_systems(PostUpdate, (project, publish).chain())
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
}

type NewPalette<T> = (
    Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
    Without<T>,
);
type ProjectionRow = (
    &'static PaletteOpen,
    &'static PaletteDraftInput,
    Option<&'static PaletteMenuCursor>,
    &'static PaletteMcp,
    &'static mut PaletteContext,
    &'static mut PaletteSnapshot,
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

impl PaletteDraftInput {
    fn history(&mut self, history: &[String], older: bool) -> bool {
        if history.is_empty() {
            return false;
        }
        let current = self.query.clone();
        let (query, cursor, scratch) = if older {
            let next = self
                .history_cursor
                .map_or(history.len() - 1, |index| index.saturating_sub(1));
            let scratch = match self.history_cursor {
                Some(_) => self.history_scratch.clone(),
                None => current.clone(),
            };
            (history[next].clone(), Some(next), scratch)
        } else {
            match self.history_cursor {
                Some(index) if index + 1 < history.len() => (
                    history[index + 1].clone(),
                    Some(index + 1),
                    self.history_scratch.clone(),
                ),
                Some(_) => (
                    self.history_scratch.clone(),
                    None,
                    self.history_scratch.clone(),
                ),
                None => return false,
            }
        };
        self.query = query;
        self.history_cursor = cursor;
        self.history_scratch = scratch;
        self.selected = 0;
        self.navigating = false;
        self.input_revision = self.input_revision.wrapping_add(1).max(1);
        true
    }

    fn next(&mut self, snapshot: &CommandPaletteUiState) {
        let rows = if snapshot.projection.mcp_open {
            snapshot.projection.mcp_entries.len()
        } else {
            snapshot.projection.rows.len()
        };
        self.selected = (self.selected + 1).min(rows.saturating_sub(1));
        self.navigating = true;
        self.changed();
    }

    fn previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.navigating = true;
        self.changed();
    }

    fn complete(&mut self, snapshot: &CommandPaletteUiState) {
        if snapshot.projection.mcp_open || snapshot.projection.ghost.is_empty() {
            return;
        }
        self.query.push_str(&snapshot.projection.ghost);
        self.selected = 0;
        self.navigating = false;
        self.changed();
    }

    fn dismiss(&mut self, snapshot: &CommandPaletteUiState) -> bool {
        if snapshot.projection.mcp_open {
            self.query.clear();
            self.selected = 0;
            self.navigating = false;
            self.changed();
            return false;
        }
        self.close_revision = self.close_revision.wrapping_add(1).max(1);
        true
    }

    fn changed(&mut self) {
        self.input_revision = self.input_revision.wrapping_add(1).max(1);
    }
}

#[derive(Component, Default)]
struct PaletteMcp(McpServers);

#[derive(Component)]
struct PaletteMcpActive;

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

fn bind(registry: CommandRegistry, mut commands: Commands) {
    registry.bind::<CommandBarNextBinding>(&mut commands);
    registry.bind::<CommandBarPreviousBinding>(&mut commands);
    registry.bind::<CommandBarCompleteBinding>(&mut commands);
    registry.bind::<CommandBarDismissBinding>(&mut commands);
    registry.bind::<CommandBarSubmitBinding>(&mut commands);
}

fn attach(pages: Query<Entity, NewPalette<PaletteSnapshot>>, mut commands: Commands) {
    for page in &pages {
        commands.entity(page).insert((
            PaletteSnapshot::default(),
            PaletteOpen::default(),
            PaletteContext::default(),
            PaletteDraftInput::default(),
            PaletteMcp::default(),
            UiState::<CommandPaletteUiState>::default(),
        ));
    }
}

fn receive_mcp(trigger: On<UiStateWrite<McpServers>>, mut palettes: Query<&mut PaletteMcp>) {
    let Ok(mut mcp) = palettes.get_mut(trigger.event().webview()) else {
        return;
    };
    mcp.0.clone_from(trigger.event().update());
}

fn open(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(
        &mut PaletteOpen,
        &mut PaletteDraftInput,
        &mut PaletteSnapshot,
    )>,
    mut commands: Commands,
) {
    let Some(opened) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<CommandBarOpenEvent>>::payload(
            trigger.event().patch(),
        )
    else {
        return;
    };
    let Ok((mut current, mut draft, mut snapshot)) = palettes.get_mut(trigger.event().webview())
    else {
        return;
    };
    current.0.clone_from(opened);
    if draft.open_id == opened.open_id {
        return;
    }
    draft.open_id = opened.open_id;
    draft.query.clone_from(&opened.url);
    draft.target_url.clear();
    draft.selected = PaletteState::opening_selection(opened);
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
}

fn update(
    trigger: On<UiInput<CommandPaletteDraftRequest>>,
    mut palettes: Query<(&PaletteOpen, &mut PaletteDraftInput)>,
    active: Query<(), With<PaletteMcpActive>>,
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
    let wants_mcp = PaletteQuery::new(&request.query).mcp_filter().is_some();
    match (wants_mcp, active.contains(target)) {
        (true, false) => {
            commands.entity(target).insert(PaletteMcpActive);
            commands.trigger(McpSnapshotRequest { target });
        }
        (false, true) => {
            commands.entity(target).remove::<PaletteMcpActive>();
        }
        _ => {}
    }
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
    let rows = match snapshot.0.projection.mcp_open {
        true => snapshot.0.projection.mcp_entries.len(),
        false => snapshot.0.projection.rows.len(),
    };
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
    input.history(&snapshot.0.prompt_history, request.older);
}

fn submit_input(
    trigger: On<UiInput<CommandPaletteSubmitRequest>>,
    palettes: Query<(&PaletteOpen, &PaletteDraftInput, &PaletteSnapshot)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, input, snapshot)) = palettes.get(target) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    if snapshot.0.projection.mcp_open {
        let Some(server) = snapshot
            .0
            .projection
            .mcp_entries
            .get(snapshot.0.projection.selected as usize)
        else {
            return;
        };
        commands.trigger(UiInput {
            webview: target,
            payload: McpServerRequest {
                id: server.id.clone(),
            },
        });
        return;
    }
    let rows = PaletteRows::from_projection(&snapshot.0.projection);
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
    let palette = PaletteState::from_rows(&rows, &opened.0, &draft, surface);
    let decision = match surface {
        CommandPaletteSurface::Start => palette.submit_start(&snapshot.0.attachments),
        CommandPaletteSurface::Modal => palette.submit_modal(&snapshot.0.attachments),
    };
    commands.trigger(PaletteDecisionReady { target, decision });
}

fn activate(
    trigger: On<UiInput<CommandPaletteActivateRequest>>,
    palettes: Query<(&PaletteOpen, &PaletteDraftInput, &PaletteSnapshot)>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, input, snapshot)) = palettes.get(target) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    let index = trigger.event().payload.index as usize;
    if snapshot.0.projection.mcp_open {
        let Some(server) = snapshot.0.projection.mcp_entries.get(index) else {
            return;
        };
        commands.trigger(UiInput {
            webview: target,
            payload: McpServerRequest {
                id: server.id.clone(),
            },
        });
        return;
    }
    let rows = PaletteRows::from_projection(&snapshot.0.projection);
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
    let palette = PaletteState::from_rows(&rows, &opened.0, &draft, surface);
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
    }
    match decision {
        PaletteDecision::None => {}
        PaletteDecision::Close => {
            commands.trigger(CommandBarDismiss::new(target, true));
        }
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
        PaletteDecision::Terminal(request) => {
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
        PaletteDecision::SwitchSpace(request) => {
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
    input.next(&snapshot.0);
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
    input.previous();
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
    input.complete(&snapshot.0);
}

fn dismiss(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<CommandBarDismissBinding>>,
    mut palettes: Query<(&mut PaletteDraftInput, &PaletteSnapshot)>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let target = trigger.event().invocation().caller;
    let Ok((mut input, snapshot)) = palettes.get_mut(target) else {
        return;
    };
    if input.dismiss(&snapshot.0) {
        commands.trigger(CommandBarDismiss::new(target, true));
    }
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

fn project(mut palettes: Query<ProjectionRow>) {
    for (
        opened,
        input,
        cursor,
        mcp,
        mut context,
        mut snapshot,
        agent_menu,
        model_menu,
        permission_menu,
        project_menu,
        branch_menu,
    ) in &mut palettes
    {
        if input.open_id != opened.0.open_id || snapshot.0.open_id != opened.0.open_id {
            continue;
        }
        let draft = PaletteDraft {
            query: input.query.clone(),
            selected: input.selected,
            nav_mode: input.navigating,
            target_url: input.target_url.clone(),
            completions: snapshot.0.completions.clone(),
            completions_partial: snapshot.0.completions_partial,
            completions_total: snapshot.0.completions_total as usize,
            history: snapshot.0.history.clone(),
            sessions: snapshot.0.sessions.clone(),
            sessions_pending: snapshot.0.sessions_loading,
        };
        let surface = match input.start {
            true => CommandPaletteSurface::Start,
            false => CommandPaletteSurface::Modal,
        };
        let rows = PaletteRows::build(&opened.0, &draft, surface);
        let palette = PaletteState::from_rows(&rows, &opened.0, &draft, surface);
        let mut projection = palette.projection();
        projection.history_recalling = input.history_cursor.is_some();
        if let Some(filter) = PaletteQuery::new(&input.query).mcp_filter() {
            let filter = filter.trim().to_ascii_lowercase();
            projection.mcp_open = true;
            for server in &mcp.0.servers {
                if filter.is_empty()
                    || server.id.to_ascii_lowercase().contains(&filter)
                    || server.name.to_ascii_lowercase().contains(&filter)
                    || server.description.to_ascii_lowercase().contains(&filter)
                {
                    projection.mcp_entries.push(server.clone());
                }
            }
            projection.selected = input
                .selected
                .min(projection.mcp_entries.len().saturating_sub(1))
                as u32;
        } else {
            projection.selected = rows.selected(input.selected) as u32;
        }
        projection.navigating = input.navigating;
        projection.menus.agent = agent_menu;
        projection.menus.model = model_menu;
        projection.menus.permission = permission_menu;
        projection.menus.project = project_menu;
        projection.menus.branch = branch_menu;
        projection.menu_cursor = cursor.map_or(0, |cursor| cursor.0 as u32);
        projection.input_revision = input.input_revision;
        projection.close_revision = input.close_revision;
        let next_context = PaletteContext {
            open_id: opened.0.open_id,
            agent: AgentSegment::in_url(&palette.composer.agent_url).unwrap_or_default(),
            cwd: palette.composer.cwd,
            project: palette.composer.project,
        };
        if *context != next_context {
            *context = next_context;
        }
        if snapshot.0.projection == projection {
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
        commands.trigger(UiStateWrite::<CommandPaletteUiState>::from_event(
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
            PaletteMcp,
        )>();
        page.remove::<menu::OpenMenu>();
        page.remove::<(
            UiState<CommandPaletteUiState>,
            search::PaletteSearch,
            prompt::PalettePrompt,
            branch::PaletteBranch,
            media::PaletteMedia,
            resume::PaletteResume,
        )>();
    }
}

#[derive(Default)]
struct OpenVersion {
    initialized: bool,
    open_id: OpenId,
    generation: RequestGeneration,
}

impl OpenVersion {
    fn accept(&mut self, open_id: OpenId) -> Option<bool> {
        if self.initialized && self.open_id == open_id {
            return Some(false);
        }
        if self.initialized && open_id.0 < self.open_id.0 {
            return None;
        }
        self.initialized = true;
        self.open_id = open_id;
        self.generation.advance();
        Some(true)
    }

    fn matches(&self, open_id: OpenId) -> bool {
        self.initialized && self.open_id == open_id
    }

    fn generation(&self) -> u64 {
        self.generation.current()
    }
}

#[derive(Default)]
struct RequestGeneration(u64);

impl RequestGeneration {
    fn advance(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }

    fn current(&self) -> u64 {
        self.0
    }

    fn matches(&self, generation: u64) -> bool {
        generation != 0 && self.0 == generation
    }
}

struct RequestDelay {
    target: Entity,
    generation: u64,
    query: String,
    due: std::time::Instant,
}

impl RequestDelay {
    fn new(target: Entity, generation: u64, query: String, delay: Duration) -> Self {
        Self {
            target,
            generation,
            query,
            due: std::time::Instant::now() + delay,
        }
    }

    fn ready(&self) -> bool {
        std::time::Instant::now() >= self.due
    }
}

#[derive(Component)]
struct PendingPaletteRequest;

fn wake(
    pending: Query<(), With<PendingPaletteRequest>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
) {
    if pending.is_empty() {
        return;
    }
    let Some(proxy) = proxy else {
        return;
    };
    let _ = (**proxy).send_event(bevy::winit::WinitUserEvent::WakeUp);
}

#[cfg(test)]
mod tests;
