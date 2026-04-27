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
use vmux_daemon::{
    client::DaemonHandle,
    protocol::{ClientMessage, DaemonMessage, SessionId},
};
use vmux_header::PageMetadata;
use vmux_history::LastActivatedAt;
use vmux_terminal::event::*;
use vmux_webview_app::UiReady;

/// Marker component for terminal content entities (analogous to Browser).
#[derive(Component)]
pub(crate) struct Terminal;

/// Marker: daemon session has exited; tab close is pending.
#[derive(Component)]
struct SessionExited;

/// Associates a terminal entity with a daemon session.
#[derive(Component)]
pub(crate) struct DaemonSessionHandle {
    pub session_id: SessionId,
}

/// Bevy resource wrapping the daemon connection.
#[derive(Resource)]
pub(crate) struct DaemonClient(pub DaemonHandle);

/// Triggered to restart the terminal session for a terminal entity.
#[derive(Event)]
pub(crate) struct RestartPty {
    pub entity: Entity,
}

pub(crate) struct TerminalInputPlugin;

impl Plugin for TerminalInputPlugin {
    fn build(&self, app: &mut App) {
        // Connect to daemon at startup
        if let Some(handle) = DaemonHandle::connect() {
            app.insert_resource(DaemonClient(handle));
        } else {
            // Spawn daemon process automatically
            let daemon_bin = std::env::current_exe()
                .ok()
                .and_then(|p| {
                    let dir = p.parent()?;
                    let candidate = dir.join("vmux-daemon");
                    candidate.exists().then_some(candidate)
                });
            if let Some(bin) = daemon_bin {
                let _ = std::process::Command::new(&bin)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                // Wait briefly for daemon to start
                std::thread::sleep(std::time::Duration::from_millis(200));
                if let Some(handle) = DaemonHandle::connect() {
                    app.insert_resource(DaemonClient(handle));
                } else {
                    warn!("Failed to connect to vmux daemon after spawn");
                }
            } else {
                warn!("vmux-daemon binary not found; terminal sessions will not persist");
            }
        }

        app.init_resource::<MouseSelectionState>()
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
                    poll_daemon_messages.in_set(WriteAppCommands),
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

        // SessionId is set to a placeholder here; the actual session is created
        // in a startup system that sends CreateSession to the daemon once the
        // DaemonClient resource is available.
        let session_id = SessionId::new();

        (
            (
                Self,
                Browser,
                DaemonSessionHandle { session_id },
                PendingDaemonCreate {
                    shell,
                    cwd: cwd_str,
                },
                PageMetadata {
                    title: format!("Terminal ({})", &session_id.to_string()[..8]),
                    url: format!("{}session/{}", TERMINAL_WEBVIEW_URL, session_id),
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

    /// Create a terminal bundle that reattaches to an existing daemon session.
    #[allow(dead_code)] // Used by persistence.rs for session reconnect
    pub(crate) fn reattach(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        session_id: SessionId,
    ) -> impl Bundle {
        (
            (
                Self,
                Browser,
                DaemonSessionHandle { session_id },
                PendingDaemonAttach,
                PageMetadata {
                    title: format!("Terminal ({})", &session_id.to_string()[..8]),
                    url: format!("{}session/{}", TERMINAL_WEBVIEW_URL, session_id),
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

/// Temporary component: terminal needs a CreateSession sent to daemon.
#[derive(Component)]
struct PendingDaemonCreate {
    shell: String,
    cwd: String,
}

/// Temporary component: terminal needs an AttachSession sent to daemon.
#[derive(Component)]
struct PendingDaemonAttach;

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

/// Send CreateSession / AttachSession for newly spawned terminals.
fn poll_daemon_messages(
    pending_create: Query<
        (Entity, &DaemonSessionHandle, &PendingDaemonCreate),
        With<Terminal>,
    >,
    pending_attach: Query<
        (Entity, &DaemonSessionHandle),
        (With<Terminal>, With<PendingDaemonAttach>),
    >,
    terminals: Query<
        (Entity, &DaemonSessionHandle, &ChildOf),
        (With<Terminal>, Without<SessionExited>),
    >,
    mut meta_q: Query<&mut PageMetadata>,
    daemon: Option<Res<DaemonClient>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
    mut writer: MessageWriter<AppCommand>,
) {
    let Some(daemon) = daemon else { return };

    // Handle pending creates — send CreateSession, wait for SessionCreated
    // response which will carry the real session ID.
    for (entity, _handle, pending) in &pending_create {
        daemon.0.send(ClientMessage::CreateSession {
            shell: pending.shell.clone(),
            cwd: pending.cwd.clone(),
            env: Vec::new(),
            cols: 80,
            rows: 24,
        });
        commands.entity(entity).remove::<PendingDaemonCreate>();
    }

    // Handle pending attaches
    for (entity, handle) in &pending_attach {
        daemon.0.send(ClientMessage::AttachSession {
            session_id: handle.session_id,
        });
        daemon.0.send(ClientMessage::RequestSnapshot {
            session_id: handle.session_id,
        });
        commands.entity(entity).remove::<PendingDaemonAttach>();
    }

    // Drain daemon messages and dispatch
    for msg in daemon.0.drain() {
        match msg {
            DaemonMessage::SessionCreated { session_id } => {
                // Update the placeholder session_id on the first terminal
                // that doesn't yet have a real daemon session.
                // CreateSession responses arrive in order.
                for (entity, _, _) in &terminals {
                    // Attach to receive viewport patches
                    daemon.0.send(ClientMessage::AttachSession { session_id });
                    // Update handle with real daemon session ID
                    commands
                        .entity(entity)
                        .insert(DaemonSessionHandle { session_id });
                    // Update PageMetadata URL so session.ron saves the real ID
                    if let Ok(mut meta) = meta_q.get_mut(entity) {
                        meta.url =
                            format!("{}session/{}", TERMINAL_WEBVIEW_URL, session_id);
                        meta.title = format!(
                            "Terminal ({})",
                            &session_id.to_string()[..8]
                        );
                    }
                    break;
                }
            }
            DaemonMessage::ViewportPatch {
                session_id,
                changed_lines,
                cursor,
                cols,
                rows,
                selection,
                full,
            } => {
                for (entity, handle, _) in &terminals {
                    if handle.session_id == session_id {
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
                        commands.trigger(HostEmitEvent::new(
                            entity,
                            TERM_VIEWPORT_EVENT,
                            &patch,
                        ));
                        break;
                    }
                }
            }
            DaemonMessage::Snapshot {
                session_id,
                lines,
                cursor,
                cols,
                rows,
            } => {
                for (entity, handle, _) in &terminals {
                    if handle.session_id == session_id {
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
                        commands.trigger(HostEmitEvent::new(
                            entity,
                            TERM_VIEWPORT_EVENT,
                            &patch,
                        ));
                        break;
                    }
                }
            }
            DaemonMessage::SessionExited {
                session_id,
                exit_code: _,
            } => {
                for (entity, handle, child_of) in &terminals {
                    if handle.session_id == session_id {
                        commands.entity(entity).insert(SessionExited);
                        let tab = child_of.get();
                        commands.entity(tab).insert(LastActivatedAt::now());
                        writer.write(AppCommand::Tab(TabCommand::Close));
                        break;
                    }
                }
            }
            DaemonMessage::Error { message } => {
                warn!("Daemon error: {message}");
            }
            _ => {} // SessionList, SessionOutput handled elsewhere if needed
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
fn handle_terminal_keyboard(
    mut er: MessageReader<KeyboardInput>,
    q: Query<&DaemonSessionHandle, (With<Terminal>, With<CefKeyboardTarget>)>,
    input: Res<ButtonInput<KeyCode>>,
    chord_state: Res<crate::shortcut::ChordState>,
    daemon: Option<Res<DaemonClient>>,
) {
    if q.is_empty() {
        return;
    }
    let Some(daemon) = daemon else {
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

        if super_key {
            match event.key_code {
                KeyCode::KeyV => {
                    // Paste via pbpaste
                    if let Ok(output) = std::process::Command::new("pbpaste").output()
                        && output.status.success()
                    {
                        let text = String::from_utf8_lossy(&output.stdout);
                        if !text.is_empty() {
                            // Wrap in bracketed paste sequences
                            let mut data = Vec::new();
                            data.extend_from_slice(b"\x1b[200~");
                            data.extend_from_slice(text.as_bytes());
                            data.extend_from_slice(b"\x1b[201~");
                            for handle in &q {
                                daemon.0.send(ClientMessage::SessionInput {
                                    session_id: handle.session_id,
                                    data: data.clone(),
                                });
                            }
                        }
                    }
                    continue;
                }
                KeyCode::KeyC => {
                    // Copy: in daemon mode we can't access selection, so skip
                    // TODO: implement copy via daemon snapshot
                    continue;
                }
                _ => continue,
            }
        }

        // Skip selection keys (Shift+Arrow etc) — daemon doesn't support local selection
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
        for handle in &q {
            daemon.0.send(ClientMessage::SessionInput {
                session_id: handle.session_id,
                data: bytes.clone(),
            });
        }
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

/// Handle mouse wheel scrolling — sends scroll input to daemon.
fn handle_terminal_scroll(
    mut er: MessageReader<MouseWheel>,
    q: Query<&DaemonSessionHandle, (With<Terminal>, With<CefKeyboardTarget>)>,
    daemon: Option<Res<DaemonClient>>,
) {
    if q.is_empty() {
        return;
    }
    let Some(daemon) = daemon else {
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
                daemon.0.send(ClientMessage::SessionInput {
                    session_id: handle.session_id,
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

/// Tracks mouse state for selection (reserved for future local selection).
#[derive(Resource, Default)]
struct MouseSelectionState;

/// Handle mouse events from the terminal webview — forward to daemon as input.
fn on_term_mouse(
    trigger: On<Receive<TermMouseEvent>>,
    q: Query<&DaemonSessionHandle, With<Terminal>>,
    daemon: Option<Res<DaemonClient>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(daemon) = daemon else { return };
    let Ok(handle) = q.get(entity) else { return };

    // Forward all mouse events as SGR sequences to daemon
    let button = if event.moving {
        event.button + 32
    } else {
        event.button
    };
    let seq = sgr_mouse_sequence(button, event.col, event.row, event.modifiers, event.pressed);
    daemon.0.send(ClientMessage::SessionInput {
        session_id: handle.session_id,
        data: seq,
    });
}

/// Mark dirty when webview becomes ready so initial viewport is sent.
fn on_term_ready(
    trigger: On<Add, UiReady>,
    q: Query<&DaemonSessionHandle, With<Terminal>>,
    daemon: Option<Res<DaemonClient>>,
) {
    let entity = trigger.event_target();
    let Some(daemon) = daemon else { return };
    if let Ok(handle) = q.get(entity) {
        // Request a full snapshot when webview is ready
        daemon.0.send(ClientMessage::RequestSnapshot {
            session_id: handle.session_id,
        });
    }
}

/// Handle resize event from webview (reports char cell dimensions).
fn on_term_resize(
    trigger: On<Receive<TermResizeEvent>>,
    webview_q: Query<&WebviewSize, With<Terminal>>,
    handle_q: Query<&DaemonSessionHandle, With<Terminal>>,
    daemon: Option<Res<DaemonClient>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(daemon) = daemon else { return };

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

    daemon.0.send(ClientMessage::ResizeSession {
        session_id: handle.session_id,
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
    mut q: Query<(&mut DaemonSessionHandle, &mut PageMetadata)>,
    daemon: Option<Res<DaemonClient>>,
    settings: Res<AppSettings>,
) {
    let entity = trigger.event().entity;
    let Some(daemon) = daemon else { return };
    let Ok((mut handle, mut meta)) = q.get_mut(entity) else {
        return;
    };

    // Kill old session
    daemon.0.send(ClientMessage::KillSession {
        session_id: handle.session_id,
    });

    let shell = settings
        .terminal
        .as_ref()
        .map(|t| t.resolve_theme(&t.default_theme).shell)
        .unwrap_or_else(default_shell);

    // Create new session
    let new_id = SessionId::new();
    daemon.0.send(ClientMessage::CreateSession {
        shell,
        cwd: String::new(),
        env: Vec::new(),
        cols: 80,
        rows: 24,
    });
    daemon.0.send(ClientMessage::AttachSession {
        session_id: new_id,
    });

    handle.session_id = new_id;
    meta.url = format!("{}session/{}", TERMINAL_WEBVIEW_URL, new_id);
    meta.title = format!("Terminal ({})", &new_id.to_string()[..8]);
}
