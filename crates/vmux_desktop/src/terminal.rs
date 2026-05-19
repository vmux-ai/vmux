use crate::{
    browser::Browser,
    command::{AppCommand, LayoutCommand, StackCommand, WriteAppCommands},
    processes_monitor::ProcessesMonitor,
    settings::AppSettings,
};
use bevy::{
    ecs::relationship::Relationship,
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
        mouse::{MouseScrollUnit, MouseWheel},
    },
    picking::Pickable,
    prelude::*,
    render::alpha::AlphaMode,
};
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_history::LastActivatedAt;
use vmux_layout::window::WEBVIEW_MESH_DEPTH_BIAS;
use vmux_layout::{CloseRequiresConfirmation, LayoutSpawnRequest};
use vmux_service::{
    client::{ServiceHandle, ServiceWake},
    protocol::{ClientMessage, ProcessId, ServiceMessage},
};
use vmux_terminal::event::*;
use vmux_webview_app::UiReady;

pub(crate) mod launch;
pub(crate) mod pid;

/// Maximum interval between consecutive mouse-down events that count as a
/// multi-click (double, triple).
const MULTI_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(300);
/// Maximum cell distance between consecutive mouse-down points that still
/// counts as a multi-click (jitter tolerance).
const MULTI_CLICK_CELL_TOLERANCE: i32 = 1;

/// Marker component for terminal content entities (analogous to Browser).
#[derive(Component)]
pub(crate) struct Terminal;

/// Marker: service-managed process has exited; tab close is pending.
#[derive(Component)]
pub(crate) struct ProcessExited;

/// Alias for backwards compatibility with close-confirmation code.
pub(crate) type PtyExited = ProcessExited;

/// Check if confirmation is needed based on settings.
pub(crate) fn should_confirm_close(settings: &AppSettings) -> bool {
    settings.terminal.as_ref().is_none_or(|t| t.confirm_close)
}

/// Check if a tab entity has any child terminal that is still running.
pub(crate) fn has_live_terminal(
    tab: Entity,
    children_q: &Query<&Children>,
    terminal_q: &Query<(), (With<Terminal>, Without<ProcessExited>)>,
) -> bool {
    if let Ok(children) = children_q.get(tab) {
        children.iter().any(|child| terminal_q.contains(child))
    } else {
        false
    }
}

/// Show confirmation dialog for quitting with N running terminals.
pub(crate) fn confirm_quit_dialog(count: usize) -> bool {
    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    let msg = if count == 1 {
        "A terminal is still running. Quit anyway?".to_string()
    } else {
        format!("{count} terminals are still running. Quit anyway?")
    };
    let result = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Quit Vmux?")
        .set_description(&msg)
        .set_buttons(MessageButtons::OkCancel)
        .show();
    matches!(result, MessageDialogResult::Ok)
}

/// Bevy resource wrapping the service connection.
#[derive(Resource)]
pub(crate) struct ServiceClient(pub ServiceHandle);

#[derive(Resource, Clone)]
struct ServiceWakeCallback(Option<ServiceWake>);

/// Per-process terminal mode flags, last broadcast by the service.
#[derive(Resource, Default)]
pub(crate) struct TerminalModeMap {
    pub modes: std::collections::HashMap<ProcessId, TerminalModeFlags>,
}

/// Optimistic copy-mode state owned by the desktop. The service confirms
/// asynchronously via `TerminalMode`, but keyboard routing must switch on
/// immediately after the shortcut or first mouse drag.
#[derive(Resource, Default)]
struct LocalCopyModeState {
    active: std::collections::HashSet<ProcessId>,
    input_states: std::collections::HashMap<ProcessId, CopyModeInputState>,
}

#[derive(Default)]
struct CopyModeInputState {
    pending_key: Option<CopyModePendingKey>,
    count: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CopyModePendingKey {
    G,
    FindForward,
    FindBackward,
    TillForward,
    TillBackward,
}

#[derive(Clone, Copy)]
struct CopyModeKeyInput<'a> {
    key: &'a Key,
    key_code: KeyCode,
    ctrl: bool,
    shift: bool,
}

#[cfg(test)]
impl<'a> CopyModeKeyInput<'a> {
    fn new(key: &'a Key, key_code: KeyCode) -> Self {
        Self {
            key,
            key_code,
            ctrl: false,
            shift: false,
        }
    }

