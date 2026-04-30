use bevy::{
    ecs::relationship::Relationship, picking::Pickable, prelude::*, render::alpha::AlphaMode,
};
use bevy_cef::prelude::*;
use vmux_history::LastActivatedAt;
use vmux_processes::event::*;
use vmux_service::protocol::{ClientMessage, ProcessId};
use vmux_webview_app::UiReady;

use crate::{
    browser::Browser,
    layout::{
        pane::{Pane, PaneSplit},
        space::Space,
        tab::{Tab, focused_tab, tab_bundle},
        window::WEBVIEW_MESH_DEPTH_BIAS,
    },
    terminal::{ServiceClient, ServiceProcessHandle, Terminal},
};

/// Marker for the processes monitor webview entity.
#[derive(Component)]
pub(crate) struct ProcessesMonitor;

impl ProcessesMonitor {
    /// Create a processes monitor webview bundle.
    pub(crate) fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(PROCESSES_WEBVIEW_URL),
                ResolvedWebviewUri(PROCESSES_WEBVIEW_URL.to_string()),
                vmux_header::PageMetadata {
                    title: "Background Services".to_string(),
                    url: PROCESSES_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                },
                Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                    Vec3::Z,
                    Vec2::splat(0.5),
                ))),
            ),
            (
                MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                    base: StandardMaterial {
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        depth_bias: WEBVIEW_MESH_DEPTH_BIAS,
                        ..default()
                    },
                    ..default()
                })),
                WebviewSize(Vec2::new(1280.0, 720.0)),
                Transform::default(),
                GlobalTransform::default(),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                Visibility::Inherited,
                Pickable::default(),
            ),
        )
    }
}

/// Cached process list from the service, updated via ListProcesses responses.
/// Written by terminal.rs's poll_service_messages, read by this module.
#[derive(Resource, Default)]
pub(crate) struct ServiceProcessList {
    pub processes: Vec<vmux_service::protocol::ProcessInfo>,
}

/// Timer for periodic process list polling.
#[derive(Resource)]
struct ProcessesPollTimer(Timer);

pub(crate) struct ProcessesMonitorPlugin;

impl Plugin for ProcessesMonitorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ServiceProcessList>()
            .insert_resource(ProcessesPollTimer(Timer::from_seconds(
                1.0,
                TimerMode::Repeating,
            )))
            .add_plugins(JsEmitEventPlugin::<ProcessNavigateEvent>::default())
            .add_plugins(JsEmitEventPlugin::<ProcessKillEvent>::default())
            .add_plugins(JsEmitEventPlugin::<ProcessKillAllEvent>::default())
            .add_systems(
                Update,
                (request_process_list, broadcast_to_monitors).chain(),
            )
            .add_observer(on_process_navigate)
            .add_observer(on_process_kill)
            .add_observer(on_process_kill_all);
    }
}

/// Periodically send ListProcesses to the service.
fn request_process_list(
    time: Res<Time>,
    mut timer: ResMut<ProcessesPollTimer>,
    service: Option<Res<ServiceClient>>,
    monitors: Query<(), With<ProcessesMonitor>>,
) {
    if monitors.is_empty() {
        return;
    }
    timer.0.tick(time.delta());
    if timer.0.just_finished()
        && let Some(service) = service
    {
        service.0.send(ClientMessage::ListProcesses);
    }
}

