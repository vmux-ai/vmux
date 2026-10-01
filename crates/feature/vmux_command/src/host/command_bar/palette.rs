use std::time::Duration;

use super::model::{
    AgentSegment, PaletteDecision, PaletteDraft, PaletteQuery, PaletteRows, PaletteState,
    PaletteSurface,
};
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::{
    CommandBarOpenEvent, CommandBarUiState, CommandBarUiStatePatch, CommandPaletteActivateRequest,
    CommandPaletteDraftRequest, CommandPaletteHighlightRequest, CommandPaletteHistoryMoveRequest,
    CommandPaletteMenu, CommandPaletteMenuActivateRequest, CommandPaletteMenuDismissRequest,
    CommandPaletteMenuHighlightRequest, CommandPaletteMenuMoveRequest,
    CommandPaletteMenuToggleRequest, CommandPaletteRemoveAttachmentRequest, CommandPaletteState,
    CommandPaletteSubmitRequest, OpenId, StartGoToBranch, StartSelectMode, StartSelectModel,
    StartSelectWorkspace,
};
use vmux_api::mcp::{McpServerRequest, McpServers};
#[cfg(test)]
use vmux_core::host::manifest::FeaturePlugin;
use vmux_core::host::{UiState, UiStateWrite};
use vmux_core::launcher::{HostsLauncher, RendersLauncherPanel};
use vmux_tool::McpSnapshotRequest;

use crate::{BindCommands, CommandBinding, CommandDispatch, CommandRegistry, CommandRuntimePlugin};

use super::CommandBarDismiss;

mod branch;
mod media;
mod prompt;
mod resume;
mod search;

pub struct PalettePlugin;

impl Plugin for PalettePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_plugins((
            UiEventPlugin::<(
                CommandPaletteDraftRequest,
                CommandPaletteHighlightRequest,
                CommandPaletteHistoryMoveRequest,
                CommandPaletteMenuToggleRequest,
                CommandPaletteMenuMoveRequest,
                CommandPaletteMenuHighlightRequest,
                CommandPaletteMenuActivateRequest,
                CommandPaletteMenuDismissRequest,
                CommandPaletteSubmitRequest,
                CommandPaletteActivateRequest,
                CommandPaletteRemoveAttachmentRequest,
            )>::default(),
            vmux_core::host::UiStatePlugin::<CommandPaletteState>::default(),
            search::PaletteSearchPlugin,
            prompt::PalettePromptPlugin,
            branch::PaletteBranchPlugin,
            media::PaletteMediaPlugin,
            resume::PaletteResumePlugin,
        ))
        .add_observer(receive_open)
        .add_observer(receive_mcp_servers)
        .add_observer(update_draft)
        .add_observer(highlight)
        .add_observer(move_history)
        .add_observer(toggle_menu)
        .add_observer(move_menu_request)
        .add_observer(highlight_menu)
        .add_observer(activate_menu)
        .add_observer(dismiss_menu_request)
        .add_observer(dispatch_submit)
        .add_observer(activate_row)
        .add_observer(apply_decision)
        .add_observer(apply_key)
        .add_observer(submit)
        .add_observer(move_menu)
        .add_observer(choose_menu)
        .add_observer(dismiss_menu)
        .add_systems(Startup, bind_commands.in_set(BindCommands))
        .add_systems(PreUpdate, (attach_snapshot, detach_snapshot))
        .configure_sets(
            PostUpdate,
            (
                PaletteProjectionSet::Project,
                PaletteProjectionSet::Context,
                PaletteProjectionSet::Publish,
            )
                .chain(),
        )
        .add_systems(PostUpdate, project.in_set(PaletteProjectionSet::Project))
        .add_systems(
            PostUpdate,
            publish_snapshot.in_set(PaletteProjectionSet::Publish),
        )
        .add_systems(Last, keep_frames_coming);
    }
}

#[derive(Component, Default)]
struct PaletteSnapshot(CommandPaletteState);

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

impl PaletteDraftInput {
    fn move_history(&mut self, history: &[String], older: bool) -> bool {
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
}

#[derive(Component, Default)]
struct PaletteMenuState {
    opened: Option<CommandPaletteMenu>,
    cursor: usize,
}

impl PaletteMenuState {
    fn rows(menu: CommandPaletteMenu, snapshot: &CommandPaletteState) -> usize {
        let composer = &snapshot.projection.composer;
        match menu {
            CommandPaletteMenu::Agent => composer.agents.len(),
            CommandPaletteMenu::Model => composer.model_options.len(),
            CommandPaletteMenu::Permission => composer.permission_modes.len(),
            CommandPaletteMenu::Project => {
                composer
                    .projects
                    .iter()
                    .filter(|project| project.depth == 0)
                    .count()
                    + 1
            }
            CommandPaletteMenu::Branch => snapshot.branches.len(),
        }
    }

