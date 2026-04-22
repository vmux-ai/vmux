use crate::{
    browser::Browser,
    command::{AppCommand, TabCommand},
    layout::window::WEBVIEW_MESH_DEPTH_BIAS,
    settings::AppSettings,
};
use alacritty_terminal::{
    event::{Event as TermEvent, EventListener as TermEventListener},
    grid::{Dimensions, Scroll},
    index::{Column, Line},
    term::{Config as TermConfig, Term, TermMode, cell::Flags as CellFlags},
    vte::ansi::{Color, NamedColor, Processor},
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
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::{
    io::{Read, Write},
    sync::{mpsc, Arc, Mutex},
};
use vmux_header::PageMetadata;
use vmux_history::LastActivatedAt;
use vmux_terminal::event::*;
use vmux_webview_app::UiReady;

/// Marker component for terminal content entities (analogous to Browser).
#[derive(Component)]
pub(crate) struct Terminal;

/// Marker: PTY child process has exited; tab close is pending.
#[derive(Component)]
struct PtyExited;

/// Holds the alacritty_terminal state for a terminal instance.
#[derive(Component)]
pub(crate) struct TerminalState {
    term: Term<VmuxEventProxy>,
    processor: Processor,
    dirty: bool,
}

/// Receives PTY output from a background reader thread.
#[derive(Component)]
pub(crate) struct PtyHandle {
    rx: Mutex<mpsc::Receiver<Vec<u8>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
}

/// Triggered to restart the PTY process for a terminal entity.
#[derive(Event)]
pub(crate) struct RestartPty {
    pub entity: Entity,
}

/// Event proxy that forwards PtyWrite responses back to the PTY.
#[derive(Clone)]
pub(crate) struct VmuxEventProxy {
    pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl TermEventListener for VmuxEventProxy {
    fn send_event(&self, event: TermEvent) {
        if let TermEvent::PtyWrite(text) = event {
            if let Ok(mut writer) = self.pty_writer.lock() {
                let _ = writer.write_all(text.as_bytes());
            }
        }
    }
}

pub(crate) struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsEmitEventPlugin::<TermResizeEvent>::default())
            .add_plugins(JsEmitEventPlugin::<TermMouseEvent>::default())
            .add_systems(
                PreUpdate,
                (
                    handle_terminal_keyboard
                        .run_if(on_message::<KeyboardInput>),
                    handle_terminal_scroll
                        .run_if(on_message::<MouseWheel>),
                )
                    .after(InputSystems),
            )
            .add_systems(Update, (poll_pty_output, sync_terminal_viewport, sync_terminal_theme).chain())
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
        let cols = 80u16;
        let rows = 24u16;

        let shell = settings
            .terminal
            .as_ref()
            .map(|t| t.resolve_theme(&t.default_theme).shell)
            .unwrap_or_else(default_shell);

        // Create PTY
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("failed to open PTY");

        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair.slave.spawn_command(cmd).expect("failed to spawn shell");
        let pid = child.process_id().unwrap_or(0);
        let reader = pair
            .master
            .try_clone_reader()
            .expect("failed to clone PTY reader");
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(
            pair.master
                .take_writer()
                .expect("failed to take PTY writer"),
        ));
        drop(pair.slave);

        // Spawn background reader thread
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || {
                pty_reader_thread(reader, tx);
            })
            .expect("failed to spawn PTY reader thread");

        // Create alacritty terminal with event proxy that can write back to PTY
        let event_proxy = VmuxEventProxy {
            pty_writer: Arc::clone(&writer),
        };
        let term_config = TermConfig::default();
        let dims = PtyDimensions { cols, rows };
        let term = Term::new(term_config, &dims, event_proxy);
        let processor = Processor::new();

        (
            (
                Self,
                Browser,
                TerminalState {
                    term,
                    processor,
                    dirty: true,
                },
                PtyHandle {
                    rx: Mutex::new(rx),
                    writer,
                    master: Mutex::new(pair.master),
                    child: Mutex::new(child),
                },
                PageMetadata {
                    title: format!("Terminal - {}", shell),
                    url: format!("{}?session={}", TERMINAL_WEBVIEW_URL, pid),
                    favicon_url: String::new(),
                },
                WebviewSource::new(TERMINAL_WEBVIEW_URL),
                ResolvedWebviewUri(TERMINAL_WEBVIEW_URL.to_string()),
                Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
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

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

/// Background thread that reads from PTY and sends chunks via channel.
fn pty_reader_thread(mut reader: Box<dyn Read + Send>, tx: mpsc::Sender<Vec<u8>>) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

/// Helper to implement alacritty_terminal's Dimensions trait.
struct PtyDimensions {
    cols: u16,
    rows: u16,
}

impl Dimensions for PtyDimensions {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }
    fn screen_lines(&self) -> usize {
        self.rows as usize
    }
    fn columns(&self) -> usize {
        self.cols as usize
    }
}

/// Drain PTY output from background thread, feed to alacritty_terminal.
fn poll_pty_output(
    mut q: Query<(Entity, &mut TerminalState, &PtyHandle, &ChildOf), (With<Terminal>, Without<PtyExited>)>,
    mut commands: Commands,
    mut writer: MessageWriter<AppCommand>,
) {
    for (entity, mut state, pty, child_of) in &mut q {
        let rx = pty.rx.lock().unwrap();
        let mut got_data = false;
        while let Ok(data) = rx.try_recv() {
            let TerminalState { ref mut term, ref mut processor, .. } = *state;
            processor.advance(term, &data);
            got_data = true;
        }
        if got_data {
            state.dirty = true;
        }

        // Check if the shell process has exited.
        if let Ok(mut child) = pty.child.lock() {
            if let Ok(Some(_status)) = child.try_wait() {
                commands.entity(entity).insert(PtyExited);
                // Activate the parent tab so TabCommand::Close targets it.
                let tab = child_of.get();
                commands.entity(tab).insert(LastActivatedAt::now());
                writer.write(AppCommand::Tab(TabCommand::Close));
            }
        }
    }
}

/// Serialize visible viewport and send to webview.
fn sync_terminal_viewport(
    mut q: Query<(Entity, &mut TerminalState), With<Terminal>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, mut state) in &mut q {
        if !state.dirty {
            continue;
        }
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        state.dirty = false;

        let viewport = build_viewport(&state.term);
        let body = ron::ser::to_string(&viewport).unwrap_or_default();
        commands.trigger(HostEmitEvent::new(entity, TERM_VIEWPORT_EVENT, &body));
    }
}