    fn shift(key: &'a Key, key_code: KeyCode) -> Self {
        Self {
            shift: true,
            ..Self::new(key, key_code)
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub(crate) struct TerminalModeFlags {
    pub mouse_capture: bool,
    pub copy_mode: bool,
}

/// Triggered to restart the terminal process for a terminal entity.
#[derive(Event)]
pub(crate) struct RestartPty {
    pub entity: Entity,
}

#[derive(Resource)]
struct ServiceConnectRetry {
    timer: Timer,
    next_delay_ms: u64,
    remaining_attempts: u32,
}

impl ServiceConnectRetry {
    fn new() -> Self {
        Self {
            timer: Timer::from_seconds(0.05, TimerMode::Once),
            next_delay_ms: 50,
            remaining_attempts: 6,
        }
    }
}

pub(crate) struct TerminalInputPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ServiceMessageSet;

impl Plugin for TerminalInputPlugin {
    fn build(&self, app: &mut App) {
        let service_wake = service_wake_callback(app);
        if let Some(handle) = ServiceHandle::connect_with_wake(service_wake.clone()) {
            tracing::info!("connected to existing service");
            handle.send(ClientMessage::SubscribeAgentCommands);
            app.insert_resource(ServiceClient(handle));
        } else {
            ensure_service_started();
            app.insert_resource(ServiceConnectRetry::new());
        }
        app.insert_resource(ServiceWakeCallback(service_wake));

        app.init_resource::<MouseSelectionState>()
            .init_resource::<TerminalModeMap>()
            .init_resource::<LocalCopyModeState>()
            .init_resource::<pid::PidToEntity>()
            .add_systems(
                Update,
                (pid::track_pid_inserts, pid::track_pid_removals).chain(),
            )
            .add_systems(
                Update,
                pid::format_terminal_url.after(pid::track_pid_inserts),
            )
            .add_plugins(BinJsEmitEventPlugin::<TermResizeEvent>::default())
            .add_plugins(BinJsEmitEventPlugin::<TermMouseEvent>::default())
            .add_systems(
                PreUpdate,
                (
                    handle_terminal_keyboard.run_if(on_message::<KeyboardInput>),
                    handle_terminal_scroll.run_if(on_message::<MouseWheel>),
                )
                    .after(InputSystems),
            );
        add_terminal_update_systems(app)
            .add_observer(on_term_ready)
            .add_observer(on_term_resize)
            .add_observer(on_term_mouse)
            .add_observer(on_restart_pty)
            .add_observer(on_terminal_removed);
    }
}

fn on_terminal_removed(
    trigger: On<Remove, ProcessId>,
    service: Option<Res<ServiceClient>>,
    pids: Query<&ProcessId>,
) {
    let Some(service) = service else { return };
    let entity = trigger.event_target();
    let Ok(process_id) = pids.get(entity) else {
        return;
    };
    service.0.send(ClientMessage::KillProcess {
        process_id: *process_id,
    });
}

fn add_terminal_update_systems(app: &mut App) -> &mut App {
    app.add_message::<ProcessExitedEvent>()
        .add_systems(Update, respawn_shell_on_vibe_exit)
        .add_systems(
            Update,
            spawn_layout_requested_content.after(vmux_layout::stack::StackCommandSet),
        )
        .add_systems(
            Update,
            (
                try_connect_service.run_if(resource_exists::<ServiceConnectRetry>),
                poll_service_messages
                    .in_set(WriteAppCommands)
                    .in_set(ServiceMessageSet),
                flush_pending_terminal_input,
                handle_terminal_copy_mode_command.in_set(crate::command::ReadAppCommands),
                sync_terminal_theme,
            )
                .chain(),
        )
}

fn spawn_layout_requested_content(
    mut reader: MessageReader<LayoutSpawnRequest>,
    settings: Res<AppSettings>,
    active_space: Res<crate::spaces::ActiveSpace>,
    strategies: Option<Res<vmux_agent::strategy::AgentStrategies>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for request in reader.read() {
        match request {
            LayoutSpawnRequest::Terminal { stack } => {
                let cwd = crate::agent::space_dir(&active_space.record.id);
                let terminal = commands
                    .spawn((
                        Terminal::new_with_cwd(&mut meshes, &mut webview_mt, &settings, Some(&cwd)),
                        ChildOf(*stack),
                    ))
                    .id();
                commands.entity(terminal).insert(CefKeyboardTarget);
            }
            LayoutSpawnRequest::ProcessesMonitor { stack } => {
                commands.spawn((
                    ProcessesMonitor::new(&mut meshes, &mut webview_mt),
                    ChildOf(*stack),
                ));
            }
            LayoutSpawnRequest::OpenUrl { stack, url } => {
                spawn_url_into_stack(
                    *stack,
                    url,
                    strategies.as_deref(),
                    &mut commands,
                    &mut meshes,
                    &mut webview_mt,
                    &settings,
                );
            }
        }
    }
}

fn spawn_url_into_stack(
    stack: Entity,
    url: &str,
    strategies: Option<&vmux_agent::strategy::AgentStrategies>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) {
    if url.starts_with(vmux_terminal::event::TERMINAL_WEBVIEW_URL) {
        let terminal = commands
            .spawn((Terminal::new(meshes, webview_mt, settings), ChildOf(stack)))
            .id();
        commands.entity(terminal).insert(CefKeyboardTarget);
    } else if let Some(kind) = vmux_agent::AgentKind::all()
        .into_iter()
        .find(|k| url.starts_with(&k.cli_url_prefix()))
    {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
        let id_part = url.strip_prefix(&kind.cli_url_prefix()).unwrap_or("");
        let session_id = (!id_part.is_empty()).then(|| id_part.to_string());
        let strats = match strategies {
            Some(s) => s,
            None => {
                bevy::log::warn!("agent strategies not registered; falling back to terminal");
                let terminal = commands
                    .spawn((Terminal::new(meshes, webview_mt, settings), ChildOf(stack)))
                    .id();
                commands.entity(terminal).insert(CefKeyboardTarget);
                return;
            }
        };
        if let Err(e) = spawn_agent_into_stack(
            kind, stack, cwd, session_id, strats, commands, meshes, webview_mt, settings,
        ) {
            bevy::log::warn!("agent spawn ({kind:?}) failed: {e}; falling back to terminal");
            let terminal = commands
                .spawn((Terminal::new(meshes, webview_mt, settings), ChildOf(stack)))
                .id();
            commands.entity(terminal).insert(CefKeyboardTarget);
        }
    } else if url.starts_with(vmux_layout::event::SERVICES_WEBVIEW_URL) {
        commands.spawn((ProcessesMonitor::new(meshes, webview_mt), ChildOf(stack)));
    } else if url.starts_with(vmux_space::event::SPACES_WEBVIEW_URL) {
        commands.spawn((
            crate::spaces::SpacesView::new(meshes, webview_mt),
            ChildOf(stack),
        ));
    } else {
        let browser_e = commands
            .spawn((Browser::new(meshes, webview_mt, url), ChildOf(stack)))
            .id();
        commands.entity(browser_e).insert(CefKeyboardTarget);
    }
}

pub(crate) fn spawn_agent_into_stack(
    kind: vmux_agent::AgentKind,
    stack: Entity,
    cwd: std::path::PathBuf,
    session_id: Option<String>,
    strategies: &vmux_agent::strategy::AgentStrategies,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: &AppSettings,
) -> Result<(), String> {
    let launch = crate::agent::build_agent_launch(kind, &cwd, session_id.as_deref(), strategies)?;
    let terminal = commands
        .spawn((
            Terminal::new_with_cwd(meshes, webview_mt, settings, Some(&cwd)),
            ChildOf(stack),
        ))
        .id();
    commands.entity(terminal).insert(CefKeyboardTarget);
    commands
        .entity(terminal)
        .insert((launch, vmux_agent::session::AgentSession { kind }));
    if let Some(id) = session_id {
        commands
            .entity(terminal)
            .insert(vmux_agent::session::SessionId(id));
    } else {
        commands
            .entity(terminal)
            .insert(vmux_agent::session::PendingAgentSession {
                kind,
                spawn_time: std::time::SystemTime::now(),
                cwd,
            });
    }
    Ok(())
}

fn service_wake_callback(app: &App) -> Option<ServiceWake> {
    app.world()
        .get_resource::<bevy::winit::EventLoopProxyWrapper>()
        .map(|wrapper| {
            let proxy = (**wrapper).clone();
            std::sync::Arc::new(move || {
                let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }) as ServiceWake
        })
}

impl Terminal {
    pub(crate) fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        settings: &AppSettings,
    ) -> impl Bundle {
        Self::new_with_cwd(meshes, webview_mt, settings, None)
    }

    pub(crate) fn new_with_cwd(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        settings: &AppSettings,
        cwd: Option<&std::path::Path>,
    ) -> impl Bundle {
        let shell = settings
            .terminal
            .as_ref()
            .map(|t| t.resolve_theme(&t.default_theme).shell)
            .unwrap_or_else(default_shell);

        let cwd_str = cwd
            .filter(|d| !d.to_string_lossy().contains("://"))
            .map(|d| d.to_string_lossy().to_string())
            .unwrap_or_default();

        let launch = crate::terminal::launch::TerminalLaunch {
            command: shell,
            args: vec![],
            cwd: cwd_str,
            env: vec![],
            kind: crate::terminal::launch::TerminalKind::Plain,
        };

        let process_id = ProcessId::new();

        (
            (
                Self,
                Browser,
                CloseRequiresConfirmation,
                process_id,
                launch,
                PendingServiceCreate,
                PageMetadata {
                    title: format!("Terminal ({})", &process_id.to_string()[..8]),
                    url: TERMINAL_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                WebviewSource::new(TERMINAL_WEBVIEW_URL),
                ResolvedWebviewUri(TERMINAL_WEBVIEW_URL.to_string()),
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

    /// Create a terminal bundle that reattaches to an existing service-managed process.
    pub(crate) fn reattach(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        process_id: ProcessId,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                CloseRequiresConfirmation,
                process_id,
                PendingServiceAttach,
                PageMetadata {
                    title: format!("Terminal ({})", &process_id.to_string()[..8]),
                    url: TERMINAL_WEBVIEW_URL.to_string(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
                WebviewSource::new(TERMINAL_WEBVIEW_URL),
                ResolvedWebviewUri(TERMINAL_WEBVIEW_URL.to_string()),
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

#[derive(Component)]
pub(crate) struct PendingServiceCreate;

/// Temporary component: terminal needs an AttachProcess sent to service.
#[derive(Component)]
struct PendingServiceAttach;

#[derive(Component)]
pub(crate) struct PendingTerminalInput {
    pub data: Vec<u8>,
}

/// Marker: CreateProcess was sent, waiting for ProcessCreated response.
#[derive(Component)]
struct AwaitingProcessCreated;

pub(crate) fn apply_process_created(
    commands: &mut Commands,
    entity: Entity,
    process_id: ProcessId,
    process_pid: u32,
) {
    commands
        .entity(entity)
        .insert(process_id)
        .insert(pid::Pid(process_pid))
        .remove::<AwaitingProcessCreated>();
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

struct MissingTerminalRestart {
    entity: Entity,
    new_id: ProcessId,
    command: ClientMessage,
    cwd: String,
    agent_kind: Option<vmux_agent::AgentKind>,
}

fn terminal_shell(settings: &AppSettings) -> String {
    settings
        .terminal
        .as_ref()
        .map(|t| t.resolve_theme(&t.default_theme).shell)
        .unwrap_or_else(default_shell)
}

fn missing_terminal_restart(
    process_id: ProcessId,
    terminals: impl IntoIterator<
        Item = (
            Entity,
            ProcessId,
            crate::terminal::launch::TerminalLaunch,
            Option<vmux_agent::AgentKind>,
        ),
    >,
) -> Option<MissingTerminalRestart> {
    terminals
        .into_iter()
        .find(|(_, terminal_pid, _, _)| *terminal_pid == process_id)
        .map(|(entity, _, launch, agent_kind)| {
            let new_id = ProcessId::new();
            let cwd = launch.cwd.clone();
            MissingTerminalRestart {
                entity,
                new_id,
                command: ClientMessage::CreateProcess {
                    process_id: new_id,
                    command: launch.command,
                    args: launch.args,
                    cwd: launch.cwd,
                    env: launch.env,
                    cols: 80,
                    rows: 24,
                },
                cwd,
                agent_kind,
            }
        })
}

fn missing_process_id(message: &str) -> Option<ProcessId> {
    message
        .strip_prefix("process not found: ")
        .and_then(|id| id.parse().ok())
}

fn ensure_service_started() {
    if ServiceHandle::service_running() {
        tracing::info!("service already running");
        return;
    }
    let binary = match vmux_service::daemon_binary_path() {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "could not locate vmux_service binary");
            return;
        }
    };
    #[cfg(target_os = "macos")]
    {
        let profile = vmux_service::current_profile();
        if let Err(e) = vmux_service::service_registration::ensure_running(profile, &binary) {
            tracing::error!(error = ?e, "service registration failed");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        use std::os::unix::process::CommandExt;
        let log_dir = vmux_service::service_dir();
        let _ = std::fs::create_dir_all(&log_dir);
        let stderr_cfg = match std::fs::File::create(vmux_service::log_path()) {
            Ok(f) => std::process::Stdio::from(f),
            Err(e) => {
                tracing::warn!(error = %e, "could not create service log; stderr will be discarded");
                std::process::Stdio::null()
            }
        };
        let spawn_result = unsafe {
            std::process::Command::new(&binary)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(stderr_cfg)
                .pre_exec(|| {
                    libc::setsid();
                    Ok(())
                })
                .spawn()
        };
        if let Err(e) = spawn_result {
            tracing::error!(error = %e, "failed to spawn vmux_service (non-macOS fallback)");
        }
    }
}

fn broadcast_service_unavailable(
    terminals: &Query<Entity, With<Terminal>>,
    browsers: &NonSend<Browsers>,
    commands: &mut Commands,
    message: String,
) {
    let evt = ServiceUnavailableEvent { message };
    for entity in terminals.iter() {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                SERVICE_UNAVAILABLE_EVENT,
                &evt,
            ));
        }
    }
}

fn try_connect_service(
    mut retry: ResMut<ServiceConnectRetry>,
    time: Res<Time>,
    mut commands: Commands,
    wake: Res<ServiceWakeCallback>,
    terminal_webviews: Query<Entity, With<Terminal>>,
    browsers: NonSend<Browsers>,
) {
    retry.timer.tick(time.delta());
    if !retry.timer.just_finished() {
        return;
    }

    retry.remaining_attempts = retry.remaining_attempts.saturating_sub(1);

    let sock = vmux_service::socket_path();
    if !sock.exists() {
        if retry.remaining_attempts == 0 {
            tracing::warn!("service socket never appeared — giving up");
            commands.remove_resource::<ServiceConnectRetry>();
            broadcast_service_unavailable(
                &terminal_webviews,
                &browsers,
                &mut commands,
                "vmux service unavailable \u{2014} run `vmux service logs` for details.".into(),
            );
        } else {
            retry.next_delay_ms = (retry.next_delay_ms * 2).min(1600);
            retry.timer = Timer::new(
                std::time::Duration::from_millis(retry.next_delay_ms),
                TimerMode::Once,
            );
        }
        return;
    }

    match ServiceHandle::connect_with_wake(wake.0.clone()) {
        Some(handle) => {
            tracing::info!("connected to service after retry");
            handle.send(ClientMessage::SubscribeAgentCommands);
            commands.insert_resource(ServiceClient(handle));
            commands.remove_resource::<ServiceConnectRetry>();
            broadcast_service_unavailable(
                &terminal_webviews,
                &browsers,
                &mut commands,
                String::new(),
            );
        }
        None => {
            if retry.remaining_attempts == 0 {
                tracing::error!("failed to connect to service after all retries");
                let log_path = vmux_service::log_path();
                if let Ok(log) = std::fs::read_to_string(&log_path)
                    && !log.is_empty()
                {
                    tracing::error!(service_log = %log, "service log contents");
                }
                commands.remove_resource::<ServiceConnectRetry>();
                broadcast_service_unavailable(
                    &terminal_webviews,
                    &browsers,
                    &mut commands,
                    "vmux service unavailable \u{2014} run `vmux service logs` for details.".into(),
                );
            } else {
                retry.next_delay_ms = (retry.next_delay_ms * 2).min(1600);
                retry.timer = Timer::new(
                    std::time::Duration::from_millis(retry.next_delay_ms),
                    TimerMode::Once,
                );
            }
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct PollServiceWriters<'w> {
    app_commands: MessageWriter<'w, AppCommand>,
    agent_commands: MessageWriter<'w, crate::agent::AgentCommandRequest>,
    agent_queries: MessageWriter<'w, crate::agent::AgentQueryRequest>,
    process_exited: MessageWriter<'w, ProcessExitedEvent>,
}

fn poll_service_messages(
    pending_create: Query<
        (Entity, &ProcessId, &crate::terminal::launch::TerminalLaunch),
        (With<Terminal>, With<PendingServiceCreate>),
    >,
    pending_attach: Query<(Entity, &ProcessId), (With<Terminal>, With<PendingServiceAttach>)>,
    awaiting_create: Query<
        (Entity, &ProcessId, &ChildOf),
        (With<Terminal>, With<AwaitingProcessCreated>),
    >,
    terminals: Query<
        (Entity, &ProcessId, &ChildOf),
        (
            With<Terminal>,
            Without<ProcessExited>,
            Without<AwaitingProcessCreated>,
        ),
    >,
    service: Option<Res<ServiceClient>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
    mut writers: PollServiceWriters,
    mut mode_map: ResMut<TerminalModeMap>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
    mut mouse_state: ResMut<MouseSelectionState>,
    settings: Res<AppSettings>,
    launches: Query<&crate::terminal::launch::TerminalLaunch>,
    agent_sessions: Query<&vmux_agent::session::AgentSession>,
) {
    let Some(service) = service else { return };

    // Handle pending creates — send CreateProcess, wait for ProcessCreated
    // response which will carry the real process ID.
    for (entity, process_id, launch) in &pending_create {
        service.0.send(ClientMessage::CreateProcess {
            process_id: *process_id,
            command: launch.command.clone(),
            args: launch.args.clone(),
            cwd: launch.cwd.clone(),
            env: launch.env.clone(),
            cols: 80,
            rows: 24,
        });
        commands
            .entity(entity)
            .remove::<PendingServiceCreate>()
            .insert(AwaitingProcessCreated);
    }

    // Handle pending attaches
    for (entity, pid) in &pending_attach {
        service
            .0
            .send(ClientMessage::AttachProcess { process_id: *pid });
        service
            .0
            .send(ClientMessage::RequestSnapshot { process_id: *pid });
        commands.entity(entity).remove::<PendingServiceAttach>();
    }

    // Drain service messages and dispatch
    let mut restarted_missing_processes = Vec::new();
    for msg in service.0.drain() {
        match msg {
            ServiceMessage::ProcessCreated { process_id, pid } => {
                let entity = (&awaiting_create)
                    .into_iter()
                    .find(|(_, pid_c, _)| **pid_c == process_id)
                    .map(|(e, _, _)| e);
                if let Some(entity) = entity {
                    service.0.send(ClientMessage::AttachProcess { process_id });
                    apply_process_created(&mut commands, entity, process_id, pid);
                } else {
                    bevy::log::warn!(
                        "ProcessCreated for unknown process_id {process_id}; dropping"
                    );
                }
            }
            ServiceMessage::ProcessCreateFailed { reason } => {
                bevy::log::warn!("service failed to create process: {reason}");
            }
            ServiceMessage::ViewportPatch {
                process_id,
                changed_lines,
                cursor,
                cols,
                rows,
                selection,
                copy_mode,
                full,
            } => {
                for (entity, pid, _) in &terminals {
                    if *pid == process_id {
                        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                            continue;
                        }
                        let patch = TermViewportPatch {
                            changed_lines,
                            cursor,
                            cols,
                            rows,
                            selection,
                            copy_mode,
                            full,
                        };
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            TERM_VIEWPORT_EVENT,
                            &patch,
                        ));
                        break;
                    }
                }
            }
            ServiceMessage::ProcessTitle { process_id, title } => {
                for (entity, pid, _) in &terminals {
                    if *pid == process_id {
                        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                            continue;
                        }
                        let evt = TermTitleEvent { title };
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            TERM_TITLE_EVENT,
                            &evt,
                        ));
                        break;
                    }
                }
            }
            ServiceMessage::Snapshot {
                process_id,
                lines,
                cursor,
                cols,
                rows,
            } => {
                for (entity, pid, _) in &terminals {
                    if *pid == process_id {
                        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                            continue;
                        }
                        let patch = TermViewportPatch {
                            changed_lines: lines
                                .into_iter()
                                .enumerate()
                                .map(|(i, l)| (i as u16, l))
                                .collect(),
                            cursor,
                            cols,
                            rows,
                            selection: None,
                            copy_mode: false,
                            full: true,
                        };
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            TERM_VIEWPORT_EVENT,
                            &patch,
                        ));
                        break;
                    }
                }
            }
            ServiceMessage::ProcessExited {
                process_id,
                exit_code,
            } => {
                writers.process_exited.write(ProcessExitedEvent {
                    process_id,
                    exit_code,
                });
                mode_map.modes.remove(&process_id);
                set_local_copy_mode(&mut local_copy_mode, process_id, false);
                mouse_state.per_process.remove(&process_id);
                for (entity, pid, child_of) in &terminals {
                    if *pid == process_id {
                        commands
                            .entity(entity)
                            .insert(ProcessExited)
                            .remove::<CloseRequiresConfirmation>();
                        let tab = child_of.get();
                        commands.entity(tab).insert(LastActivatedAt::now());
                        writers
                            .app_commands
                            .write(AppCommand::Layout(LayoutCommand::Stack(
                                StackCommand::Close,
                            )));
                        break;
                    }
                }
            }
            ServiceMessage::ProcessList { processes } => {
                commands
                    .insert_resource(crate::processes_monitor::ServiceProcessList { processes });
            }
            ServiceMessage::Error { message } => {
                if let Some(stale_pid) = missing_process_id(&message)
                    && !restarted_missing_processes.contains(&stale_pid)
                {
                    let candidates = terminals.iter().map(|(entity, terminal_pid, _)| {
                        let launch = launches.get(entity).cloned().unwrap_or_else(|_| {
                            crate::terminal::launch::TerminalLaunch {
                                command: terminal_shell(&settings),
                                args: vec![],
                                cwd: String::new(),
                                env: vec![],
                                kind: crate::terminal::launch::TerminalKind::Plain,
                            }
                        });
                        let agent_kind = agent_sessions.get(entity).ok().map(|s| s.kind);
                        (entity, *terminal_pid, launch, agent_kind)
                    });
                    if let Some(restart) = missing_terminal_restart(stale_pid, candidates) {
                        restarted_missing_processes.push(stale_pid);
                        let cwd = restart.cwd.clone();
                        let agent_kind = restart.agent_kind;
                        let new_id = restart.new_id;
                        let entity = restart.entity;
                        service.0.send(restart.command);
                        let mut ec = commands.entity(entity);
                        ec.insert(new_id);
                        ec.insert(AwaitingProcessCreated);
                        if let Some(kind) = agent_kind {
                            ec.insert(vmux_agent::session::PendingAgentSession {
                                kind,
                                spawn_time: std::time::SystemTime::now(),
                                cwd: std::path::PathBuf::from(&cwd),
                            });
                        }
                    }
                }
                warn!("Service error: {message}");
            }
            ServiceMessage::TerminalMode {
                process_id,
                mouse_capture,
                copy_mode,
            } => {
                mode_map.modes.insert(
                    process_id,
                    TerminalModeFlags {
                        mouse_capture,
                        copy_mode,
                    },
                );
                set_local_copy_mode(&mut local_copy_mode, process_id, copy_mode);
            }
            ServiceMessage::SelectionText {
                process_id: _,
                text,
            } if !text.is_empty() => {
                crate::clipboard::write(text);
            }
            ServiceMessage::AgentCommand {
                request_id,
                command,
            } => {
                writers
                    .agent_commands
                    .write(crate::agent::AgentCommandRequest {
                        request_id,
                        command,
                    });
            }
            ServiceMessage::AgentQuery { request_id, query } => {
                writers
                    .agent_queries
                    .write(crate::agent::AgentQueryRequest { request_id, query });
            }
            _ => {}
        }
    }
}

