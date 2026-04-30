use crate::{
    browser::Browser,
    command::{AppCommand, TabCommand, WriteAppCommands},
    layout::window::WEBVIEW_MESH_DEPTH_BIAS,
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
use vmux_header::PageMetadata;
use vmux_history::LastActivatedAt;
use vmux_service::{
    client::ServiceHandle,
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

/// Check if a pane has any tab with a live terminal.
pub(crate) fn pane_has_live_terminal(
    pane: Entity,
    pane_children_q: &Query<&Children, With<crate::layout::pane::Pane>>,
    all_children_q: &Query<&Children>,
    terminal_q: &Query<(), (With<Terminal>, Without<ProcessExited>)>,
) -> bool {
    if let Ok(tabs) = pane_children_q.get(pane) {
        tabs.iter()
            .any(|tab| has_live_terminal(tab, all_children_q, terminal_q))
    } else {
        false
    }
}

/// Show confirmation dialog for closing a terminal tab/pane.
pub(crate) fn show_close_dialog() -> bool {
    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    let result = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Close Terminal?")
        .set_description("A process is still running in this terminal. Close anyway?")
        .set_buttons(MessageButtons::OkCancel)
        .show();
    matches!(result, MessageDialogResult::Ok)
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

/// Per-process terminal mode flags, last broadcast by the service.
#[derive(Resource, Default)]
pub(crate) struct TerminalModeMap {
    pub modes: std::collections::HashMap<ProcessId, TerminalModeFlags>,
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
        // Try to connect to an already-running service first.
        if let Some(handle) = ServiceHandle::connect() {
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

        app.init_resource::<MouseSelectionState>()
            .init_resource::<TerminalModeMap>()
            .add_plugins(JsEmitEventPlugin::<TermResizeEvent>::default())
            .add_plugins(JsEmitEventPlugin::<TermMouseEvent>::default())
            .add_systems(
                PreUpdate,
                (
                    handle_terminal_keyboard.run_if(on_message::<KeyboardInput>),
                    handle_terminal_scroll.run_if(on_message::<MouseWheel>),
                )
                    .after(InputSystems),
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
            .add_observer(on_term_ready)
            .add_observer(on_term_resize)
            .add_observer(on_term_mouse)
            .add_observer(on_restart_pty);
    }
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
    match ServiceHandle::connect() {
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
        (With<Terminal>, Without<ProcessExited>),
    >,
    mut meta_q: Query<&mut PageMetadata>,
    service: Option<Res<ServiceClient>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
    mut writer: MessageWriter<AppCommand>,
    mut mode_map: ResMut<TerminalModeMap>,
    mut mouse_state: ResMut<MouseSelectionState>,
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
                mouse_state.per_process.remove(&process_id);
                for (entity, handle, child_of) in &terminals {
                    if handle.process_id == process_id {
                        commands.entity(entity).insert(ProcessExited);
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
    q: Query<&ServiceProcessHandle, (With<Terminal>, With<CefKeyboardTarget>)>,
    input: Res<ButtonInput<KeyCode>>,
    chord_state: Res<crate::shortcut::ChordState>,
    service: Option<Res<ServiceClient>>,
    mode_map: Res<TerminalModeMap>,
) {
    let Some(active_handle) = q.iter().next() else {
        return;
    };
    let active_process_id = active_handle.process_id;
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

    let copy_mode_active = mode_map
        .modes
        .get(&active_process_id)
        .map(|m| m.copy_mode)
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
            if let Some(k) = map_copy_mode_key(&event.logical_key, ctrl) {
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
                        service.0.send(ClientMessage::ProcessInput {
                            process_id: active_process_id,
                            data,
                        });
                    }
                    continue;
                }
                KeyCode::KeyC => {
                    // Round-trip selection through the service, then copy to pasteboard.
                    service.0.send(ClientMessage::GetSelectionText {
                        process_id: active_process_id,
                    });
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
        service.0.send(ClientMessage::ProcessInput {
            process_id: active_process_id,
            data: bytes,
        });
    }
}

/// Translate a Bevy logical key + ctrl modifier into the corresponding
/// tmux-style copy-mode action. Returns None if the key has no copy-mode
/// binding (caller should swallow it regardless).
fn map_copy_mode_key(key: &Key, ctrl: bool) -> Option<vmux_service::protocol::CopyModeKey> {
    use vmux_service::protocol::CopyModeKey as K;
    match (key, ctrl) {
        (Key::ArrowLeft, _) => Some(K::Left),
        (Key::ArrowRight, _) => Some(K::Right),
        (Key::ArrowUp, _) => Some(K::Up),
        (Key::ArrowDown, _) => Some(K::Down),
        (Key::Enter, _) => Some(K::Copy),
        (Key::Escape, _) => Some(K::Exit),
        (Key::Character(s), c) => match (s.as_str(), c) {
            ("h", false) => Some(K::Left),
            ("j", false) => Some(K::Down),
            ("k", false) => Some(K::Up),
            ("l", false) => Some(K::Right),
            ("0", false) => Some(K::LineStart),
            ("$", false) => Some(K::LineEnd),
            ("u", true) => Some(K::PageUp),
            ("d", true) => Some(K::PageDown),
            ("v", false) => Some(K::StartSelection),
            ("y", false) => Some(K::Copy),
            ("q", false) => Some(K::Exit),
            _ => None,
        },
        _ => None,
    }
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
    q: Query<&ServiceProcessHandle, (With<Terminal>, With<CefKeyboardTarget>)>,
    service: Option<Res<ServiceClient>>,
) {
    if q.is_empty() {
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
        for handle in &q {
            for _ in 0..count {
                service.0.send(ClientMessage::ProcessInput {
                    process_id: handle.process_id,
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

/// Tracks the most recent mouse-down per session for click-count detection
/// (300ms / ~1 cell window) and an active drag anchor.
#[derive(Resource, Default)]
struct MouseSelectionState {
    per_process: std::collections::HashMap<ProcessId, MouseSessionState>,
}

#[derive(Default, Clone, Debug)]
struct MouseSessionState {
    last_click: Option<MouseClickRecord>,
    drag_active: bool,
    /// Last (col, row) sent via ExtendSelectionTo during the active drag.
    /// Used to dedupe redundant move events at the same cell.
    last_extend_cell: Option<(u16, u16)>,
}

#[derive(Clone, Copy, Debug)]
struct MouseClickRecord {
    when: std::time::Instant,
    col: u16,
    row: u16,
    count: u8,
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
    let shift = event.modifiers & MOD_SHIFT != 0;
    let is_left = event.button == 0;
    let select_mode = is_left && (!mouse_capture || shift);

    if !select_mode {
        let button = if event.moving {
            event.button + 32
        } else {
            event.button
        };
        let seq = sgr_mouse_sequence(button, event.col, event.row, event.modifiers, event.pressed);
        service.0.send(ClientMessage::ProcessInput {
            process_id,
            data: seq,
        });
        return;
    }

    let entry = state.per_process.entry(process_id).or_default();

    if event.pressed && !event.moving {
        // Mouse-down: detect click count.
        let now = std::time::Instant::now();
        let count = match entry.last_click {
            Some(prev)
                if now.duration_since(prev.when) <= MULTI_CLICK_WINDOW
                    && (prev.col as i32 - event.col as i32).abs() <= MULTI_CLICK_CELL_TOLERANCE
                    && (prev.row as i32 - event.row as i32).abs() <= MULTI_CLICK_CELL_TOLERANCE =>
            {
                // Wrap back to 1 on the 4th click (browser behavior).
                if prev.count >= 3 { 1 } else { prev.count + 1 }
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
        entry.last_extend_cell = Some((event.col, event.row));

        match count {
            1 if shift => {
                service.0.send(ClientMessage::ExtendSelectionTo {
                    process_id,
                    col: event.col,
                    row: event.row,
                });
            }
            1 => {
                service.0.send(ClientMessage::SetSelection {
                    process_id,
                    range: Some(TermSelectionRange {
                        start_col: event.col,
                        start_row: event.row,
                        end_col: event.col,
                        end_row: event.row,
                        is_block: false,
                    }),
                });
            }
            2 => service.0.send(ClientMessage::SelectWordAt {
                process_id,
                col: event.col,
                row: event.row,
            }),
            _ => service.0.send(ClientMessage::SelectLineAt {
                process_id,
                row: event.row,
            }),
        }
    } else if event.moving && entry.drag_active {
        // Dedupe: only send when the cursor crosses into a new cell.
        if entry.last_extend_cell != Some((event.col, event.row)) {
            entry.last_extend_cell = Some((event.col, event.row));
            service.0.send(ClientMessage::ExtendSelectionTo {
                process_id,
                col: event.col,
                row: event.row,
            });
        }
    } else if !event.pressed {
        entry.drag_active = false;
        entry.last_extend_cell = None;
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

/// Consume `AppCommand::Terminal::CopyMode` and ask the service to enter copy mode
/// for the currently focused terminal process.
fn handle_terminal_copy_mode_command(
    mut er: MessageReader<AppCommand>,
    q: Query<&ServiceProcessHandle, (With<Terminal>, With<CefKeyboardTarget>)>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        for _ in er.read() {}
        return;
    };
    let active_process_id = q.iter().next().map(|h| h.process_id);
    for cmd in er.read() {
        if matches!(
            cmd,
            AppCommand::Terminal(crate::command::TerminalCommand::CopyMode)
        ) && let Some(process_id) = active_process_id
        {
            service.0.send(ClientMessage::EnterCopyMode { process_id });
        }
    }
}