fn build_viewport<T: TermEventListener>(term: &Term<T>) -> TermViewportEvent {
    let grid = term.grid();
    let num_cols = grid.columns();
    let num_lines = grid.screen_lines();
    let offset = grid.display_offset() as i32;
    let mut lines = Vec::with_capacity(num_lines);

    for row_idx in 0..num_lines {
        let row = &grid[Line(row_idx as i32 - offset)];
        let mut spans = Vec::new();
        let mut text = String::new();
        let mut cur_fg: TermColor = TermColor::Default;
        let mut cur_bg: TermColor = TermColor::Default;
        let mut cur_flags: u16 = 0;

        for col_idx in 0..num_cols {
            let cell = &row[Column(col_idx)];

            // Skip wide char spacers
            if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
                continue;
            }

            let fg = color_to_term_color(&cell.fg);
            let bg = color_to_term_color(&cell.bg);
            let flags = cell_flags_to_u16(cell.flags);

            if fg != cur_fg || bg != cur_bg || flags != cur_flags {
                if !text.is_empty() {
                    spans.push(TermSpan {
                        text: std::mem::take(&mut text),
                        fg: cur_fg,
                        bg: cur_bg,
                        flags: cur_flags,
                    });
                }
                cur_fg = fg;
                cur_bg = bg;
                cur_flags = flags;
            }
            text.push(cell.c);
        }
        if !text.is_empty() {
            spans.push(TermSpan {
                text,
                fg: cur_fg,
                bg: cur_bg,
                flags: cur_flags,
            });
        }
        lines.push(TermLine { spans });
    }

    let cursor_point = grid.cursor.point;
    let scrolled_back = offset > 0;
    TermViewportEvent {
        lines,
        cursor: TermCursor {
            col: cursor_point.column.0 as u16,
            row: cursor_point.line.0 as u16,
            shape: CursorShape::Block,
            visible: !scrolled_back,
        },
        cols: num_cols as u16,
        rows: num_lines as u16,
        title: None,
    }
}

