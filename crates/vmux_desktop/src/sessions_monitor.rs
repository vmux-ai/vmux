use bevy::{ecs::relationship::Relationship, picking::Pickable, prelude::*, render::alpha::AlphaMode};
use bevy_cef::prelude::*;
use vmux_daemon::protocol::{ClientMessage, SessionId};
use vmux_history::LastActivatedAt;
use vmux_sessions::event::*;
use vmux_webview_app::UiReady;

use crate::{
    browser::Browser,
    layout::{
        pane::{Pane, PaneSplit},
        space::Space,
        tab::{Tab, focused_tab, tab_bundle},
        window::WEBVIEW_MESH_DEPTH_BIAS,
    },
    terminal::{DaemonClient, DaemonSessionHandle, Terminal},
};

/// Marker for the sessions monitor webview entity.
#[derive(Component)]
pub(crate) struct SessionsMonitor;

impl SessionsMonitor {
    /// Create a sessions monitor webview bundle.
    pub(crate) fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                WebviewSource::new(SESSIONS_WEBVIEW_URL),
                ResolvedWebviewUri(SESSIONS_WEBVIEW_URL.to_string()),
                vmux_header::PageMetadata {
                    title: "Sessions".to_string(),
                    url: SESSIONS_WEBVIEW_URL.to_string(),
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

/// Cached session list from the daemon, updated via ListSessions responses.
/// Written by terminal.rs's poll_daemon_messages, read by this module.
#[derive(Resource, Default)]
pub(crate) struct DaemonSessionList {
    pub sessions: Vec<vmux_daemon::protocol::SessionInfo>,
}

/// Timer for periodic session list polling.
#[derive(Resource)]
struct SessionsPollTimer(Timer);

pub(crate) struct SessionsMonitorPlugin;

impl Plugin for SessionsMonitorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DaemonSessionList>()
            .insert_resource(SessionsPollTimer(Timer::from_seconds(
                1.0,
                TimerMode::Repeating,
            )))
            .add_plugins(JsEmitEventPlugin::<SessionNavigateEvent>::default())
            .add_plugins(JsEmitEventPlugin::<SessionKillEvent>::default())
            .add_plugins(JsEmitEventPlugin::<SessionKillAllEvent>::default())
            .add_systems(Update, (request_session_list, broadcast_to_monitors).chain())
            .add_observer(on_session_navigate)
            .add_observer(on_session_kill)
            .add_observer(on_session_kill_all);
    }
}

/// Periodically send ListSessions to the daemon.
fn request_session_list(
    time: Res<Time>,
    mut timer: ResMut<SessionsPollTimer>,
    daemon: Option<Res<DaemonClient>>,
    monitors: Query<(), With<SessionsMonitor>>,
) {
    if monitors.is_empty() {
        return;
    }
    timer.0.tick(time.delta());
    if timer.0.just_finished() {
        if let Some(daemon) = daemon {
            daemon.0.send(ClientMessage::ListSessions);
        }
    }
}

/// Broadcast the cached session list to all sessions monitor webviews.
fn broadcast_to_monitors(
    session_list: Res<DaemonSessionList>,
    daemon: Option<Res<DaemonClient>>,
    monitors: Query<Entity, (With<SessionsMonitor>, With<UiReady>)>,
    browsers: NonSend<Browsers>,
    terminal_handles: Query<&DaemonSessionHandle, With<Terminal>>,
    mut commands: Commands,
) {
    if monitors.is_empty() || !session_list.is_changed() {
        return;
    }

    let connected = daemon.is_some();

    // Build attached set from local terminal handles
    let attached_ids: std::collections::HashSet<String> = terminal_handles
        .iter()
        .map(|h| h.session_id.to_string())
        .collect();

    let sessions: Vec<SessionEntry> = session_list
        .sessions
        .iter()
        .map(|info| SessionEntry {
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

    let event = SessionsListEvent {
        connected,
        sessions,
    };

    for entity in &monitors {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(HostEmitEvent::new(entity, SESSIONS_LIST_EVENT, &event));
        }
    }
}

/// Navigate to the terminal tab for the clicked session, or open a new one.
fn on_session_navigate(
    trigger: On<Receive<SessionNavigateEvent>>,
    terminals: Query<(Entity, &DaemonSessionHandle, &ChildOf), With<Terminal>>,
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
    let sid = &trigger.event().payload.session_id;

    // If a tab already has this session attached, activate it
    for (_, handle, content_child_of) in &terminals {
        if handle.session_id.to_string() == *sid {
            let tab = content_child_of.get();
            commands.entity(tab).insert(LastActivatedAt::now());
            if let Ok(tab_child_of) = tab_parent.get(tab) {
                commands.entity(tab_child_of.get()).insert(LastActivatedAt::now());
            }
            return;
        }
    }

    // No existing tab — open a new one with reattach
    let Ok(session_id) = sid.parse::<SessionId>() else {
        warn!("Invalid session ID from navigate event: {sid}");
        return;
    };
    let (_, active_pane, _) = focused_tab(
        &spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts,
    );
    let Some(pane) = active_pane else { return };

    let tab = commands
        .spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    commands.spawn((
        Terminal::reattach(&mut meshes, &mut webview_mt, session_id),
        ChildOf(tab),
    ));
}

/// Kill a single daemon session and close the associated terminal tab if any.
fn on_session_kill(
    trigger: On<Receive<SessionKillEvent>>,
    daemon: Option<Res<DaemonClient>>,
    terminals: Query<(Entity, &DaemonSessionHandle, &ChildOf), With<Terminal>>,
    tab_parent: Query<&ChildOf, With<Tab>>,
    mut commands: Commands,
) {
    let Some(daemon) = daemon else { return };
    let sid = &trigger.event().payload.session_id;

    if let Ok(session_id) = sid.parse::<SessionId>() {
        daemon.0.send(ClientMessage::KillSession { session_id });

        // Close the terminal tab that owns this session
        for (_, handle, content_child_of) in &terminals {
            if handle.session_id == session_id {
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

/// Kill all daemon sessions and close their terminal tabs.
fn on_session_kill_all(
    _trigger: On<Receive<SessionKillAllEvent>>,
    daemon: Option<Res<DaemonClient>>,
    session_list: Res<DaemonSessionList>,
    terminals: Query<(Entity, &DaemonSessionHandle, &ChildOf), With<Terminal>>,
    mut commands: Commands,
) {
    let Some(daemon) = daemon else { return };

    for info in &session_list.sessions {
        daemon.0.send(ClientMessage::KillSession {
            session_id: info.id,
        });

        // Close the terminal tab
        for (_, handle, content_child_of) in &terminals {
            if handle.session_id == info.id {
                let tab = content_child_of.get();
                commands.entity(tab).despawn();
                break;
            }
        }
    }
}