fn flush_pending_terminal_input(
    pending: Query<
        (Entity, &ProcessId, &PendingTerminalInput),
        (
            With<Terminal>,
            Without<PendingServiceCreate>,
            Without<AwaitingProcessCreated>,
            Without<ProcessExited>,
        ),
    >,
    service: Option<Res<ServiceClient>>,
    mut commands: Commands,
) {
    let Some(service) = service else { return };
    for (entity, pid, input) in &pending {
        service.0.send(ClientMessage::ProcessInput {
            process_id: *pid,
            data: input.data.clone(),
        });
        commands.entity(entity).remove::<PendingTerminalInput>();
    }
}

fn is_non_character_key(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::F1
            | KeyCode::F2
            | KeyCode::F3
            | KeyCode::F4
            | KeyCode::F5
            | KeyCode::F6
            | KeyCode::F7
            | KeyCode::F8
            | KeyCode::F9
            | KeyCode::F10
            | KeyCode::F11
            | KeyCode::F12
            | KeyCode::ArrowLeft
            | KeyCode::ArrowUp
            | KeyCode::ArrowRight
            | KeyCode::ArrowDown
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Escape
            | KeyCode::Tab
            | KeyCode::Enter
            | KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::Insert
    )
}

/// Handle keyboard input directly from Bevy, bypassing CEF round-trip.
///
/// Only routes input to the single focused terminal (CefKeyboardTarget is
/// expected to mark exactly one entity). If multiple terminals are
/// keyboard-targeted simultaneously, only the first is used and the rest
/// are ignored — copy-mode and Cmd+C decisions are per-terminal so we
/// must not broadcast them.
fn handle_terminal_keyboard(
    mut er: MessageReader<KeyboardInput>,
    targeted_terminals: Query<&ProcessId, (With<Terminal>, With<CefKeyboardTarget>)>,
    keyboard_targets: Query<(), With<CefKeyboardTarget>>,
    terminals: Query<(&ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    mode: Res<crate::scene::InteractionMode>,
    input: Res<ButtonInput<KeyCode>>,
    chord_state: Res<crate::shortcut::ChordState>,
    service: Option<Res<ServiceClient>>,
    mode_map: Res<TerminalModeMap>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
) {
    let target_processes = resolve_terminal_input_targets(
        targeted_terminals.iter().copied(),
        !keyboard_targets.is_empty(),
        focus.stack,
        terminals
            .iter()
            .map(|(pid, child_of)| (child_of.get(), *pid)),
        *mode,
    );

    if target_processes.is_empty() {
        for _ in er.read() {}
        return;
    };
    let active_process_id = target_processes.first().copied();
    let Some(service) = service else {
        for _ in er.read() {}
        return;
    };
    if chord_state.pending_prefix.is_some() {
        for _ in er.read() {}
        return;
    }
    let ctrl = input.pressed(KeyCode::ControlLeft) || input.pressed(KeyCode::ControlRight);
    let alt = input.pressed(KeyCode::AltLeft) || input.pressed(KeyCode::AltRight);
    let shift = input.pressed(KeyCode::ShiftLeft) || input.pressed(KeyCode::ShiftRight);
    let super_key = input.pressed(KeyCode::SuperLeft) || input.pressed(KeyCode::SuperRight);

    let copy_mode_active = active_process_id
        .map(|process_id| is_copy_mode_active(&mode_map, &local_copy_mode, process_id))
        .unwrap_or(false);

    let mut seen_keys: Vec<KeyCode> = Vec::new();
    for event in er.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        if !event.repeat && is_non_character_key(event.key_code) {
            if seen_keys.contains(&event.key_code) {
                continue;
            }
            seen_keys.push(event.key_code);
            if !input.just_pressed(event.key_code) {
                continue;
            }
        }

        if copy_mode_active {
            let Some(active_process_id) = active_process_id else {
                continue;
            };
            let mapped = map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                active_process_id,
                CopyModeKeyInput {
                    key: &event.logical_key,
                    key_code: event.key_code,
                    ctrl,
                    shift,
                },
            );
            for k in mapped {
                if copy_mode_key_exits(k) {
                    set_local_copy_mode(&mut local_copy_mode, active_process_id, false);
                }
                service.0.send(ClientMessage::CopyModeKey {
                    process_id: active_process_id,
                    key: k,
                });
            }
            // While in copy mode, swallow ALL keys — never forward to PTY.
            continue;
        }

        if super_key {
            match event.key_code {
                KeyCode::KeyV => {
                    // Paste via OS clipboard.
                    if let Some(text) = crate::clipboard::read_blocking()
                        && !text.is_empty()
                    {
                        // Wrap in bracketed paste sequences
                        let mut data = Vec::new();
                        data.extend_from_slice(b"\x1b[200~");
                        data.extend_from_slice(text.as_bytes());
                        data.extend_from_slice(b"\x1b[201~");
                        for process_id in &target_processes {
                            service.0.send(ClientMessage::ProcessInput {
                                process_id: *process_id,
                                data: data.clone(),
                            });
                        }
                    }
                    continue;
                }
                KeyCode::KeyC => {
                    // Round-trip selection through the service, then copy to pasteboard.
                    if let Some(process_id) = active_process_id {
                        service
                            .0
                            .send(ClientMessage::GetSelectionText { process_id });
                    }
                    continue;
                }
                _ => continue,
            }
        }

        // Skip selection keys (Shift+Arrow etc) — service doesn't support local selection
        if shift
            && matches!(
                event.key_code,
                KeyCode::ArrowLeft
                    | KeyCode::ArrowRight
                    | KeyCode::ArrowUp
                    | KeyCode::ArrowDown
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::PageUp
                    | KeyCode::PageDown
            )
        {
            continue;
        }

        let bytes = logical_key_to_bytes(&event.logical_key, ctrl, alt);
        if bytes.is_empty() {
            continue;
        }
        for process_id in &target_processes {
            service.0.send(ClientMessage::ProcessInput {
                process_id: *process_id,
                data: bytes.clone(),
            });
        }
    }
}