/// Broadcast the cached process list to all process monitor webviews.
fn broadcast_to_monitors(
    process_list: Res<ServiceProcessList>,
    service: Option<Res<ServiceClient>>,
    monitors: Query<Entity, (With<ProcessesMonitor>, With<UiReady>)>,
    browsers: NonSend<Browsers>,
    terminal_handles: Query<&ServiceProcessHandle, With<Terminal>>,
    mut commands: Commands,
) {
    if monitors.is_empty() || !process_list.is_changed() {
        return;
    }

    let connected = service.is_some();

    // Build attached set from local terminal handles
    let attached_ids: std::collections::HashSet<String> = terminal_handles
        .iter()
        .map(|h| h.process_id.to_string())
        .collect();

    let processes: Vec<ProcessEntry> = process_list
        .processes
        .iter()
        .map(|info| ProcessEntry {
            id: info.id.to_string(),
            shell: info.shell.clone(),
            cwd: info.cwd.clone(),
            cols: info.cols,
            rows: info.rows,
            pid: info.pid,
            uptime_secs: info.created_at_secs,
            attached: attached_ids.contains(&info.id.to_string()),
            preview_lines: Vec::new(), // TODO: add preview from snapshot
        })
        .collect();

    let event = ProcessesListEvent {
        connected,
        processes,
    };

    for entity in &monitors {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(HostEmitEvent::new(entity, PROCESSES_LIST_EVENT, &event));
        }
    }
}

/// Navigate to the terminal tab for the clicked process, or open a new one.
fn on_process_navigate(
    trigger: On<Receive<ProcessNavigateEvent>>,
    terminals: Query<(Entity, &ServiceProcessHandle, &ChildOf), With<Terminal>>,
    tab_parent: Query<&ChildOf, With<Tab>>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut commands: Commands,
) {
    let pid = &trigger.event().payload.process_id;

    // If a tab already has this process attached, activate it
    for (_, handle, content_child_of) in &terminals {
        if handle.process_id.to_string() == *pid {
            let tab = content_child_of.get();
            commands.entity(tab).insert(LastActivatedAt::now());
            if let Ok(tab_child_of) = tab_parent.get(tab) {
                commands
                    .entity(tab_child_of.get())
                    .insert(LastActivatedAt::now());
            }
            return;
        }
    }

    // No existing tab — open a new one with reattach
    let Ok(process_id) = pid.parse::<ProcessId>() else {
        warn!("Invalid process ID from navigate event: {pid}");
        return;
    };
    let (_, active_pane, _) = focused_tab(
        &spaces,
        &all_children,
        &leaf_panes,
        &pane_ts,
        &pane_children,
        &tab_ts,
    );
    let Some(pane) = active_pane else { return };

    let tab = commands
        .spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    commands.spawn((
        Terminal::reattach(&mut meshes, &mut webview_mt, process_id),
        ChildOf(tab),
    ));
}

/// Kill a single service-managed process and close the associated terminal tab if any.
fn on_process_kill(
    trigger: On<Receive<ProcessKillEvent>>,
    service: Option<Res<ServiceClient>>,
    terminals: Query<(Entity, &ServiceProcessHandle, &ChildOf), With<Terminal>>,
    tab_parent: Query<&ChildOf, With<Tab>>,
    mut commands: Commands,
) {
    let Some(service) = service else { return };
    let pid = &trigger.event().payload.process_id;

    if let Ok(process_id) = pid.parse::<ProcessId>() {
        service.0.send(ClientMessage::KillProcess { process_id });

        // Close the terminal tab that owns this process
        for (_, handle, content_child_of) in &terminals {
            if handle.process_id == process_id {
                let tab = content_child_of.get();
                // Only despawn if it's actually a tab
                if tab_parent.get(tab).is_ok() || commands.get_entity(tab).is_ok() {
                    commands.entity(tab).despawn();
                }
                break;
            }
        }
    }
}

/// Kill all service-managed processes and close their terminal tabs.
fn on_process_kill_all(
    _trigger: On<Receive<ProcessKillAllEvent>>,
    service: Option<Res<ServiceClient>>,
    process_list: Res<ServiceProcessList>,
    terminals: Query<(Entity, &ServiceProcessHandle, &ChildOf), With<Terminal>>,
    mut commands: Commands,
) {
    let Some(service) = service else { return };

    for info in &process_list.processes {
        service.0.send(ClientMessage::KillProcess {
            process_id: info.id,
        });

        // Close the terminal tab
        for (_, handle, content_child_of) in &terminals {
            if handle.process_id == info.id {
                let tab = content_child_of.get();
                commands.entity(tab).despawn();
                break;
            }
        }
    }
}