fn color_to_term_color(color: &Color) -> TermColor {
    match color {
        Color::Named(named) => match named {
            NamedColor::Foreground | NamedColor::DimForeground
            | NamedColor::BrightForeground => TermColor::Default,
            NamedColor::Background => TermColor::Default,
            NamedColor::Cursor => TermColor::Default,
            other => TermColor::Indexed(named_to_ansi_index(other)),
        },
        Color::Indexed(idx) if *idx < 16 => TermColor::Indexed(*idx),
        Color::Indexed(idx) => {
            let [r, g, b] = ansi_256_to_rgb(*idx);
            TermColor::Rgb(r, g, b)
        }
        Color::Spec(rgb) => TermColor::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

fn named_to_ansi_index(named: &NamedColor) -> u8 {
    match named {
        NamedColor::Black | NamedColor::DimBlack => 0,
        NamedColor::Red | NamedColor::DimRed => 1,
        NamedColor::Green | NamedColor::DimGreen => 2,
        NamedColor::Yellow | NamedColor::DimYellow => 3,
        NamedColor::Blue | NamedColor::DimBlue => 4,
        NamedColor::Magenta | NamedColor::DimMagenta => 5,
        NamedColor::Cyan | NamedColor::DimCyan => 6,
        NamedColor::White | NamedColor::DimWhite => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
        _ => 7, // fallback to white
    }
}

fn cell_flags_to_u16(flags: CellFlags) -> u16 {
    let mut f = 0u16;
    if flags.contains(CellFlags::BOLD) {
        f |= FLAG_BOLD;
    }
    if flags.contains(CellFlags::ITALIC) {
        f |= FLAG_ITALIC;
    }
    if flags.contains(CellFlags::UNDERLINE) {
        f |= FLAG_UNDERLINE;
    }
    if flags.contains(CellFlags::STRIKEOUT) {
        f |= FLAG_STRIKETHROUGH;
    }
    if flags.contains(CellFlags::DIM) {
        f |= FLAG_DIM;
    }
    if flags.contains(CellFlags::INVERSE) {
        f |= FLAG_INVERSE;
    }
    f
}

/// Convert ANSI 256-color index (16-255) to RGB.
fn ansi_256_to_rgb(idx: u8) -> [u8; 3] {
    if idx < 16 {
        return [0, 0, 0];
    }
    if idx < 232 {
        let i = idx - 16;
        let r = (i / 36) * 51;
        let g = ((i % 36) / 6) * 51;
        let b = (i % 6) * 51;
        [r, g, b]
    } else {
        let v = 8 + (idx - 232) * 10;
        [v, v, v]
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
    q: Query<&PtyHandle, (With<Terminal>, With<CefKeyboardTarget>)>,
    input: Res<ButtonInput<KeyCode>>,
) {
    if q.is_empty() {
        return;
    }
    let ctrl = input.pressed(KeyCode::ControlLeft) || input.pressed(KeyCode::ControlRight);
    let alt = input.pressed(KeyCode::AltLeft) || input.pressed(KeyCode::AltRight);

    let mut seen_keys: Vec<KeyCode> = Vec::new();
    for event in er.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        // Deduplicate non-character keys within the same batch — macOS/bevy_winit
        // can deliver two Pressed messages for a single physical press.
        if !event.repeat && is_non_character_key(event.key_code) {
            if seen_keys.contains(&event.key_code) {
                continue;
            }
            seen_keys.push(event.key_code);
        }
        let bytes = logical_key_to_bytes(&event.logical_key, ctrl, alt);
        if bytes.is_empty() {
            continue;
        }
        for pty in &q {
            if let Ok(mut writer) = pty.writer.lock() {
                let _ = writer.write_all(&bytes);
            }
        }
    }
}

fn logical_key_to_bytes(key: &Key, ctrl: bool, alt: bool) -> Vec<u8> {
    match key {
        Key::Character(s) => {
            if ctrl {
                if let Some(c) = s.chars().next() {
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

/// Handle mouse wheel scrolling — forwards to PTY when mouse mode is active,
/// otherwise scrolls the scrollback buffer.
fn handle_terminal_scroll(
    mut er: MessageReader<MouseWheel>,
    mut q: Query<(&mut TerminalState, &PtyHandle), (With<Terminal>, With<CefKeyboardTarget>)>,
) {
    if q.is_empty() {
        return;
    }
    for event in er.read() {
        let lines = match event.unit {
            MouseScrollUnit::Line => -event.y as i32,
            MouseScrollUnit::Pixel => (-event.y / 20.0) as i32,
        };
        if lines == 0 {
            continue;
        }
        for (mut state, pty) in &mut q {
            let mode = *state.term.mode();
            if mode.intersects(TermMode::MOUSE_MODE) && mode.contains(TermMode::SGR_MOUSE) {
                // Forward scroll as SGR mouse button 64 (up) / 65 (down) to the PTY
                let button: u8 = if lines < 0 { 64 } else { 65 };
                let count = lines.unsigned_abs();
                let seq = sgr_mouse_sequence(button, 0, 0, 0, true);
                if let Ok(mut w) = pty.writer.lock() {
                    for _ in 0..count {
                        let _ = w.write_all(&seq);
                    }
                }
            } else {
                state.term.scroll_display(Scroll::Delta(lines));
            }
            state.dirty = true;
        }
    }
}

/// Encode a mouse event as an SGR escape sequence.
/// button: SGR button value (0=left, 1=mid, 2=right, 32+=drag, 35=motion, 64/65=scroll)
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
    // SGR coordinates are 1-based
    format!("\x1b[<{};{};{}{}", cb, col + 1, row + 1, suffix).into_bytes()
}

/// Handle mouse events from the terminal webview.
fn on_term_mouse(
    trigger: On<Receive<TermMouseEvent>>,
    state_q: Query<&TerminalState, With<Terminal>>,
    pty_q: Query<&PtyHandle, With<Terminal>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;

    let Ok(state) = state_q.get(entity) else {
        return;
    };

    let mode = *state.term.mode();
    if !mode.intersects(TermMode::MOUSE_MODE) {
        return;
    }

    // Only support SGR encoding (used by all modern terminal apps)
    if !mode.contains(TermMode::SGR_MOUSE) {
        return;
    }

    // Filter motion events based on terminal mode
    if event.moving {
        let is_drag = event.button < 3;
        if is_drag && !mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) {
            return;
        }
        if !is_drag && !mode.contains(TermMode::MOUSE_MOTION) {
            return;
        }
    }

    // Encode SGR button: add 32 for motion/drag events
    let button = if event.moving {
        event.button + 32
    } else {
        event.button
    };

    let seq = sgr_mouse_sequence(button, event.col, event.row, event.modifiers, event.pressed);

    if let Ok(pty) = pty_q.get(entity) {
        if let Ok(mut w) = pty.writer.lock() {
            let _ = w.write_all(&seq);
        }
    }
}

/// Mark dirty when webview becomes ready so initial viewport is sent.
fn on_term_ready(trigger: On<Add, UiReady>, mut q: Query<&mut TerminalState, With<Terminal>>) {
    let entity = trigger.event_target();
    if let Ok(mut state) = q.get_mut(entity) {
        state.dirty = true;
    }
}

/// Handle resize event from webview (reports char cell dimensions).
fn on_term_resize(
    trigger: On<Receive<TermResizeEvent>>,
    webview_q: Query<&WebviewSize, With<Terminal>>,
    mut state_q: Query<&mut TerminalState, With<Terminal>>,
    pty_q: Query<&PtyHandle, With<Terminal>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;

    let Ok(webview_size) = webview_q.get(entity) else {
        return;
    };

    if event.char_width <= 0.0 || event.char_height <= 0.0 {
        return;
    }

    // Use viewport dimensions from JS when available (accounts for CEF zoom),
    // otherwise fall back to WebviewSize (DIP).
    let vw = if event.viewport_width > 0.0 { event.viewport_width } else { webview_size.0.x };
    let vh = if event.viewport_height > 0.0 { event.viewport_height } else { webview_size.0.y };

    let cols = (vw / event.char_width).floor().max(1.0) as u16;
    let rows = (vh / event.char_height).floor().max(1.0) as u16;

    // Resize PTY
    if let Ok(pty) = pty_q.get(entity) {
        let master = pty.master.lock().unwrap();
        let _ = master.resize(PtySize {
            rows,
            cols,
            pixel_width: webview_size.0.x as u16,
            pixel_height: webview_size.0.y as u16,
        });
    }

    // Resize alacritty terminal grid
    if let Ok(mut state) = state_q.get_mut(entity) {
        let dims = PtyDimensions { cols, rows };
        state.term.resize(dims);
        state.dirty = true;
    }
}

fn sync_terminal_theme(
    q: Query<Entity, With<Terminal>>,
    new_terminals: Query<Entity, Added<Terminal>>,
    browsers: NonSend<Browsers>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut last_theme_hash: Local<u64>,
) {
    let Some(terminal_settings) = &settings.terminal else {
        return;
    };

    let theme = terminal_settings.resolve_theme(&terminal_settings.default_theme);
    let colors = crate::themes::resolve_theme(&theme.color_scheme, &terminal_settings.custom_themes);

    // Simple hash to detect theme changes
    let hash = {
        let mut h: u64 = 0;
        for b in &colors.foreground { h = h.wrapping_mul(31).wrapping_add(*b as u64); }
        for b in &colors.background { h = h.wrapping_mul(31).wrapping_add(*b as u64); }
        for row in &colors.ansi { for b in row { h = h.wrapping_mul(31).wrapping_add(*b as u64); } }
        h
    };

    let theme_changed = hash != *last_theme_hash;
    if !theme_changed && new_terminals.is_empty() {
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
        new_terminals.iter().collect()
    };

    for entity in targets {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(HostEmitEvent::new(entity, TERM_THEME_EVENT, &body));
        }
    }
}

fn on_restart_pty(
    trigger: On<RestartPty>,
    mut q: Query<(&mut TerminalState, &mut PtyHandle, &mut PageMetadata)>,
    settings: Res<AppSettings>,
) {
    let entity = trigger.event().entity;
    let Ok((mut state, mut pty, mut meta)) = q.get_mut(entity) else {
        return;
    };

    // Kill old PTY
    let _ = pty.child.lock().unwrap().kill();

    let shell = settings
        .terminal
        .as_ref()
        .map(|t| t.resolve_theme(&t.default_theme).shell)
        .unwrap_or_else(default_shell);

    // Spawn new PTY
    let pty_system = NativePtySystem::default();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("failed to open PTY");

    let mut cmd = CommandBuilder::new(&shell);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    let child = pair.slave.spawn_command(cmd).expect("failed to spawn shell");
    let pid = child.process_id().unwrap_or(0);
    let reader = pair
        .master
        .try_clone_reader()
        .expect("failed to clone PTY reader");
    let new_writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(
        pair.master
            .take_writer()
            .expect("failed to take PTY writer"),
    ));
    drop(pair.slave);

    // Spawn new reader thread
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("pty-reader".into())
        .spawn(move || {
            pty_reader_thread(reader, tx);
        })
        .expect("failed to spawn PTY reader thread");

    // Reset terminal state
    let event_proxy = VmuxEventProxy {
        pty_writer: Arc::clone(&new_writer),
    };
    let term_config = TermConfig::default();
    let dims = PtyDimensions { cols: 80, rows: 24 };
    let new_term = Term::new(term_config, &dims, event_proxy);

    state.term = new_term;
    state.processor = Processor::new();
    state.dirty = true;

    // Replace PtyHandle entirely
    *pty = PtyHandle {
        rx: Mutex::new(rx),
        writer: new_writer,
        master: Mutex::new(pair.master),
        child: Mutex::new(child),
    };

    // Update metadata
    meta.url = format!("{}?session={}", TERMINAL_WEBVIEW_URL, pid);
    meta.title = format!("Terminal - {}", shell);
}