/// Translate a Bevy logical key + ctrl modifier into the corresponding
/// Vim-style copy-mode action. Returns None if the key has no copy-mode
/// binding (caller should swallow it regardless).
#[cfg(test)]
fn map_copy_mode_key(key: &Key, ctrl: bool) -> Option<vmux_service::protocol::CopyModeKey> {
    map_copy_mode_key_from_input(CopyModeKeyInput {
        key,
        key_code: KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
        ctrl,
        shift: false,
    })
}

fn map_copy_mode_key_from_input(
    input: CopyModeKeyInput<'_>,
) -> Option<vmux_service::protocol::CopyModeKey> {
    use vmux_service::protocol::CopyModeKey as K;
    match (input.key, input.ctrl) {
        (Key::ArrowLeft, _) => Some(K::Left),
        (Key::ArrowRight, _) => Some(K::Right),
        (Key::ArrowUp, _) => Some(K::Up),
        (Key::ArrowDown, _) => Some(K::Down),
        (Key::Enter, _) => Some(K::Copy),
        (Key::Escape, _) => Some(K::Exit),
        (Key::Home, _) => Some(K::LineStart),
        (Key::End, _) => Some(K::LineEnd),
        (Key::PageUp, _) => Some(K::PageUp),
        (Key::PageDown, _) => Some(K::PageDown),
        _ if input.ctrl && key_char_eq(input, 'u') => Some(K::PageUp),
        _ if input.ctrl && key_char_eq(input, 'd') => Some(K::PageDown),
        _ if input.ctrl && key_char_eq(input, 'e') => Some(K::Down),
        _ if input.ctrl && key_char_eq(input, 'y') => Some(K::Up),
        _ if input.ctrl && key_char_eq(input, 'b') => Some(K::PageUp),
        _ if input.ctrl && key_char_eq(input, 'f') => Some(K::PageDown),
        _ if input.ctrl && key_char_eq(input, 'c') => Some(K::Exit),
        _ if key_char_eq(input, 'h') => Some(K::Left),
        _ if key_char_eq(input, 'j') => Some(K::Down),
        _ if key_char_eq(input, 'k') => Some(K::Up),
        _ if key_char_eq(input, 'l') => Some(K::Right),
        _ if key_char_eq(input, '0') => Some(K::LineStart),
        _ if key_char_eq(input, '$') => Some(K::LineEnd),
        _ if key_char_eq(input, '^') => Some(K::FirstNonBlank),
        _ if key_char_eq(input, 'w') => Some(K::WordForward),
        _ if key_char_eq(input, 'W') => Some(K::BigWordForward),
        _ if key_char_eq(input, 'b') => Some(K::WordBackward),
        _ if key_char_eq(input, 'B') => Some(K::BigWordBackward),
        _ if key_char_eq(input, 'e') => Some(K::WordEndForward),
        _ if key_char_eq(input, 'E') => Some(K::BigWordEndForward),
        _ if key_char_eq(input, 'G') => Some(K::Bottom),
        _ if key_char_eq(input, 'H') => Some(K::ScreenTop),
        _ if key_char_eq(input, 'M') => Some(K::ScreenMiddle),
        _ if key_char_eq(input, 'L') => Some(K::ScreenBottom),
        _ if key_char_eq(input, '{') => Some(K::PrevParagraph),
        _ if key_char_eq(input, '}') => Some(K::NextParagraph),
        _ if key_char_eq(input, ';') => Some(K::RepeatFind),
        _ if key_char_eq(input, ',') => Some(K::RepeatFindReverse),
        _ if key_char_eq(input, 'o') => Some(K::SwapSelectionEnds),
        _ if key_char_eq(input, 'v') => Some(K::StartSelection),
        _ if key_char_eq(input, 'V') => Some(K::StartLineSelection),
        _ if key_char_eq(input, 'y') => Some(K::Copy),
        _ if key_char_eq(input, 'q') => Some(K::Exit),
        _ => None,
    }
}

#[cfg(test)]
fn map_copy_mode_key_with_state(
    local_copy_mode: &mut LocalCopyModeState,
    process_id: ProcessId,
    key: &Key,
    ctrl: bool,
) -> Option<vmux_service::protocol::CopyModeKey> {
    map_copy_mode_keys_with_state(
        local_copy_mode,
        process_id,
        CopyModeKeyInput {
            key,
            key_code: KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
            ctrl,
            shift: false,
        },
    )
    .into_iter()
    .next()
}

