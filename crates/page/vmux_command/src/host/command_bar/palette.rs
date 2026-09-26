use std::time::Duration;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::{
    CommandBarOpenEvent, CommandBarQuery, CommandBarUiState, CommandBarUiStatePatch,
    CommandPaletteActivateRequest, CommandPaletteBranchesRequest, CommandPaletteDraftRequest,
    CommandPalettePromptHistoryRequest, CommandPaletteRemoveAttachmentRequest,
    CommandPaletteSelectionRequest, CommandPaletteState, CommandPaletteSubmitRequest, OpenId,
};
use vmux_api::mcp::{McpServerRequest, McpServers};
use vmux_core::host::{UiState, UiStateWrite};
use vmux_core::launcher::{HostsLauncher, RendersLauncherPanel};
use vmux_ui::launcher::palette::{PaletteDecision, PaletteState};
use vmux_ui::launcher::palette::{PaletteDraft, PaletteRows, PaletteSurface};
use vmux_ui::launcher::results::active_space_index;

use crate::{
    CommandDefinitions, CommandDispatch, CommandRuntimePlugin, RegisterCommandDefinitions,
};

use super::CloseCommandBar;

mod branch;
mod media;
mod prompt;
mod resume;
mod search;

pub struct PalettePlugin;

impl Plugin for PalettePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_plugins((
            UiEventPlugin::<(
                CommandPaletteDraftRequest,
                CommandPaletteSelectionRequest,
                CommandPaletteSubmitRequest,
                CommandPaletteActivateRequest,
                CommandPalettePromptHistoryRequest,
                CommandPaletteBranchesRequest,
                CommandPaletteRemoveAttachmentRequest,
            )>::default(),
            vmux_core::host::UiStatePlugin::<CommandPaletteState>::default(),
            search::PaletteSearchPlugin,
            prompt::PalettePromptPlugin,
            branch::PaletteBranchPlugin,
            media::PaletteMediaPlugin,
            resume::PaletteResumePlugin,
        ))
        .add_observer(receive_palette_open)
        .add_observer(receive_mcp_servers)
        .add_observer(update_palette_draft)
        .add_observer(update_palette_selection)
        .add_observer(submit_palette)
        .add_observer(activate_palette_row)
        .add_observer(apply_palette_decision)
        .add_observer(apply_palette_key)
        .add_systems(
            Startup,
            spawn_palette_commands.in_set(RegisterCommandDefinitions),
        )
        .add_systems(
            PreUpdate,
            (attach_palette_snapshot, detach_palette_snapshot),
        )
        .add_systems(
            PostUpdate,
            (project_palette, publish_palette_snapshot).chain(),
        )
        .add_systems(Last, keep_palette_frames_coming);
    }
}

#[derive(Component, Default)]
struct PaletteSnapshot(CommandPaletteState);

#[derive(Component, Default)]
struct PaletteOpen(CommandBarOpenEvent);

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
}

#[derive(Component, Default)]
struct PaletteMcp(McpServers);

#[derive(Clone, Copy)]
enum PaletteKey {
    Next,
    Previous,
    Complete,
    Dismiss,
}

#[derive(Component)]
struct PaletteKeyBinding(PaletteKey);

#[derive(EntityEvent)]
struct PaletteDecisionReady {
    #[event_target]
    target: Entity,
    decision: PaletteDecision,
}

fn spawn_palette_commands(mut commands: Commands) {
    let mut definitions = CommandDefinitions::from_ron(include_str!("palette.ron"));
    commands.spawn((
        definitions.take("command_bar_next"),
        PaletteKeyBinding(PaletteKey::Next),
    ));
    commands.spawn((
        definitions.take("command_bar_previous"),
        PaletteKeyBinding(PaletteKey::Previous),
    ));
    commands.spawn((
        definitions.take("command_bar_complete"),
        PaletteKeyBinding(PaletteKey::Complete),
    ));
    commands.spawn((
        definitions.take("command_bar_dismiss"),
        PaletteKeyBinding(PaletteKey::Dismiss),
    ));
    definitions.assert_all_registered();
}