    fn initial(menu: CommandPaletteMenu, snapshot: &CommandPaletteState) -> usize {
        let composer = &snapshot.projection.composer;
        match menu {
            CommandPaletteMenu::Agent => composer
                .agents
                .iter()
                .position(|agent| agent.url == composer.agent_url)
                .unwrap_or(0),
            CommandPaletteMenu::Model => composer
                .model_options
                .iter()
                .position(|model| model.id == composer.model_current_id)
                .unwrap_or(0),
            CommandPaletteMenu::Permission => composer
                .permission_modes
                .iter()
                .position(|mode| mode.id == composer.permission_current_id)
                .unwrap_or(0),
            CommandPaletteMenu::Project => composer
                .projects
                .iter()
                .filter(|project| project.depth == 0)
                .position(|project| project.is_active)
                .unwrap_or(0),
            CommandPaletteMenu::Branch => snapshot
                .branches
                .iter()
                .position(|branch| branch.branch == composer.branch_label)
                .unwrap_or(0),
        }
    }

    fn toggle(&mut self, menu: CommandPaletteMenu, snapshot: &CommandPaletteState) {
        if self.opened == Some(menu) {
            self.dismiss();
            return;
        }
        self.opened = Some(menu);
        self.cursor = Self::initial(menu, snapshot);
    }

    fn step(&mut self, next: bool, snapshot: &CommandPaletteState) {
        let Some(menu) = self.opened else {
            return;
        };
        let rows = Self::rows(menu, snapshot);
        if rows == 0 {
            return;
        }
        let at = self.cursor.min(rows - 1);
        self.cursor = if next {
            (at + 1).min(rows - 1)
        } else {
            at.saturating_sub(1)
        };
    }

    fn highlight(&mut self, index: usize, snapshot: &CommandPaletteState) {
        let Some(menu) = self.opened else {
            return;
        };
        if index < Self::rows(menu, snapshot) {
            self.cursor = index;
        }
    }