fn map_copy_mode_keys_with_state(
    local_copy_mode: &mut LocalCopyModeState,
    process_id: ProcessId,
    input: CopyModeKeyInput<'_>,
) -> Vec<vmux_service::protocol::CopyModeKey> {
    use vmux_service::protocol::CopyModeKey as K;

    let state = local_copy_mode.input_states.entry(process_id).or_default();
    if let Some(pending) = state.pending_key.take() {
        let key = match pending {
            CopyModePendingKey::G if !input.ctrl && key_char_eq(input, '_') => {
                Some(K::LastNonBlank)
            }
            CopyModePendingKey::G if !input.ctrl && key_char_eq(input, 'g') => Some(K::Top),
            CopyModePendingKey::G if !input.ctrl && key_char_eq(input, 'e') => {
                Some(K::WordEndBackward)
            }
            CopyModePendingKey::G if !input.ctrl && key_char_eq(input, 'E') => {
                Some(K::BigWordEndBackward)
            }
            CopyModePendingKey::FindForward => input_char(input).map(K::FindForward),
            CopyModePendingKey::FindBackward => input_char(input).map(K::FindBackward),
            CopyModePendingKey::TillForward => input_char(input).map(K::TillForward),
            CopyModePendingKey::TillBackward => input_char(input).map(K::TillBackward),
            _ => None,
        };
        if let Some(key) = key {
            return repeat_copy_mode_key(state, key);
        }
    }

    if let Some(digit) = input_digit(input)
        && (!matches!(digit, 0) || state.count.is_some())
        && !input.ctrl
    {
        let current = state.count.unwrap_or(0);
        state.count = Some(current.saturating_mul(10).saturating_add(digit).min(999));
        return Vec::new();
    }

    if !input.ctrl && key_char_eq(input, 'g') {
        state.pending_key = Some(CopyModePendingKey::G);
        return Vec::new();
    }

    if !input.ctrl && key_char_eq(input, 'f') {
        state.pending_key = Some(CopyModePendingKey::FindForward);
        return Vec::new();
    }

    if !input.ctrl && key_char_eq(input, 'F') {
        state.pending_key = Some(CopyModePendingKey::FindBackward);
        return Vec::new();
    }

    if !input.ctrl && key_char_eq(input, 't') {
        state.pending_key = Some(CopyModePendingKey::TillForward);
        return Vec::new();
    }

    if !input.ctrl && key_char_eq(input, 'T') {
        state.pending_key = Some(CopyModePendingKey::TillBackward);
        return Vec::new();
    }

    map_copy_mode_key_from_input(input)
        .map(|key| repeat_copy_mode_key(state, key))
        .unwrap_or_default()
}

fn repeat_copy_mode_key(
    state: &mut CopyModeInputState,
    key: vmux_service::protocol::CopyModeKey,
) -> Vec<vmux_service::protocol::CopyModeKey> {
    let repeat = if copy_mode_key_uses_count(key) {
        state.count.take().unwrap_or(1)
    } else {
        state.count = None;
        1
    };
    vec![key; repeat as usize]
}

fn copy_mode_key_uses_count(key: vmux_service::protocol::CopyModeKey) -> bool {
    use vmux_service::protocol::CopyModeKey as K;
    !matches!(
        key,
        K::StartSelection | K::StartLineSelection | K::Copy | K::Exit
    )
}

fn input_char(input: CopyModeKeyInput<'_>) -> Option<char> {
    match input.key {
        Key::Character(s) => s.chars().next(),
        _ => None,
    }
}

fn input_digit(input: CopyModeKeyInput<'_>) -> Option<u16> {
    let c = input_char(input)?;
    c.to_digit(10).map(|d| d as u16)
}

fn key_char_eq(input: CopyModeKeyInput<'_>, expected: char) -> bool {
    if input_char(input) == Some(expected) {
        return true;
    }
    match expected {
        '_' => input.shift && input.key_code == KeyCode::Minus,
        '$' => input.shift && input.key_code == KeyCode::Digit4,
        '^' => input.shift && input.key_code == KeyCode::Digit6,
        '{' => input.shift && input.key_code == KeyCode::BracketLeft,
        '}' => input.shift && input.key_code == KeyCode::BracketRight,
        'W' => input.shift && input.key_code == KeyCode::KeyW,
        'B' => input.shift && input.key_code == KeyCode::KeyB,
        'E' => input.shift && input.key_code == KeyCode::KeyE,
        'G' => input.shift && input.key_code == KeyCode::KeyG,
        'H' => input.shift && input.key_code == KeyCode::KeyH,
        'M' => input.shift && input.key_code == KeyCode::KeyM,
        'L' => input.shift && input.key_code == KeyCode::KeyL,
        'F' => input.shift && input.key_code == KeyCode::KeyF,
        'T' => input.shift && input.key_code == KeyCode::KeyT,
        'V' => input.shift && input.key_code == KeyCode::KeyV,
        _ => false,
    }
}

fn resolve_terminal_input_targets(
    targeted_terminal_ids: impl IntoIterator<Item = ProcessId>,
    any_keyboard_target_active: bool,
    focused_tab: Option<Entity>,
    terminal_ids_by_tab: impl IntoIterator<Item = (Entity, ProcessId)>,
    mode: crate::scene::InteractionMode,
) -> Vec<ProcessId> {
    let targeted: Vec<ProcessId> = targeted_terminal_ids.into_iter().collect();
    if !targeted.is_empty() {
        return targeted;
    }
    if any_keyboard_target_active || mode != crate::scene::InteractionMode::User {
        return Vec::new();
    }
    let Some(focused_tab) = focused_tab else {
        return Vec::new();
    };
    terminal_ids_by_tab
        .into_iter()
        .filter_map(|(tab, process_id)| (tab == focused_tab).then_some(process_id))
        .collect()
}

fn logical_key_to_bytes(key: &Key, ctrl: bool, alt: bool) -> Vec<u8> {
    match key {
        Key::Character(s) => {
            if ctrl && let Some(c) = s.chars().next() {
                let code = (c.to_ascii_lowercase() as u8)
                    .wrapping_sub(b'a')
                    .wrapping_add(1);
                if code <= 26 {
                    let mut v = Vec::new();
                    if alt {
                        v.push(0x1b);
                    }
                    v.push(code);
                    return v;
                }
            }
            if alt {
                let mut v = vec![0x1b];
                v.extend_from_slice(s.as_bytes());
                return v;
            }
            s.as_bytes().to_vec()
        }
        Key::Enter => b"\r".to_vec(),
        Key::Backspace => {
            if ctrl {
                vec![0x08]
            } else {
                vec![0x7f]
            }
        }
        Key::Tab => b"\t".to_vec(),
        Key::Escape => vec![0x1b],
        Key::Space => {
            if ctrl {
                let mut v = Vec::new();
                if alt {
                    v.push(0x1b);
                }
                v.push(0);
                return v;
            }
            b" ".to_vec()
        }
        Key::ArrowUp => b"\x1b[A".to_vec(),
        Key::ArrowDown => b"\x1b[B".to_vec(),
        Key::ArrowRight => b"\x1b[C".to_vec(),
        Key::ArrowLeft => b"\x1b[D".to_vec(),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Delete => b"\x1b[3~".to_vec(),
        Key::Insert => b"\x1b[2~".to_vec(),
        _ => Vec::new(),
    }
}