fn attach_palette_snapshot(
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
            PaletteDraftInput::default(),
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

fn receive_palette_open(
    trigger: On<UiStateWrite<CommandBarUiState>>,
    mut palettes: Query<(
        &mut PaletteOpen,
        &mut PaletteDraftInput,
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
    draft.selected = if opened.picker.is_some_and(|picker| picker.is_space()) {
        active_space_index(&opened.spaces)
    } else {
        0
    };
    draft.navigating = false;
    draft.input_revision = draft.input_revision.wrapping_add(1).max(1);
    draft.close_revision = 0;
    snapshot.0.open_id = opened.open_id;
    snapshot.0.projection = Default::default();
}

fn update_palette_draft(
    trigger: On<bevy_cef::prelude::UiInput<CommandPaletteDraftRequest>>,
    mut palettes: Query<(&PaletteOpen, &mut PaletteDraftInput)>,
) {
    let Ok((opened, mut draft)) = palettes.get_mut(trigger.event().webview) else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    draft.open_id = request.open_id;
    draft.query.clone_from(&request.query);
    draft.start = request.start;
    draft.target_url.clone_from(&request.target_url);
    draft.selected = request.selected as usize;
    draft.navigating = request.navigating;
}

fn update_palette_selection(
    trigger: On<bevy_cef::prelude::UiInput<CommandPaletteSelectionRequest>>,
    mut palettes: Query<(&PaletteOpen, &mut PaletteDraftInput)>,
) {
    let Ok((opened, mut draft)) = palettes.get_mut(trigger.event().webview) else {
        return;
    };
    let request = &trigger.event().payload;
    if request.open_id != opened.0.open_id {
        return;
    }
    draft.selected = request.selected as usize;
    draft.navigating = request.navigating;
}

fn submit_palette(
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

fn activate_palette_row(
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

fn apply_palette_decision(
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
            commands.trigger(CloseCommandBar::after(target, false));
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

fn apply_palette_key(
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
            commands.trigger(CloseCommandBar::after(target, false));
            return;
        }
    }
    input.input_revision = input.input_revision.wrapping_add(1).max(1);
}

fn project_palette(
    mut palettes: Query<(
        &PaletteOpen,
        &PaletteDraftInput,
        &PaletteMcp,
        &mut PaletteSnapshot,
    )>,
) {
    for (opened, input, mcp, mut snapshot) in &mut palettes {
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
        let mut projection = rows.projection();
        projection.query.clone_from(&input.query);
        if let Some(filter) = CommandBarQuery(&input.query).mcp_filter() {
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
        projection.input_revision = input.input_revision;
        projection.close_revision = input.close_revision;
        if snapshot.0.projection == projection {
            continue;
        }
        snapshot.0.projection = projection;
    }
}

fn publish_palette_snapshot(
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

fn detach_palette_snapshot(
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

fn keep_palette_frames_coming(
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

    fn capture_invocation(
        trigger: On<UiInput<InvokeRequest>>,
        mut captured: Query<&mut CapturedInvocations>,
    ) {
        let Ok(mut captured) = captured.get_mut(trigger.event().webview) else {
            return;
        };
        captured.0.push(trigger.event().payload.clone());
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
        app.add_systems(Update, project_palette);
        let page = app
            .world_mut()
            .spawn((
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
                PaletteMcp::default(),
                PaletteSnapshot(CommandPaletteState {
                    open_id,
                    ..Default::default()
                }),
            ))
            .id();

        app.update();

        let snapshot = app.world().get::<PaletteSnapshot>(page).unwrap();
        assert!(snapshot.0.projection.rows.iter().any(
            |row| matches!(row, CommandBarResultItem::Command { id, .. } if id == "close_tab")
        ));
    }

    #[test]
    fn palette_key_commands_update_host_selection() {
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