    fn dismiss(&mut self) {
        self.opened = None;
        self.cursor = 0;
    }
}

#[derive(Component, Default)]
struct PaletteMcp(McpServers);

#[derive(Component)]
struct PaletteMcpActive;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PaletteProjectionSet {
    Project,
    Context,
    Publish,
}

#[derive(Clone, Copy)]
enum PaletteKey {
    Next,
    Previous,
    Complete,
    Dismiss,
}

#[derive(Component)]
struct PaletteKeyBinding(PaletteKey);

impl CommandBinding for PaletteKeyBinding {
    fn for_command(id: &str) -> Option<Self> {
        match id {
            "command_bar_next" => Some(Self(PaletteKey::Next)),
            "command_bar_previous" => Some(Self(PaletteKey::Previous)),
            "command_bar_complete" => Some(Self(PaletteKey::Complete)),
            "command_bar_dismiss" => Some(Self(PaletteKey::Dismiss)),
            _ => None,
        }
    }
}

#[vmux_command::command(id = "command_bar_submit")]
#[derive(Component)]
struct PaletteSubmitBinding;

#[vmux_command::command(id = "command_bar_menu_next")]
#[derive(Component)]
struct PaletteMenuNextBinding;

#[vmux_command::command(id = "command_bar_menu_previous")]
#[derive(Component)]
struct PaletteMenuPreviousBinding;

#[vmux_command::command(id = "command_bar_menu_choose")]
#[derive(Component)]
struct PaletteMenuChooseBinding;

#[vmux_command::command(id = "command_bar_menu_dismiss")]
#[derive(Component)]
struct PaletteMenuDismissBinding;

#[derive(EntityEvent)]
struct PaletteDecisionReady {
    #[event_target]
    target: Entity,
    decision: PaletteDecision,
}

fn bind_commands(registry: CommandRegistry, mut commands: Commands) {
    registry.bind::<PaletteKeyBinding>(&mut commands);
    registry.bind::<PaletteSubmitBinding>(&mut commands);
    registry.bind::<PaletteMenuNextBinding>(&mut commands);
    registry.bind::<PaletteMenuPreviousBinding>(&mut commands);
    registry.bind::<PaletteMenuChooseBinding>(&mut commands);
    registry.bind::<PaletteMenuDismissBinding>(&mut commands);
}

fn attach_snapshot(
    pages: Query<
        Entity,
        (
            Or<(With<RendersLauncherPanel>, With<HostsLauncher>)>,
            Without<PaletteSnapshot>,
        ),
    >,
    mut commands: Commands,
) {
    for page in &pages {
        commands.entity(page).insert((
            PaletteSnapshot::default(),
            PaletteOpen::default(),
            PaletteContext::default(),
            PaletteDraftInput::default(),
            PaletteMenuState::default(),
            PaletteMcp::default(),
            UiState::<CommandPaletteState>::default(),
        ));
    }
}

fn receive_mcp_servers(
    trigger: On<UiStateWrite<McpServers>>,
    mut palettes: Query<&mut PaletteMcp>,
) {
    let Ok(mut mcp) = palettes.get_mut(trigger.event().webview()) else {
        return;
    };
    mcp.0.clone_from(trigger.event().update());
}

fn receive_open(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(
        &mut PaletteOpen,
        &mut PaletteDraftInput,
        &mut PaletteMenuState,
        &mut PaletteSnapshot,
    )>,
) {
    let Some(opened) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<CommandBarOpenEvent>>::payload(
            trigger.event().patch(),
        )
    else {
        return;
    };
    let Ok((mut current, mut draft, mut menu, mut snapshot)) =
        palettes.get_mut(trigger.event().webview())
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
    menu.dismiss();
    draft.input_revision = draft.input_revision.wrapping_add(1).max(1);
    draft.close_revision = 0;
    snapshot.0.open_id = opened.open_id;
    snapshot.0.projection = Default::default();
}

fn update_draft(
    trigger: On<UiInput<CommandPaletteDraftRequest>>,
    mut palettes: Query<(&PaletteOpen, &mut PaletteDraftInput, &mut PaletteMenuState)>,
    active: Query<(), With<PaletteMcpActive>>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, mut draft, mut menu)) = palettes.get_mut(target) else {
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
        menu.dismiss();
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

fn move_history(
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
    input.move_history(&snapshot.0.prompt_history, request.older);
}

fn toggle_menu(
    trigger: On<UiInput<CommandPaletteMenuToggleRequest>>,
    mut palettes: Query<(&PaletteOpen, &PaletteSnapshot, &mut PaletteMenuState)>,
) {
    let Ok((opened, snapshot, mut menu)) = palettes.get_mut(trigger.event().webview) else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    menu.toggle(request.menu, &snapshot.0);
}

fn move_menu_request(
    trigger: On<UiInput<CommandPaletteMenuMoveRequest>>,
    mut palettes: Query<(&PaletteOpen, &PaletteSnapshot, &mut PaletteMenuState)>,
) {
    let Ok((opened, snapshot, mut menu)) = palettes.get_mut(trigger.event().webview) else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    menu.step(request.next, &snapshot.0);
}

fn highlight_menu(
    trigger: On<UiInput<CommandPaletteMenuHighlightRequest>>,
    mut palettes: Query<(&PaletteOpen, &PaletteSnapshot, &mut PaletteMenuState)>,
) {
    let Ok((opened, snapshot, mut menu)) = palettes.get_mut(trigger.event().webview) else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    menu.highlight(request.index as usize, &snapshot.0);
}

fn activate_menu(
    trigger: On<UiInput<CommandPaletteMenuActivateRequest>>,
    mut palettes: Query<(
        &PaletteOpen,
        &mut PaletteDraftInput,
        &mut PaletteMenuState,
        &PaletteSnapshot,
    )>,
    mut commands: Commands,
) {
    let target = trigger.event().webview;
    let Ok((opened, mut input, mut menu, snapshot)) = palettes.get_mut(target) else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    let Some(kind) = menu.opened else {
        return;
    };
    let index = request.index as usize;
    let composer = &snapshot.0.projection.composer;
    let handled = match kind {
        CommandPaletteMenu::Agent => {
            let Some(agent) = composer.agents.get(index) else {
                return;
            };
            input.target_url.clone_from(&agent.url);
            input.selected = 0;
            input.navigating = false;
            input.input_revision = input.input_revision.wrapping_add(1).max(1);
            true
        }
        CommandPaletteMenu::Model => {
            let Some(model) = composer.model_options.get(index) else {
                return;
            };
            commands.trigger(UiInput {
                webview: target,
                payload: StartSelectModel {
                    agent_key: composer.model_agent_key.clone(),
                    model_id: model.id.clone(),
                },
            });
            true
        }
        CommandPaletteMenu::Permission => {
            let Some(mode) = composer.permission_modes.get(index) else {
                return;
            };
            commands.trigger(UiInput {
                webview: target,
                payload: StartSelectMode {
                    agent_key: composer.permission_agent_key.clone(),
                    mode_id: mode.id.clone(),
                },
            });
            true
        }
        CommandPaletteMenu::Project => {
            let projects = composer
                .projects
                .iter()
                .filter(|project| project.depth == 0);
            let count = projects.clone().count();
            if index == count {
                commands.trigger(UiInput {
                    webview: target,
                    payload: StartSelectWorkspace {
                        current_dir: composer.cwd.clone(),
                    },
                });
                true
            } else {
                let Some(project) = projects.into_iter().nth(index) else {
                    return;
                };
                commands.trigger(UiInput {
                    webview: target,
                    payload: StartGoToBranch {
                        project: project.path.clone(),
                        branch: String::new(),
                        checkout: String::new(),
                    },
                });
                true
            }
        }
        CommandPaletteMenu::Branch => {
            let Some(branch) = snapshot.0.branches.get(index) else {
                return;
            };
            commands.trigger(UiInput {
                webview: target,
                payload: StartGoToBranch {
                    project: composer.project.clone(),
                    branch: branch.branch.clone(),
                    checkout: branch.checkout.clone(),
                },
            });
            true
        }
    };
    if handled {
        menu.dismiss();
    }
}

fn dismiss_menu_request(
    trigger: On<UiInput<CommandPaletteMenuDismissRequest>>,
    mut palettes: Query<(&PaletteOpen, &mut PaletteMenuState)>,
) {
    let Ok((opened, mut menu)) = palettes.get_mut(trigger.event().webview) else {
        return;
    };
    if trigger.event().payload.open_id != opened.0.open_id {
        return;
    }
    menu.dismiss();
}

fn dispatch_submit(
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
        true => PaletteSurface::Start,
        false => PaletteSurface::Modal,
    };
    let palette = PaletteState::from_rows(&rows, &opened.0, &draft, surface);
    let decision = match surface {
        PaletteSurface::Start => palette.submit_start(&snapshot.0.attachments),
        PaletteSurface::Modal => palette.submit_modal(&snapshot.0.attachments),
    };
    commands.trigger(PaletteDecisionReady { target, decision });
}