/// Handle mouse wheel scrolling — sends scroll input to service.
fn handle_terminal_scroll(
    mut er: MessageReader<MouseWheel>,
    targeted_terminals: Query<&ProcessId, (With<Terminal>, With<CefKeyboardTarget>)>,
    keyboard_targets: Query<(), With<CefKeyboardTarget>>,
    terminals: Query<(&ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    mode: Res<crate::scene::InteractionMode>,
    service: Option<Res<ServiceClient>>,
) {
    let target_processes = resolve_terminal_input_targets(
        targeted_terminals.iter().copied(),
        !keyboard_targets.is_empty(),
        focus.stack,
        terminals
            .iter()
            .map(|(pid, child_of)| (child_of.get(), *pid)),
        *mode,
    );

    if target_processes.is_empty() {
        for _ in er.read() {}
        return;
    }
    let Some(service) = service else {
        for _ in er.read() {}
        return;
    };
    for event in er.read() {
        let lines = match event.unit {
            MouseScrollUnit::Line => -event.y as i32,
            MouseScrollUnit::Pixel => (-event.y / 20.0) as i32,
        };
        if lines == 0 {
            continue;
        }
        // Send scroll as mouse button 64/65 SGR sequences
        let button: u8 = if lines < 0 { 64 } else { 65 };
        let count = lines.unsigned_abs();
        let seq = sgr_mouse_sequence(button, 0, 0, 0, true);
        for process_id in &target_processes {
            for _ in 0..count {
                service.0.send(ClientMessage::ProcessInput {
                    process_id: *process_id,
                    data: seq.clone(),
                });
            }
        }
    }
}

/// Encode a mouse event as an SGR escape sequence.
fn sgr_mouse_sequence(button: u8, col: u16, row: u16, modifiers: u8, pressed: bool) -> Vec<u8> {
    let mut cb = button as u32;
    if modifiers & MOD_SHIFT != 0 {
        cb += 4;
    }
    if modifiers & MOD_ALT != 0 {
        cb += 8;
    }
    if modifiers & MOD_CTRL != 0 {
        cb += 16;
    }
    let suffix = if pressed { 'M' } else { 'm' };
    format!("\x1b[<{};{};{}{}", cb, col + 1, row + 1, suffix).into_bytes()
}

/// Tracks the most recent mouse-down per process for click-count detection
/// (300ms / ~1 cell window) and an active drag anchor.
#[derive(Resource, Default)]
struct MouseSelectionState {
    per_process: std::collections::HashMap<ProcessId, MouseSessionState>,
}

#[derive(Default, Clone, Debug)]
struct MouseSessionState {
    last_click: Option<MouseClickRecord>,
    drag_active: bool,
    drag_visual_active: bool,
    /// Last (col, row) sent via ExtendSelectionTo during the active drag.
    /// Used to dedupe redundant move events at the same cell.
    last_extend_cell: Option<(u16, u16)>,
    /// Anchor cell from the most recent left-click that has not yet
    /// produced a real selection. We defer materializing the selection
    /// until the user actually drags so a single click doesn't draw a
    /// 1-character selection box.
    pending_anchor: Option<(u16, u16)>,
}

#[derive(Clone, Copy, Debug)]
struct MouseClickRecord {
    when: std::time::Instant,
    col: u16,
    row: u16,
    count: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MouseTerminalAction {
    ForwardInput(Vec<u8>),
    EnterCopyMode,
    ExitCopyMode,
    SetSelection(Option<TermSelectionRange>),
    ExtendSelectionTo { col: u16, row: u16 },
    SelectWordAt { col: u16, row: u16 },
    SelectLineAt { row: u16 },
}

fn mouse_terminal_actions(
    entry: &mut MouseSessionState,
    event: &TermMouseEvent,
    mouse_capture: bool,
    now: std::time::Instant,
) -> Vec<MouseTerminalAction> {
    let shift = event.modifiers & MOD_SHIFT != 0;
    let is_left = event.button == 0;
    let select_mode = is_left && (!mouse_capture || shift);

    if !select_mode {
        let button = if event.moving {
            event.button + 32
        } else {
            event.button
        };
        return vec![MouseTerminalAction::ForwardInput(sgr_mouse_sequence(
            button,
            event.col,
            event.row,
            event.modifiers,
            event.pressed,
        ))];
    }

    if event.pressed && !event.moving {
        let count = match entry.last_click {
            Some(prev)
                if now.duration_since(prev.when) <= MULTI_CLICK_WINDOW
                    && (prev.col as i32 - event.col as i32).abs() <= MULTI_CLICK_CELL_TOLERANCE
                    && (prev.row as i32 - event.row as i32).abs() <= MULTI_CLICK_CELL_TOLERANCE =>
            {
                if prev.count >= 3 {
                    1
                } else {
                    prev.count + 1
                }
            }
            _ => 1,
        };
        entry.last_click = Some(MouseClickRecord {
            when: now,
            col: event.col,
            row: event.row,
            count,
        });
        entry.drag_active = count == 1;
        entry.drag_visual_active = false;
        entry.last_extend_cell = Some((event.col, event.row));

        match count {
            1 if shift => {
                entry.pending_anchor = None;
                vec![MouseTerminalAction::ExtendSelectionTo {
                    col: event.col,
                    row: event.row,
                }]
            }
            1 => {
                entry.pending_anchor = Some((event.col, event.row));
                vec![MouseTerminalAction::SetSelection(None)]
            }
            2 => {
                entry.pending_anchor = None;
                vec![MouseTerminalAction::SelectWordAt {
                    col: event.col,
                    row: event.row,
                }]
            }
            _ => {
                entry.pending_anchor = None;
                vec![MouseTerminalAction::SelectLineAt { row: event.row }]
            }
        }
    } else if event.moving && entry.drag_active {
        if entry.last_extend_cell == Some((event.col, event.row)) {
            return Vec::new();
        }
        entry.last_extend_cell = Some((event.col, event.row));
        if let Some((ac, ar)) = entry.pending_anchor.take() {
            entry.drag_visual_active = true;
            vec![
                MouseTerminalAction::EnterCopyMode,
                MouseTerminalAction::SetSelection(Some(TermSelectionRange {
                    start_col: ac,
                    start_row: ar,
                    end_col: event.col,
                    end_row: event.row,
                    is_block: false,
                })),
            ]
        } else {
            vec![MouseTerminalAction::ExtendSelectionTo {
                col: event.col,
                row: event.row,
            }]
        }
    } else if !event.pressed {
        let actions = if entry.drag_visual_active {
            vec![MouseTerminalAction::ExitCopyMode]
        } else {
            Vec::new()
        };
        entry.drag_active = false;
        entry.drag_visual_active = false;
        entry.last_extend_cell = None;
        entry.pending_anchor = None;
        actions
    } else {
        Vec::new()
    }
}

fn send_mouse_action(service: &ServiceHandle, process_id: ProcessId, action: MouseTerminalAction) {
    match action {
        MouseTerminalAction::ForwardInput(data) => {
            service.send(ClientMessage::ProcessInput { process_id, data });
        }
        MouseTerminalAction::EnterCopyMode => {
            service.send(ClientMessage::EnterCopyMode { process_id });
        }
        MouseTerminalAction::ExitCopyMode => {
            service.send(ClientMessage::ExitCopyMode { process_id });
        }
        MouseTerminalAction::SetSelection(range) => {
            service.send(ClientMessage::SetSelection { process_id, range });
        }
        MouseTerminalAction::ExtendSelectionTo { col, row } => {
            service.send(ClientMessage::ExtendSelectionTo {
                process_id,
                col,
                row,
            });
        }
        MouseTerminalAction::SelectWordAt { col, row } => {
            service.send(ClientMessage::SelectWordAt {
                process_id,
                col,
                row,
            });
        }
        MouseTerminalAction::SelectLineAt { row } => {
            service.send(ClientMessage::SelectLineAt { process_id, row });
        }
    }
}

/// Handle mouse events from the terminal webview.
///
/// Selection mode (left-button + (no app mouse-capture OR shift held)) is
/// intercepted and translated into selection commands sent to the service.
/// Anything else is forwarded as SGR mouse-report bytes to the PTY.
fn on_term_mouse(
    trigger: On<BinReceive<TermMouseEvent>>,
    q: Query<&ProcessId, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
    mode_map: Res<TerminalModeMap>,
    mut state: ResMut<MouseSelectionState>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(service) = service else { return };
    let Ok(pid) = q.get(entity) else { return };
    let process_id = *pid;

    let mouse_capture = mode_map
        .modes
        .get(&process_id)
        .map(|m| m.mouse_capture)
        .unwrap_or(false);
    let entry = state.per_process.entry(process_id).or_default();
    for action in mouse_terminal_actions(entry, event, mouse_capture, std::time::Instant::now()) {
        update_local_copy_mode_for_mouse_action(&mut local_copy_mode, process_id, &action);
        send_mouse_action(&service.0, process_id, action);
    }
}

/// Mark dirty when webview becomes ready so initial viewport is sent.
fn on_term_ready(
    trigger: On<Add, UiReady>,
    q: Query<&ProcessId, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
) {
    let entity = trigger.event_target();
    let Some(service) = service else { return };
    if let Ok(pid) = q.get(entity) {
        service
            .0
            .send(ClientMessage::RequestSnapshot { process_id: *pid });
    }
}

/// Handle resize event from webview (reports char cell dimensions).
fn on_term_resize(
    trigger: On<BinReceive<TermResizeEvent>>,
    webview_q: Query<&WebviewSize, With<Terminal>>,
    pid_q: Query<&ProcessId, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(service) = service else { return };

    let Ok(webview_size) = webview_q.get(entity) else {
        return;
    };
    let Ok(pid) = pid_q.get(entity) else {
        return;
    };

    if event.char_width <= 0.0 || event.char_height <= 0.0 {
        return;
    }

    let vw = if event.viewport_width > 0.0 {
        event.viewport_width
    } else {
        webview_size.0.x
    };
    let vh = if event.viewport_height > 0.0 {
        event.viewport_height
    } else {
        webview_size.0.y
    };

    let cols = (vw / event.char_width).floor().max(1.0) as u16;
    let rows = (vh / event.char_height).floor().max(1.0) as u16;

    service.0.send(ClientMessage::ResizeProcess {
        process_id: *pid,
        cols,
        rows,
    });
}

fn sync_terminal_theme(
    q: Query<Entity, With<Terminal>>,
    new_terminals: Query<Entity, Added<Terminal>>,
    newly_ready: Query<Entity, (With<Terminal>, Added<UiReady>)>,
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut last_theme_hash: Local<u64>,
) {
    let Some(terminal_settings) = &settings.terminal else {
        return;
    };

    let theme = terminal_settings.resolve_theme(&terminal_settings.default_theme);
    let colors =
        crate::themes::resolve_theme(&theme.color_scheme, &terminal_settings.custom_themes);

    let hash = {
        let mut h: u64 = 0;
        for b in &colors.foreground {
            h = h.wrapping_mul(31).wrapping_add(*b as u64);
        }
        for b in &colors.background {
            h = h.wrapping_mul(31).wrapping_add(*b as u64);
        }
        for row in &colors.ansi {
            for b in row {
                h = h.wrapping_mul(31).wrapping_add(*b as u64);
            }
        }
        h
    };

    let theme_changed = hash != *last_theme_hash;
    if !theme_changed && new_terminals.is_empty() && newly_ready.is_empty() {
        return;
    }
    *last_theme_hash = hash;

    let event = vmux_terminal::event::TermThemeEvent {
        foreground: colors.foreground,
        background: colors.background,
        cursor: colors.cursor,
        ansi: colors.ansi,
        font_family: theme.font_family.clone(),
        font_size: theme.font_size,
        line_height: theme.line_height,
        padding: theme.padding,
        cursor_style: theme.cursor_style.clone(),
        cursor_blink: theme.cursor_blink,
    };
    let targets: Vec<Entity> = if theme_changed {
        q.iter().collect()
    } else {
        new_terminals.iter().chain(newly_ready.iter()).collect()
    };

    for entity in targets {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                TERM_THEME_EVENT,
                &event,
            ));
        }
    }
}

fn on_restart_pty(
    trigger: On<RestartPty>,
    mut q: Query<(
        &mut ProcessId,
        &mut PageMetadata,
        Option<&mut crate::terminal::launch::TerminalLaunch>,
        Option<&vmux_agent::session::AgentSession>,
        Option<&vmux_agent::session::SessionId>,
    )>,
    service: Option<Res<ServiceClient>>,
    settings: Res<AppSettings>,
    strategies: Option<Res<vmux_agent::strategy::AgentStrategies>>,
) {
    let entity = trigger.event().entity;
    let Some(service) = service else { return };
    let Ok((mut pid, mut meta, mut launch, agent_session, session_id)) = q.get_mut(entity) else {
        return;
    };

    service
        .0
        .send(ClientMessage::KillProcess { process_id: *pid });

    let (command, args, cwd, env) = match (launch.as_deref(), agent_session, strategies.as_deref())
    {
        (Some(l), Some(session), Some(strategies)) => {
            let mut updated_args = l.args.clone();
            if let Some(strategy) = strategies.get_cli(session.kind) {
                let mcp = vmux_agent::McpServerConfig {
                    command: l.command.clone(),
                    args: vec![],
                    cwd: None,
                };
                updated_args = strategy.build_args(&mcp, session_id.map(|s| s.0.as_str()));
            }
            (
                l.command.clone(),
                updated_args,
                l.cwd.clone(),
                l.env.clone(),
            )
        }
        (Some(l), _, _) => (
            l.command.clone(),
            l.args.clone(),
            l.cwd.clone(),
            l.env.clone(),
        ),
        _ => {
            let shell = settings
                .terminal
                .as_ref()
                .map(|t| t.resolve_theme(&t.default_theme).shell)
                .unwrap_or_else(default_shell);
            (shell, vec![], String::new(), Vec::new())
        }
    };

    let new_id = ProcessId::new();
    service.0.send(ClientMessage::CreateProcess {
        process_id: new_id,
        command: command.clone(),
        args: args.clone(),
        cwd: cwd.clone(),
        env: env.clone(),
        cols: 80,
        rows: 24,
    });
    service
        .0
        .send(ClientMessage::AttachProcess { process_id: new_id });

    *pid = new_id;
    if let Some(l) = launch.as_mut() {
        l.args = args;
    } else {
        meta.url = TERMINAL_WEBVIEW_URL.to_string();
        meta.title = format!("Terminal ({})", &new_id.to_string()[..8]);
    }
}

