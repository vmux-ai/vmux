use crate::{
    browser::Browser,
    command::{AppCommand, TabCommand, WriteAppCommands},
    layout::window::WEBVIEW_MESH_DEPTH_BIAS,
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
use vmux_layout::{CloseRequiresConfirmation, LayoutSpawnRequest};
use vmux_service::{
    client::{ServiceHandle, ServiceWake},
    protocol::{ClientMessage, ProcessId, ServiceMessage},
};
use vmux_terminal::event::*;
use vmux_webview_app::UiReady;

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

/// Associates a terminal entity with a service-managed process.
#[derive(Component)]
pub(crate) struct ServiceProcessHandle {
    pub process_id: ProcessId,
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

/// Tracks service connection retry state.
#[derive(Resource)]
struct ServiceConnectRetry {
    /// Countdown: stop retrying after this many ticks.
    remaining_attempts: u32,
    timer: Timer,
}

pub(crate) struct TerminalInputPlugin;

impl Plugin for TerminalInputPlugin {
    fn build(&self, app: &mut App) {
        let service_wake = service_wake_callback(app);
        // Try to connect to an already-running service first.
        if let Some(handle) = ServiceHandle::connect_with_wake(service_wake.clone()) {
            eprintln!("vmux: connected to existing service");
            app.insert_resource(ServiceClient(handle));
        } else {
            // Service not running — auto-start it and schedule connection retries.
            ensure_service_started();
            app.insert_resource(ServiceConnectRetry {
                remaining_attempts: 60, // ~3 seconds with 50ms timer
                timer: Timer::from_seconds(0.05, TimerMode::Repeating),
            });
        }
        app.insert_resource(ServiceWakeCallback(service_wake));

        app.init_resource::<MouseSelectionState>()
            .init_resource::<TerminalModeMap>()
            .init_resource::<LocalCopyModeState>()
            .add_plugins(JsEmitEventPlugin::<TermResizeEvent>::default())
            .add_plugins(JsEmitEventPlugin::<TermMouseEvent>::default())
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
            .add_observer(on_restart_pty);
    }
}

fn add_terminal_update_systems(app: &mut App) -> &mut App {
    app.add_systems(
        Update,
        spawn_layout_requested_content.after(crate::layout::tab::TabCommandSet),
    )
    .add_systems(
        Update,
        (
            try_connect_service.run_if(resource_exists::<ServiceConnectRetry>),
            poll_service_messages.in_set(WriteAppCommands),
            handle_terminal_copy_mode_command.in_set(crate::command::ReadAppCommands),
            sync_terminal_theme,
        )
            .chain(),
    )
}

fn spawn_layout_requested_content(
    mut reader: MessageReader<LayoutSpawnRequest>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for request in reader.read() {
        match *request {
            LayoutSpawnRequest::Terminal { tab } => {
                let terminal = commands
                    .spawn((
                        Terminal::new(&mut meshes, &mut webview_mt, &settings),
                        ChildOf(tab),
                    ))
                    .id();
                commands.entity(terminal).insert(CefKeyboardTarget);
            }
            LayoutSpawnRequest::ProcessesMonitor { tab } => {
                commands.spawn((
                    ProcessesMonitor::new(&mut meshes, &mut webview_mt),
                    ChildOf(tab),
                ));
            }
        }
    }
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

        // ProcessId is set to a placeholder here; the actual process is created
        // in a startup system that sends CreateProcess to the service once the
        // ServiceClient resource is available.
        let process_id = ProcessId::new();

        (
            (
                Self,
                Browser,
                CloseRequiresConfirmation,
                ServiceProcessHandle { process_id },
                PendingServiceCreate {
                    shell,
                    cwd: cwd_str,
                },
                PageMetadata {
                    title: format!("Terminal ({})", &process_id.to_string()[..8]),
                    url: format!("{}{}", TERMINAL_WEBVIEW_URL, process_id),
                    favicon_url: String::new(),
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
    #[allow(dead_code)] // Used by persistence.rs for process reconnect
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
                ServiceProcessHandle { process_id },
                PendingServiceAttach,
                PageMetadata {
                    title: format!("Terminal ({})", &process_id.to_string()[..8]),
                    url: format!("{}{}", TERMINAL_WEBVIEW_URL, process_id),
                    favicon_url: String::new(),
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

/// Temporary component: terminal needs a CreateProcess sent to service.
#[derive(Component)]
struct PendingServiceCreate {
    shell: String,
    cwd: String,
}

/// Temporary component: terminal needs an AttachProcess sent to service.
#[derive(Component)]
struct PendingServiceAttach;

/// Marker: CreateProcess was sent, waiting for ProcessCreated response.
#[derive(Component)]
struct AwaitingProcessCreated;

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

struct MissingTerminalRestart {
    entity: Entity,
    command: ClientMessage,
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
    terminals: impl IntoIterator<Item = (Entity, ProcessId)>,
    settings: &AppSettings,
) -> Option<MissingTerminalRestart> {
    terminals
        .into_iter()
        .find(|(_, terminal_process_id)| *terminal_process_id == process_id)
        .map(|(entity, _)| MissingTerminalRestart {
            entity,
            command: ClientMessage::CreateProcess {
                shell: terminal_shell(settings),
                cwd: String::new(),
                env: Vec::new(),
                cols: 80,
                rows: 24,
            },
        })
}

fn missing_process_id(message: &str) -> Option<ProcessId> {
    message
        .strip_prefix("process not found: ")
        .and_then(|id| id.parse().ok())
}

/// Spawn the service subprocess if not already running.
fn ensure_service_started() {
    if ServiceHandle::service_running() {
        eprintln!("vmux: service already running");
        return;
    }
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("vmux: failed to get current exe: {e}");
            return;
        }
    };
    eprintln!("vmux: starting service: {} service", exe.display());

    // Redirect service stderr to a log file instead of piping.
    // Piped stderr with nobody reading causes SIGPIPE on macOS,
    // which can kill the service process.
    let log_dir = vmux_service::service_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let stderr_cfg = match std::fs::File::create(log_dir.join("service.log")) {
        Ok(f) => std::process::Stdio::from(f),
        Err(_) => std::process::Stdio::null(),
    };

    match std::process::Command::new(&exe)
        .arg("service")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(stderr_cfg)
        .spawn()
    {
        Ok(_child) => {
            eprintln!("vmux: service subprocess spawned");
        }
        Err(e) => {
            eprintln!("vmux: failed to spawn service: {e}");
        }
    }
}

/// Bevy system: retry connecting to service until it succeeds or we give up.
fn try_connect_service(
    mut retry: ResMut<ServiceConnectRetry>,
    time: Res<Time>,
    mut commands: Commands,
    wake: Res<ServiceWakeCallback>,
) {
    retry.timer.tick(time.delta());
    if !retry.timer.just_finished() {
        return;
    }

    retry.remaining_attempts = retry.remaining_attempts.saturating_sub(1);

    // Check if socket is ready
    let sock = vmux_service::socket_path();
    if !sock.exists() {
        if retry.remaining_attempts == 0 {
            eprintln!("vmux: service socket never appeared — giving up");
            commands.remove_resource::<ServiceConnectRetry>();
        }
        return;
    }

    // Try to connect
    match ServiceHandle::connect_with_wake(wake.0.clone()) {
        Some(handle) => {
            eprintln!("vmux: connected to service after retry");
            commands.insert_resource(ServiceClient(handle));
            commands.remove_resource::<ServiceConnectRetry>();
        }
        None => {
            if retry.remaining_attempts == 0 {
                eprintln!("vmux: failed to connect to service after all retries");
                // Check service log for clues
                let log_path = vmux_service::service_dir().join("service.log");
                if let Ok(log) = std::fs::read_to_string(&log_path)
                    && !log.is_empty()
                {
                    eprintln!("vmux: service log:\n{log}");
                }
                commands.remove_resource::<ServiceConnectRetry>();
            }
        }
    }
}

/// Send CreateProcess / AttachProcess for newly spawned terminals.
fn poll_service_messages(
    pending_create: Query<(Entity, &ServiceProcessHandle, &PendingServiceCreate), With<Terminal>>,
    pending_attach: Query<
        (Entity, &ServiceProcessHandle),
        (With<Terminal>, With<PendingServiceAttach>),
    >,
    awaiting_create: Query<
        (Entity, &ServiceProcessHandle, &ChildOf),
        (With<Terminal>, With<AwaitingProcessCreated>),
    >,
    terminals: Query<
        (Entity, &ServiceProcessHandle, &ChildOf),
        (
            With<Terminal>,
            Without<ProcessExited>,
            Without<AwaitingProcessCreated>,
        ),
    >,
    mut meta_q: Query<&mut PageMetadata>,
    service: Option<Res<ServiceClient>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
    mut writer: MessageWriter<AppCommand>,
    mut mode_map: ResMut<TerminalModeMap>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
    mut mouse_state: ResMut<MouseSelectionState>,
    settings: Res<AppSettings>,
) {
    let Some(service) = service else { return };

    // Handle pending creates — send CreateProcess, wait for ProcessCreated
    // response which will carry the real process ID.
    for (entity, _handle, pending) in &pending_create {
        service.0.send(ClientMessage::CreateProcess {
            shell: pending.shell.clone(),
            cwd: pending.cwd.clone(),
            env: Vec::new(),
            cols: 80,
            rows: 24,
        });
        commands
            .entity(entity)
            .remove::<PendingServiceCreate>()
            .insert(AwaitingProcessCreated);
    }

    // Handle pending attaches
    for (entity, handle) in &pending_attach {
        service.0.send(ClientMessage::AttachProcess {
            process_id: handle.process_id,
        });
        service.0.send(ClientMessage::RequestSnapshot {
            process_id: handle.process_id,
        });
        commands.entity(entity).remove::<PendingServiceAttach>();
    }

    // Drain service messages and dispatch
    let mut matched_entities = Vec::new();
    let mut restarted_missing_processes = Vec::new();
    for msg in service.0.drain() {
        match msg {
            ServiceMessage::ProcessCreated { process_id } => {
                // Match the first unmatched terminal awaiting a ProcessCreated response.
                if let Some((entity, _, _)) = (&awaiting_create)
                    .into_iter()
                    .find(|(e, _, _)| !matched_entities.contains(e))
                {
                    matched_entities.push(entity);
                    // Attach to receive viewport patches
                    service.0.send(ClientMessage::AttachProcess { process_id });
                    // Update handle with real service-managed process ID
                    commands
                        .entity(entity)
                        .insert(ServiceProcessHandle { process_id })
                        .remove::<AwaitingProcessCreated>();
                    // Update PageMetadata URL so persistence saves the real ID
                    if let Ok(mut meta) = meta_q.get_mut(entity) {
                        meta.url = format!("{}{}", TERMINAL_WEBVIEW_URL, process_id);
                        meta.title = format!("Terminal ({})", &process_id.to_string()[..8]);
                    }
                }
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
                for (entity, handle, _) in &terminals {
                    if handle.process_id == process_id {
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
                        commands.trigger(HostEmitEvent::new(entity, TERM_VIEWPORT_EVENT, &patch));
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
                for (entity, handle, _) in &terminals {
                    if handle.process_id == process_id {
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
                        commands.trigger(HostEmitEvent::new(entity, TERM_VIEWPORT_EVENT, &patch));
                        break;
                    }
                }
            }
            ServiceMessage::ProcessExited {
                process_id,
                exit_code: _,
            } => {
                // Drop per-process caches so they don't leak across the app lifetime.
                mode_map.modes.remove(&process_id);
                set_local_copy_mode(&mut local_copy_mode, process_id, false);
                mouse_state.per_process.remove(&process_id);
                for (entity, handle, child_of) in &terminals {
                    if handle.process_id == process_id {
                        commands
                            .entity(entity)
                            .insert(ProcessExited)
                            .remove::<CloseRequiresConfirmation>();
                        let tab = child_of.get();
                        commands.entity(tab).insert(LastActivatedAt::now());
                        writer.write(AppCommand::Tab(TabCommand::Close));
                        break;
                    }
                }
            }
            ServiceMessage::ProcessList { processes } => {
                commands
                    .insert_resource(crate::processes_monitor::ServiceProcessList { processes });
            }
            ServiceMessage::Error { message } => {
                if let Some(process_id) = missing_process_id(&message)
                    && !restarted_missing_processes.contains(&process_id)
                {
                    let terminals = terminals
                        .iter()
                        .map(|(entity, handle, _)| (entity, handle.process_id));
                    if let Some(restart) =
                        missing_terminal_restart(process_id, terminals, &settings)
                    {
                        restarted_missing_processes.push(process_id);
                        service.0.send(restart.command);
                        commands
                            .entity(restart.entity)
                            .insert(AwaitingProcessCreated);
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
            } => {
                if !text.is_empty() {
                    crate::clipboard::write(text.clone());
                }
            }
            _ => {} // ProcessOutput handled elsewhere if needed
        }
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
    targeted_terminals: Query<&ServiceProcessHandle, (With<Terminal>, With<CefKeyboardTarget>)>,
    keyboard_targets: Query<(), With<CefKeyboardTarget>>,
    terminals: Query<(&ServiceProcessHandle, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    focus: Res<crate::layout::tab::FocusedTab>,
    mode: Res<crate::scene::InteractionMode>,
    input: Res<ButtonInput<KeyCode>>,
    chord_state: Res<crate::shortcut::ChordState>,
    service: Option<Res<ServiceClient>>,
    mode_map: Res<TerminalModeMap>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
) {
    let target_processes = resolve_terminal_input_targets(
        targeted_terminals.iter().map(|handle| handle.process_id),
        !keyboard_targets.is_empty(),
        focus.tab,
        terminals
            .iter()
            .map(|(handle, child_of)| (child_of.get(), handle.process_id)),
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
    targeted_terminals: Query<&ServiceProcessHandle, (With<Terminal>, With<CefKeyboardTarget>)>,
    keyboard_targets: Query<(), With<CefKeyboardTarget>>,
    terminals: Query<(&ServiceProcessHandle, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    focus: Res<crate::layout::tab::FocusedTab>,
    mode: Res<crate::scene::InteractionMode>,
    service: Option<Res<ServiceClient>>,
) {
    let target_processes = resolve_terminal_input_targets(
        targeted_terminals.iter().map(|handle| handle.process_id),
        !keyboard_targets.is_empty(),
        focus.tab,
        terminals
            .iter()
            .map(|(handle, child_of)| (child_of.get(), handle.process_id)),
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
    trigger: On<Receive<TermMouseEvent>>,
    q: Query<&ServiceProcessHandle, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
    mode_map: Res<TerminalModeMap>,
    mut state: ResMut<MouseSelectionState>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(service) = service else { return };
    let Ok(handle) = q.get(entity) else { return };
    let process_id = handle.process_id;

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
    q: Query<&ServiceProcessHandle, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
) {
    let entity = trigger.event_target();
    let Some(service) = service else { return };
    if let Ok(handle) = q.get(entity) {
        // Request a full snapshot when webview is ready
        service.0.send(ClientMessage::RequestSnapshot {
            process_id: handle.process_id,
        });
    }
}

/// Handle resize event from webview (reports char cell dimensions).
fn on_term_resize(
    trigger: On<Receive<TermResizeEvent>>,
    webview_q: Query<&WebviewSize, With<Terminal>>,
    handle_q: Query<&ServiceProcessHandle, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(service) = service else { return };

    let Ok(webview_size) = webview_q.get(entity) else {
        return;
    };
    let Ok(handle) = handle_q.get(entity) else {
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
        process_id: handle.process_id,
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
    let body = ron::ser::to_string(&event).unwrap_or_default();

    let targets: Vec<Entity> = if theme_changed {
        q.iter().collect()
    } else {
        new_terminals.iter().chain(newly_ready.iter()).collect()
    };

    for entity in targets {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(HostEmitEvent::new(entity, TERM_THEME_EVENT, &body));
        }
    }
}

fn on_restart_pty(
    trigger: On<RestartPty>,
    mut q: Query<(&mut ServiceProcessHandle, &mut PageMetadata)>,
    service: Option<Res<ServiceClient>>,
    settings: Res<AppSettings>,
) {
    let entity = trigger.event().entity;
    let Some(service) = service else { return };
    let Ok((mut handle, mut meta)) = q.get_mut(entity) else {
        return;
    };

    // Kill old process

    service.0.send(ClientMessage::KillProcess {
        process_id: handle.process_id,
    });

    let shell = settings
        .terminal
        .as_ref()
        .map(|t| t.resolve_theme(&t.default_theme).shell)
        .unwrap_or_else(default_shell);

    // Create new process
    let new_id = ProcessId::new();
    service.0.send(ClientMessage::CreateProcess {
        shell,
        cwd: String::new(),
        env: Vec::new(),
        cols: 80,
        rows: 24,
    });
    service
        .0
        .send(ClientMessage::AttachProcess { process_id: new_id });

    handle.process_id = new_id;
    meta.url = format!("{}{}", TERMINAL_WEBVIEW_URL, new_id);
    meta.title = format!("Terminal ({})", &new_id.to_string()[..8]);
}

/// Consume `AppCommand::Terminal::CopyMode` and ask the service to enter
/// visual/copy mode for the currently focused terminal process.
fn handle_terminal_copy_mode_command(
    mut er: MessageReader<AppCommand>,
    targeted_terminals: Query<&ServiceProcessHandle, (With<Terminal>, With<CefKeyboardTarget>)>,
    keyboard_targets: Query<(), With<CefKeyboardTarget>>,
    terminals: Query<(&ServiceProcessHandle, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    focus: Res<crate::layout::tab::FocusedTab>,
    mode: Res<crate::scene::InteractionMode>,
    service: Option<Res<ServiceClient>>,
    mut local_copy_mode: ResMut<LocalCopyModeState>,
) {
    let Some(service) = service else {
        for _ in er.read() {}
        return;
    };
    let target_processes = resolve_terminal_input_targets(
        targeted_terminals.iter().map(|handle| handle.process_id),
        !keyboard_targets.is_empty(),
        focus.tab,
        terminals
            .iter()
            .map(|(handle, child_of)| (child_of.get(), handle.process_id)),
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
    use crate::settings::{
        BrowserSettings, FocusRingSettings, LayoutSettings, PaneSettings, ShortcutSettings,
        SideSheetSettings, WindowSettings,
    };
    use bevy::ecs::schedule::Schedules;

    fn process_id(byte: u8) -> ProcessId {
        ProcessId([byte; 16])
    }

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings {
                    gap: 0.0,
                    radius: 0.0,
                },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
        }
    }

    #[test]
    fn missing_service_process_restarts_matching_terminal() {
        let missing = process_id(7);
        let target = Entity::from_bits(1);
        let restart = missing_terminal_restart(
            missing,
            [(Entity::from_bits(2), process_id(8)), (target, missing)],
            &test_settings(),
        )
        .unwrap();

        assert_eq!(restart.entity, target);
        assert!(matches!(
            restart.command,
            ClientMessage::CreateProcess {
                shell,
                cwd,
                env,
                cols: 80,
                rows: 24
            } if shell == default_shell() && cwd.is_empty() && env.is_empty()
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
            .add_plugins(vmux_layout::tab::TabPlugin)
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
}