fn activate_row(
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
        true => PaletteSurface::Start,
        false => PaletteSurface::Modal,
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

fn apply_decision(
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

fn apply_key(
    trigger: On<CommandDispatch>,
    keys: Query<&PaletteKeyBinding>,
    mut palettes: Query<(&mut PaletteDraftInput, &PaletteSnapshot)>,
    mut commands: Commands,
) {
    let Ok(key) = keys.get(trigger.event().command()) else {
        return;
    };
    let target = trigger.event().invocation().caller;
    let Ok((mut input, snapshot)) = palettes.get_mut(target) else {
        return;
    };
    if snapshot.0.projection.mcp_open {
        match key.0 {
            PaletteKey::Next => {
                input.selected = (input.selected + 1)
                    .min(snapshot.0.projection.mcp_entries.len().saturating_sub(1));
                input.navigating = true;
            }
            PaletteKey::Previous => {
                input.selected = input.selected.saturating_sub(1);
                input.navigating = true;
            }
            PaletteKey::Complete => return,
            PaletteKey::Dismiss => {
                input.query.clear();
                input.selected = 0;
                input.navigating = false;
            }
        }
        input.input_revision = input.input_revision.wrapping_add(1).max(1);
        return;
    }
    match key.0 {
        PaletteKey::Next => {
            input.selected =
                (input.selected + 1).min(snapshot.0.projection.rows.len().saturating_sub(1));
            input.navigating = true;
        }
        PaletteKey::Previous => {
            input.selected = input.selected.saturating_sub(1);
            input.navigating = true;
        }
        PaletteKey::Complete => {
            if snapshot.0.projection.ghost.is_empty() {
                return;
            }
            input.query.push_str(&snapshot.0.projection.ghost);
            input.selected = 0;
            input.navigating = false;
        }
        PaletteKey::Dismiss => {
            input.close_revision = input.close_revision.wrapping_add(1).max(1);
            commands.trigger(CommandBarDismiss::new(target, true));
            return;
        }
    }
    input.input_revision = input.input_revision.wrapping_add(1).max(1);
}

fn submit(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<PaletteSubmitBinding>>,
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

fn move_menu(
    trigger: On<CommandDispatch>,
    next: Query<(), With<PaletteMenuNextBinding>>,
    previous: Query<(), With<PaletteMenuPreviousBinding>>,
    palettes: Query<&PaletteOpen>,
    mut commands: Commands,
) {
    let command = trigger.event().command();
    let next = if next.contains(command) {
        true
    } else if previous.contains(command) {
        false
    } else {
        return;
    };
    let target = trigger.event().invocation().caller;
    let Ok(opened) = palettes.get(target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: target,
        payload: CommandPaletteMenuMoveRequest {
            open_id: opened.0.open_id,
            next,
        },
    });
}

fn choose_menu(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<PaletteMenuChooseBinding>>,
    palettes: Query<(&PaletteOpen, &PaletteMenuState)>,
    mut commands: Commands,
) {
    if !bindings.contains(trigger.event().command()) {
        return;
    }
    let target = trigger.event().invocation().caller;
    let Ok((opened, menu)) = palettes.get(target) else {
        return;
    };
    commands.trigger(UiInput {
        webview: target,
        payload: CommandPaletteMenuActivateRequest {
            open_id: opened.0.open_id,
            index: menu.cursor as u32,
        },
    });
}

fn dismiss_menu(
    trigger: On<CommandDispatch>,
    bindings: Query<(), With<PaletteMenuDismissBinding>>,
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
        payload: CommandPaletteMenuDismissRequest {
            open_id: opened.0.open_id,
        },
    });
}