/// Consume `AppCommand::Terminal::CopyMode` and ask the service to enter
/// visual/copy mode for the currently focused terminal process.
fn handle_terminal_copy_mode_command(
    mut er: MessageReader<AppCommand>,
    targeted_terminals: Query<&ProcessId, (With<Terminal>, With<CefKeyboardTarget>)>,
    keyboard_targets: Query<(), With<CefKeyboardTarget>>,
    terminals: Query<(&ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    mode: Res<crate::scene::InteractionMode>,
    service: Option<Res<ServiceClient>>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
) {
    let Some(service) = service else {
        for _ in er.read() {}
        return;
    };
    let target_processes = resolve_terminal_input_targets(
        targeted_terminals.iter().copied(),
        !keyboard_targets.is_empty(),
        focus.stack,
        terminals
            .iter()
            .map(|(pid, child_of)| (child_of.get(), *pid)),
        *mode,
    );
    let active_process_id = target_processes.first().copied();
    for cmd in er.read() {
        if matches!(
            cmd,
            AppCommand::Terminal(crate::command::TerminalCommand::CopyMode)
        ) && let Some(process_id) = active_process_id
        {
            set_local_copy_mode(&mut local_copy_mode, process_id, true);
            service.0.send(ClientMessage::EnterCopyMode { process_id });
        }
    }
}

fn is_copy_mode_active(
    mode_map: &TerminalModeMap,
    local_copy_mode: &LocalCopyModeState,
    process_id: ProcessId,
) -> bool {
    mode_map
        .modes
        .get(&process_id)
        .map(|m| m.copy_mode)
        .unwrap_or(false)
        || local_copy_mode.active.contains(&process_id)
}

fn set_local_copy_mode(
    local_copy_mode: &mut LocalCopyModeState,
    process_id: ProcessId,
    active: bool,
) {
    if active {
        local_copy_mode.active.insert(process_id);
    } else {
        local_copy_mode.active.remove(&process_id);
        local_copy_mode.input_states.remove(&process_id);
    }
}

fn copy_mode_key_exits(key: vmux_service::protocol::CopyModeKey) -> bool {
    use vmux_service::protocol::CopyModeKey as K;
    matches!(key, K::Copy | K::Exit)
}

#[derive(Message, Debug, Clone)]
pub(crate) struct ProcessExitedEvent {
    pub process_id: ProcessId,
    #[allow(dead_code)]
    pub exit_code: Option<i32>,
}

fn respawn_shell_on_agent_exit_for_entity(
    commands: &mut Commands,
    entity: Entity,
    shell: &str,
    cwd: String,
) {
    let new_id = ProcessId::new();
    let mut ec = commands.entity(entity);
    ec.remove::<vmux_agent::session::AgentSession>();
    ec.remove::<vmux_agent::session::SessionId>();
    ec.remove::<vmux_agent::session::PendingAgentSession>();
    ec.insert(new_id);
    ec.insert(PendingServiceCreate);
    ec.insert(crate::terminal::launch::TerminalLaunch {
        command: shell.to_string(),
        args: vec![],
        cwd,
        env: vec![],
        kind: crate::terminal::launch::TerminalKind::Plain,
    });
}

pub(crate) fn respawn_shell_on_vibe_exit(
    mut commands: Commands,
    mut exited: MessageReader<ProcessExitedEvent>,
    q: Query<
        (Entity, &ProcessId, &crate::terminal::launch::TerminalLaunch),
        With<vmux_agent::session::AgentSession>,
    >,
    settings: Res<AppSettings>,
) {
    for ev in exited.read() {
        let Some((entity, _pid, launch)) = q.iter().find(|(_, pid, _)| **pid == ev.process_id)
        else {
            continue;
        };
        let shell = terminal_shell(&settings);
        let cwd = launch.cwd.clone();
        respawn_shell_on_agent_exit_for_entity(&mut commands, entity, &shell, cwd);
    }
}

fn update_local_copy_mode_for_mouse_action(
    local_copy_mode: &mut LocalCopyModeState,
    process_id: ProcessId,
    action: &MouseTerminalAction,
) {
    match action {
        MouseTerminalAction::EnterCopyMode => {
            set_local_copy_mode(local_copy_mode, process_id, true)
        }
        MouseTerminalAction::ExitCopyMode => {
            set_local_copy_mode(local_copy_mode, process_id, false)
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::schedule::Schedules;

    fn process_id(byte: u8) -> ProcessId {
        ProcessId([byte; 16])
    }

    #[test]
    fn missing_service_process_restarts_matching_terminal() {
        let missing = process_id(7);
        let target = Entity::from_bits(1);
        let plain_launch = || crate::terminal::launch::TerminalLaunch {
            command: default_shell(),
            args: vec![],
            cwd: String::new(),
            env: vec![],
            kind: crate::terminal::launch::TerminalKind::Plain,
        };
        let restart = missing_terminal_restart(
            missing,
            [
                (Entity::from_bits(2), process_id(8), plain_launch(), None),
                (target, missing, plain_launch(), None),
            ],
        )
        .unwrap();

        assert_eq!(restart.entity, target);
        assert!(restart.agent_kind.is_none());
        assert!(matches!(
            restart.command,
            ClientMessage::CreateProcess {
                process_id: _,
                command,
                args,
                cwd,
                env,
                cols: 80,
                rows: 24
            } if command == default_shell() && args.is_empty() && cwd.is_empty() && env.is_empty()
        ));
    }

    #[test]
    fn process_not_found_message_parses_process_id() {
        let missing = process_id(9);

        assert_eq!(
            missing_process_id(&format!("process not found: {missing}")),
            Some(missing)
        );
        assert_eq!(missing_process_id("permission denied"), None);
    }

    #[test]
    fn terminal_update_schedule_has_no_before_after_cycle() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(crate::command::CommandPlugin)
            .add_plugins(vmux_layout::stack::StackPlugin)
            .add_message::<LayoutSpawnRequest>();
        add_terminal_update_systems(&mut app);

        let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
        let mut update = schedules.remove(Update).unwrap();
        let result = update.initialize(app.world_mut());

        if let Err(error) = result {
            panic!("{}", error.to_string(update.graph(), app.world()));
        }
    }

    #[test]
    fn terminal_input_targets_fallback_to_focused_terminal_in_user_mode() {
        let tab = Entity::from_bits(1);
        let process_id = process_id(7);

        let targets = resolve_terminal_input_targets(
            [],
            false,
            Some(tab),
            [(tab, process_id)],
            crate::scene::InteractionMode::User,
        );

        assert_eq!(targets, vec![process_id]);
    }

    #[test]
    fn terminal_input_targets_do_not_steal_input_from_non_terminal_target() {
        let tab = Entity::from_bits(1);

        let targets = resolve_terminal_input_targets(
            [],
            true,
            Some(tab),
            [(tab, process_id(7))],
            crate::scene::InteractionMode::User,
        );

        assert!(targets.is_empty());
    }

    fn mouse_event(button: u8, col: u16, row: u16, pressed: bool, moving: bool) -> TermMouseEvent {
        TermMouseEvent {
            button,
            col,
            row,
            modifiers: 0,
            pressed,
            moving,
        }
    }

    #[test]
    fn drag_enters_visual_mode_on_first_motion_and_exits_on_release() {
        let mut state = MouseSessionState::default();
        let now = std::time::Instant::now();

        let down = mouse_event(0, 2, 3, true, false);
        assert_eq!(
            mouse_terminal_actions(&mut state, &down, false, now),
            vec![MouseTerminalAction::SetSelection(None)]
        );

        let drag = mouse_event(0, 5, 3, true, true);
        assert_eq!(
            mouse_terminal_actions(
                &mut state,
                &drag,
                false,
                now + std::time::Duration::from_millis(10),
            ),
            vec![
                MouseTerminalAction::EnterCopyMode,
                MouseTerminalAction::SetSelection(Some(TermSelectionRange {
                    start_col: 2,
                    start_row: 3,
                    end_col: 5,
                    end_row: 3,
                    is_block: false,
                })),
            ]
        );

        let release = mouse_event(0, 5, 3, false, false);
        assert_eq!(
            mouse_terminal_actions(
                &mut state,
                &release,
                false,
                now + std::time::Duration::from_millis(20),
            ),
            vec![MouseTerminalAction::ExitCopyMode]
        );
    }

    #[test]
    fn single_click_never_enters_visual_mode() {
        let mut state = MouseSessionState::default();
        let now = std::time::Instant::now();

        let down = mouse_event(0, 2, 3, true, false);
        assert_eq!(
            mouse_terminal_actions(&mut state, &down, false, now),
            vec![MouseTerminalAction::SetSelection(None)]
        );

        let release = mouse_event(0, 2, 3, false, false);
        assert_eq!(
            mouse_terminal_actions(
                &mut state,
                &release,
                false,
                now + std::time::Duration::from_millis(20),
            ),
            Vec::<MouseTerminalAction>::new()
        );
    }

    #[test]
    fn captured_mouse_without_shift_still_forwards_drag_motion() {
        let mut state = MouseSessionState::default();
        let event = mouse_event(0, 4, 5, true, true);

        assert_eq!(
            mouse_terminal_actions(&mut state, &event, true, std::time::Instant::now()),
            vec![MouseTerminalAction::ForwardInput(sgr_mouse_sequence(
                32, 4, 5, 0, true,
            ))]
        );
    }

    #[test]
    fn vim_visual_keys_map_to_copy_mode_actions() {
        use vmux_service::protocol::CopyModeKey as K;

        assert_eq!(
            map_copy_mode_key(&Key::Character("v".into()), false),
            Some(K::StartSelection)
        );
        assert_eq!(
            map_copy_mode_key(&Key::Character("V".into()), false),
            Some(K::StartLineSelection)
        );
        assert_eq!(
            map_copy_mode_key(&Key::Character("e".into()), true),
            Some(K::Down)
        );
        assert_eq!(
            map_copy_mode_key(&Key::Character("y".into()), true),
            Some(K::Up)
        );
        assert_eq!(
            map_copy_mode_key(&Key::Character("y".into()), false),
            Some(K::Copy)
        );
        assert_eq!(
            map_copy_mode_key(&Key::Character("c".into()), true),
            Some(K::Exit)
        );
    }

    #[test]
    fn vim_g_ends_visual_selection_at_last_non_blank() {
        use vmux_service::protocol::CopyModeKey as K;

        let process_id = ProcessId::new();
        let mut local_copy_mode = LocalCopyModeState::default();

        assert_eq!(
            map_copy_mode_key_with_state(
                &mut local_copy_mode,
                process_id,
                &Key::Character("g".into()),
                false
            ),
            None
        );
        assert_eq!(
            map_copy_mode_key_with_state(
                &mut local_copy_mode,
                process_id,
                &Key::Character("_".into()),
                false
            ),
            Some(K::LastNonBlank)
        );
    }

    #[test]
    fn vim_visual_motion_keys_map_to_copy_mode_actions() {
        use vmux_service::protocol::CopyModeKey as K;

        let process_id = ProcessId::new();
        let mut local_copy_mode = LocalCopyModeState::default();

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("w".into()), KeyCode::KeyW)
            ),
            vec![K::WordForward]
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::shift(&Key::Character("W".into()), KeyCode::KeyW)
            ),
            vec![K::BigWordForward]
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("b".into()), KeyCode::KeyB)
            ),
            vec![K::WordBackward]
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("e".into()), KeyCode::KeyE)
            ),
            vec![K::WordEndForward]
        );

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("g".into()), KeyCode::KeyG)
            ),
            Vec::<K>::new()
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("e".into()), KeyCode::KeyE)
            ),
            vec![K::WordEndBackward]
        );

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("3".into()), KeyCode::Digit3)
            ),
            Vec::<K>::new()
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("w".into()), KeyCode::KeyW)
            ),
            vec![K::WordForward, K::WordForward, K::WordForward]
        );
    }

    #[test]
    fn shifted_minus_resolves_g_() {
        use vmux_service::protocol::CopyModeKey as K;

        let process_id = ProcessId::new();
        let mut local_copy_mode = LocalCopyModeState::default();

        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::new(&Key::Character("g".into()), KeyCode::KeyG)
            ),
            Vec::<K>::new()
        );
        assert_eq!(
            map_copy_mode_keys_with_state(
                &mut local_copy_mode,
                process_id,
                CopyModeKeyInput::shift(&Key::Character("-".into()), KeyCode::Minus)
            ),
            vec![K::LastNonBlank]
        );
    }

    #[test]
    fn local_copy_mode_is_active_before_service_broadcast() {
        let process_id = ProcessId::new();
        let mode_map = TerminalModeMap::default();
        let mut local_copy_mode = LocalCopyModeState::default();

        assert!(!is_copy_mode_active(
            &mode_map,
            &local_copy_mode,
            process_id
        ));

        set_local_copy_mode(&mut local_copy_mode, process_id, true);

        assert!(is_copy_mode_active(&mode_map, &local_copy_mode, process_id));
    }

    #[test]
    fn service_copy_mode_broadcast_reconciles_local_latch() {
        let process_id = ProcessId::new();
        let mut mode_map = TerminalModeMap::default();
        let mut local_copy_mode = LocalCopyModeState::default();

        set_local_copy_mode(&mut local_copy_mode, process_id, true);
        mode_map.modes.insert(
            process_id,
            TerminalModeFlags {
                mouse_capture: false,
                copy_mode: false,
            },
        );
        set_local_copy_mode(&mut local_copy_mode, process_id, false);

        assert!(!is_copy_mode_active(
            &mode_map,
            &local_copy_mode,
            process_id
        ));
    }

    #[test]
    fn exiting_copy_mode_clears_local_latch() {
        use vmux_service::protocol::CopyModeKey as K;

        let process_id = ProcessId::new();
        let mut local_copy_mode = LocalCopyModeState::default();
        set_local_copy_mode(&mut local_copy_mode, process_id, true);

        if copy_mode_key_exits(K::Exit) {
            set_local_copy_mode(&mut local_copy_mode, process_id, false);
        }

        assert!(!local_copy_mode.active.contains(&process_id));
    }

    #[test]
    fn process_created_matches_by_id_not_by_position() {
        use crate::terminal::launch::{TerminalKind, TerminalLaunch};

        let mut app = bevy::prelude::App::new();
        let id1 = ProcessId::new();
        let id2 = ProcessId::new();
        let id3 = ProcessId::new();
        let e1 = app
            .world_mut()
            .spawn((
                Terminal,
                id1,
                PendingServiceCreate,
                AwaitingProcessCreated,
                TerminalLaunch {
                    command: "/bin/sh".into(),
                    args: vec![],
                    cwd: "/tmp/1".into(),
                    env: vec![],
                    kind: TerminalKind::Plain,
                },
            ))
            .id();
        let e2 = app
            .world_mut()
            .spawn((
                Terminal,
                id2,
                AwaitingProcessCreated,
                TerminalLaunch {
                    command: "/bin/sh".into(),
                    args: vec![],
                    cwd: "/tmp/2".into(),
                    env: vec![],
                    kind: TerminalKind::Plain,
                },
            ))
            .id();
        let e3 = app
            .world_mut()
            .spawn((
                Terminal,
                id3,
                AwaitingProcessCreated,
                TerminalLaunch {
                    command: "/bin/sh".into(),
                    args: vec![],
                    cwd: "/tmp/3".into(),
                    env: vec![],
                    kind: TerminalKind::Plain,
                },
            ))
            .id();

        for (process_id, pid) in [(id3, 333u32), (id1, 111), (id2, 222)] {
            let entity = app
                .world_mut()
                .query_filtered::<(bevy::prelude::Entity, &ProcessId), With<AwaitingProcessCreated>>(
                )
                .iter(app.world())
                .find(|(_, pid_c)| **pid_c == process_id)
                .map(|(e, _)| e)
                .expect("matching entity for process_id");
            app.world_mut()
                .run_system_cached_with(
                    |In((entity, process_id, pid)): In<(Entity, ProcessId, u32)>,
                     mut commands: Commands| {
                        apply_process_created(&mut commands, entity, process_id, pid);
                    },
                    (entity, process_id, pid),
                )
                .unwrap();
        }

        let world = app.world();
        assert_eq!(
            world.get::<crate::terminal::pid::Pid>(e1).map(|p| p.0),
            Some(111)
        );
        assert_eq!(
            world.get::<crate::terminal::pid::Pid>(e2).map(|p| p.0),
            Some(222)
        );
        assert_eq!(
            world.get::<crate::terminal::pid::Pid>(e3).map(|p| p.0),
            Some(333)
        );
    }

    #[test]
    fn apply_process_created_stamps_pid_and_process_id() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let entity = app
            .world_mut()
            .spawn((Terminal, AwaitingProcessCreated))
            .id();
        let id = process_id(7);
        let pid_val = 4242u32;
        app.world_mut()
            .run_system_cached_with(
                |In((entity, id, pid_val)): In<(Entity, ProcessId, u32)>,
                 mut commands: Commands| {
                    apply_process_created(&mut commands, entity, id, pid_val);
                },
                (entity, id, pid_val),
            )
            .unwrap();
        let stored_pid = app.world().get::<pid::Pid>(entity).unwrap();
        assert_eq!(stored_pid.0, pid_val);
        assert!(app.world().get::<AwaitingProcessCreated>(entity).is_none());
        let stored_process_id = app.world().get::<ProcessId>(entity).unwrap();
        assert_eq!(*stored_process_id, id);
    }

    #[test]
    fn respawn_shell_on_agent_exit_swaps_kind_and_drops_agent() {
        use crate::terminal::launch::{TerminalKind, TerminalLaunch};

        let mut app = bevy::prelude::App::new();
        let original_id = ProcessId::new();
        let entity = app
            .world_mut()
            .spawn((
                Terminal,
                original_id,
                vmux_agent::session::AgentSession {
                    kind: vmux_agent::AgentKind::Vibe,
                },
                vmux_agent::session::SessionId("abc-123".into()),
                TerminalLaunch {
                    command: "/usr/local/bin/vibe".into(),
                    args: vec!["--trust".into()],
                    cwd: "/work".into(),
                    env: vec![("VIBE_MCP_SERVERS".into(), "[]".into())],
                    kind: TerminalKind::Vibe,
                },
            ))
            .id();

        app.world_mut()
            .run_system_cached_with(
                |In((entity, shell, cwd)): In<(Entity, String, String)>, mut commands: Commands| {
                    respawn_shell_on_agent_exit_for_entity(&mut commands, entity, &shell, cwd);
                },
                (entity, "/bin/zsh".to_string(), "/work".to_string()),
            )
            .unwrap();

        let world = app.world();
        assert!(
            world
                .get::<vmux_agent::session::AgentSession>(entity)
                .is_none()
        );
        assert!(
            world
                .get::<vmux_agent::session::SessionId>(entity)
                .is_none()
        );
        let launch = world.get::<TerminalLaunch>(entity).unwrap();
        assert_eq!(launch.kind, TerminalKind::Plain);
        assert_eq!(launch.command, "/bin/zsh");
        assert_eq!(launch.cwd, "/work");
        assert!(launch.args.is_empty());
        let new_id = world.get::<ProcessId>(entity).copied().unwrap();
        assert_ne!(new_id, original_id);
        assert!(world.get::<PendingServiceCreate>(entity).is_some());
    }
}
