use crate::{
    layout::window::WEBVIEW_MESH_DEPTH_BIAS,
    settings::AppSettings,
};
use alacritty_terminal::{
    event::{Event as TermEvent, EventListener as TermEventListener},
    grid::Dimensions,
    index::{Column, Line},
    term::{Config as TermConfig, Term, cell::Flags as CellFlags},
    vte::ansi::{Color, Processor},
};
use bevy::{picking::Pickable, prelude::*, render::alpha::AlphaMode};
use bevy_cef::prelude::*;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::{
    io::{Read, Write},
    sync::{mpsc, Mutex},
};
use vmux_header::PageMetadata;
use vmux_terminal::event::*;
use vmux_webview_app::UiReady;

/// Marker component for terminal content entities (analogous to Browser).
#[derive(Component)]
pub(crate) struct Terminal;

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
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    #[allow(dead_code)]
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
}

/// Event proxy required by alacritty_terminal. We ignore events for now.
#[derive(Clone)]
pub(crate) struct VmuxEventProxy;

impl TermEventListener for VmuxEventProxy {
    fn send_event(&self, _event: TermEvent) {}
}

pub(crate) struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsEmitEventPlugin::<TermKeyEvent>::default())
            .add_plugins(JsEmitEventPlugin::<TermResizeEvent>::default())
            .add_systems(Update, (poll_pty_output, sync_terminal_viewport).chain())
            .add_observer(on_term_key_input)
            .add_observer(on_term_ready)
            .add_observer(on_term_resize);
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
            .map(|t| t.shell.clone())
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

        let child = pair.slave.spawn_command(cmd).expect("failed to spawn shell");
        let reader = pair
            .master
            .try_clone_reader()
            .expect("failed to clone PTY reader");
        let writer = pair
            .master
            .take_writer()
            .expect("failed to take PTY writer");
        drop(pair.slave);

        // Spawn background reader thread
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || {
                pty_reader_thread(reader, tx);
            })
            .expect("failed to spawn PTY reader thread");

        // Create alacritty terminal
        let term_config = TermConfig::default();
        let dims = PtyDimensions { cols, rows };
        let term = Term::new(term_config, &dims, VmuxEventProxy);
        let processor = Processor::new();

        (
            (
                Self,
                TerminalState {
                    term,
                    processor,
                    dirty: true,
                },
                PtyHandle {
                    rx: Mutex::new(rx),
                    writer: Mutex::new(writer),
                    master: Mutex::new(pair.master),
                    child: Mutex::new(child),
                },
                PageMetadata {
                    title: format!("Terminal - {}", shell),
                    url: TERMINAL_WEBVIEW_URL.to_string(),
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
fn poll_pty_output(mut q: Query<(&mut TerminalState, &PtyHandle), With<Terminal>>) {
    for (mut state, pty) in &mut q {
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
    let mut lines = Vec::with_capacity(num_lines);

    for row_idx in 0..num_lines {
        let row = &grid[Line(row_idx as i32)];
        let mut spans = Vec::new();
        let mut text = String::new();
        let mut cur_fg: Option<[u8; 3]> = None;
        let mut cur_bg: Option<[u8; 3]> = None;
        let mut cur_flags: u16 = 0;

        for col_idx in 0..num_cols {
            let cell = &row[Column(col_idx)];

            // Skip wide char spacers
            if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
                continue;
            }

            let fg = color_to_rgb(&cell.fg);
            let bg = color_to_rgb(&cell.bg);
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
    TermViewportEvent {
        lines,
        cursor: TermCursor {
            col: cursor_point.column.0 as u16,
            row: cursor_point.line.0 as u16,
            shape: CursorShape::Block,
            visible: true,
        },
        cols: num_cols as u16,
        rows: num_lines as u16,
        title: None,
    }
}

fn color_to_rgb(color: &Color) -> Option<[u8; 3]> {
    match color {
        Color::Spec(rgb) => Some([rgb.r, rgb.g, rgb.b]),
        Color::Indexed(idx) => {
            if *idx < 16 {
                None // Use CSS theme defaults for basic 16 colors
            } else {
                Some(ansi_256_to_rgb(*idx))
            }
        }
        Color::Named(_) => None,
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

/// Handle keyboard input from webview.
fn on_term_key_input(
    trigger: On<Receive<TermKeyEvent>>,
    q: Query<&PtyHandle, With<Terminal>>,
) {
    let event = &trigger.payload;
    let entity = trigger.event_target();
    let Ok(pty) = q.get(entity) else {
        return;
    };

    let bytes = key_event_to_bytes(event);
    if !bytes.is_empty() {
        let mut writer = pty.writer.lock().unwrap();
        let _ = writer.write_all(&bytes);
    }
}

fn key_event_to_bytes(event: &TermKeyEvent) -> Vec<u8> {
    let ctrl = event.modifiers & MOD_CTRL != 0;
    let alt = event.modifiers & MOD_ALT != 0;

    // If there's a text character and no ctrl/alt, send it directly
    if let Some(ref text) = event.text {
        if !ctrl && !alt && !text.is_empty() {
            return text.as_bytes().to_vec();
        }
    }

    // Handle special keys
    let seq: Vec<u8> = match event.key.as_str() {
        "Enter" => b"\r".to_vec(),
        "Backspace" => {
            if ctrl {
                vec![0x08]
            } else {
                vec![0x7f]
            }
        }
        "Tab" => b"\t".to_vec(),
        "Escape" => vec![0x1b],
        "ArrowUp" => b"\x1b[A".to_vec(),
        "ArrowDown" => b"\x1b[B".to_vec(),
        "ArrowRight" => b"\x1b[C".to_vec(),
        "ArrowLeft" => b"\x1b[D".to_vec(),
        "Home" => b"\x1b[H".to_vec(),
        "End" => b"\x1b[F".to_vec(),
        "PageUp" => b"\x1b[5~".to_vec(),
        "PageDown" => b"\x1b[6~".to_vec(),
        "Delete" => b"\x1b[3~".to_vec(),
        "Insert" => b"\x1b[2~".to_vec(),
        _ => {
            if ctrl {
                if let Some(ref text) = event.text {
                    if let Some(c) = text.chars().next() {
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
            }
            if alt {
                if let Some(ref text) = event.text {
                    let mut v = vec![0x1b];
                    v.extend_from_slice(text.as_bytes());
                    return v;
                }
            }
            event
                .text
                .as_ref()
                .map(|t| t.as_bytes().to_vec())
                .unwrap_or_default()
        }
    };

    if alt && !seq.is_empty() && seq[0] != 0x1b {
        let mut v = vec![0x1b];
        v.extend_from_slice(&seq);
        v
    } else {
        seq
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

    let cols = (webview_size.0.x / event.char_width).floor().max(1.0) as u16;
    let rows = (webview_size.0.y / event.char_height).floor().max(1.0) as u16;

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