fn project(
    mut palettes: Query<(
        &PaletteOpen,
        &PaletteDraftInput,
        &PaletteMenuState,
        &PaletteMcp,
        &mut PaletteContext,
        &mut PaletteSnapshot,
    )>,
) {
    for (opened, input, menu, mcp, mut context, mut snapshot) in &mut palettes {
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
            true => PaletteSurface::Start,
            false => PaletteSurface::Modal,
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
        projection.menu = menu.opened;
        projection.menu_cursor = menu.cursor as u32;
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

fn publish_snapshot(
    snapshots: Query<(Entity, &PaletteSnapshot), Changed<PaletteSnapshot>>,
    mut commands: Commands,
) {
    for (target, snapshot) in &snapshots {
        commands.trigger(UiStateWrite::<CommandPaletteState>::from_event(
            target,
            &snapshot.0,
        ));
    }
}

fn detach_snapshot(
    pages: Query<
        Entity,
        (
            With<PaletteSnapshot>,
            Without<RendersLauncherPanel>,
            Without<HostsLauncher>,
        ),
    >,
    mut commands: Commands,
) {
    for page in &pages {
        commands.entity(page).remove::<(
            PaletteSnapshot,
            PaletteOpen,
            PaletteContext,
            PaletteDraftInput,
            PaletteMcp,
            UiState<CommandPaletteState>,
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

fn keep_frames_coming(
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
mod tests {
    use super::*;
    use crate::CommandInvocation;
    use vmux_api::command_bar::{CommandBarCommandEntry, CommandBarResultItem, InvokeRequest};
    use vmux_api::mcp::{McpServerEntry, McpServerStatus};

    #[derive(Component, Default)]
    struct CapturedInvocations(Vec<InvokeRequest>);

    #[derive(Component, Default)]
    struct CapturedMcpSnapshots(u32);

    fn capture_invocation(
        trigger: On<UiInput<InvokeRequest>>,
        mut captured: Query<&mut CapturedInvocations>,
    ) {
        let Ok(mut captured) = captured.get_mut(trigger.event().webview) else {
            return;
        };
        captured.0.push(trigger.event().payload.clone());
    }

    fn capture_mcp_snapshot(
        trigger: On<McpSnapshotRequest>,
        mut captured: Query<&mut CapturedMcpSnapshots>,
    ) {
        let Ok(mut captured) = captured.get_mut(trigger.event().target) else {
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
            PaletteSnapshot(CommandPaletteState {
                open_id,
                ..Default::default()
            }),
        ));

        app.update();

        let snapshot = app.world().get::<PaletteSnapshot>(page).unwrap();
        assert!(snapshot.0.projection.rows.iter().any(
            |row| matches!(row, CommandBarResultItem::Command { id, .. } if id == "close_tab")
        ));
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
            PaletteSnapshot(CommandPaletteState {
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
            },
            vmux_api::command_bar::CommandPaletteAgent {
                url: "vmux://sessions/codex/".to_string(),
                title: "Codex".to_string(),
            },
        ];
        app.world_mut()
            .get_mut::<PaletteMenuState>(page)
            .unwrap()
            .opened = Some(CommandPaletteMenu::Agent);
        app.world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .write(CommandInvocation::new(page, "command_bar_menu_next"));
        app.update();

        let menu = app.world().get::<PaletteMenuState>(page).unwrap();
        assert_eq!(menu.opened, Some(CommandPaletteMenu::Agent));
        assert_eq!(menu.cursor, 1);
    }

    #[test]
    fn palette_history_is_recalled_in_host_state() {
        let open_id = OpenId(11);
        let mut app = App::new();
        app.add_observer(move_history);
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
                PaletteSnapshot(CommandPaletteState {
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
    fn mcp_filter_and_key_selection_stay_in_host_state() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_plugins(PalettePlugin);
        let page = app.world_mut().spawn(HostsLauncher).id();
        app.update();

        let open_id = OpenId(9);
        app.world_mut().entity_mut(page).insert((
            PaletteOpen(CommandBarOpenEvent {
                open_id,
                ..Default::default()
            }),
            PaletteDraftInput {
                open_id,
                query: "/mcp lin".to_string(),
                ..Default::default()
            },
            PaletteSnapshot(CommandPaletteState {
                open_id,
                ..Default::default()
            }),
        ));
        app.world_mut()
            .trigger(UiStateWrite::<McpServers>::from_event(
                page,
                &McpServers {
                    loaded: true,
                    servers: vec![
                        McpServerEntry {
                            id: "linear".to_string(),
                            name: "Linear".to_string(),
                            description: String::new(),
                            status: McpServerStatus::Connected,
                        },
                        McpServerEntry {
                            id: "github".to_string(),
                            name: "GitHub".to_string(),
                            description: String::new(),
                            status: McpServerStatus::Available,
                        },
                    ],
                    ..Default::default()
                },
            ));
        app.update();

        let snapshot = app.world().get::<PaletteSnapshot>(page).unwrap();
        assert!(snapshot.0.projection.mcp_open);
        assert_eq!(snapshot.0.projection.mcp_entries.len(), 1);
        assert_eq!(snapshot.0.projection.mcp_entries[0].id, "linear");

        app.world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .write(CommandInvocation::new(page, "command_bar_dismiss"));
        app.update();

        let input = app.world().get::<PaletteDraftInput>(page).unwrap();
        assert!(input.query.is_empty());
        assert_eq!(input.input_revision, 1);
    }

    #[test]
    fn entering_mcp_mode_requests_one_tool_snapshot() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_plugins(PalettePlugin)
            .add_observer(capture_mcp_snapshot);
        let page = app
            .world_mut()
            .spawn((HostsLauncher, CapturedMcpSnapshots::default()))
            .id();
        app.update();

        let open_id = OpenId(10);
        app.world_mut().entity_mut(page).insert((
            PaletteOpen(CommandBarOpenEvent {
                open_id,
                ..Default::default()
            }),
            PaletteDraftInput {
                open_id,
                ..Default::default()
            },
        ));
        for query in ["/mcp", "/mcp linear"] {
            app.world_mut().trigger(UiInput {
                webview: page,
                payload: CommandPaletteDraftRequest {
                    open_id,
                    query: query.to_string(),
                    ..Default::default()
                },
            });
            app.update();
        }

        assert_eq!(app.world().get::<CapturedMcpSnapshots>(page).unwrap().0, 1);
        assert!(app.world().get::<PaletteMcpActive>(page).is_some());
    }

    #[test]
    fn submission_dispatches_the_projected_row_in_host_ecs() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<bevy_cef::prelude::BinIpcEventRawBuffer>()
            .add_plugins(PalettePlugin)
            .add_observer(capture_invocation);
        let page = app
            .world_mut()
            .spawn((HostsLauncher, CapturedInvocations::default()))
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
            PaletteMcp::default(),
            PaletteSnapshot(CommandPaletteState {
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
    }
}
