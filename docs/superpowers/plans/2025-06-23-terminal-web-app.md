# Terminal Web App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a terminal emulator tab type to vmux, running as a Dioxus WASM app in CEF with PTY management and VT parsing on the Bevy native side.

**Architecture:** PTY output is read by a Bevy system, parsed by `alacritty_terminal`, serialized as styled spans, and sent to a Dioxus webview app via `HostEmitEvent`. Keyboard/mouse input flows back via `JsEmitEvent` to the PTY. Terminal tabs live alongside browser tabs using the existing Tab/Pane hierarchy.

**Tech Stack:** alacritty_terminal 0.26.0, portable-pty 0.9.0, Dioxus (WASM), Bevy 0.18, CEF (bevy_cef)

**Spec:** `docs/superpowers/specs/2025-06-23-terminal-web-app-design.md`

---

### Task 1: Add Workspace Dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add alacritty_terminal and portable-pty to workspace dependencies**

In the `[workspace.dependencies]` section of the root `Cargo.toml`, add:

```toml
alacritty_terminal = "0.26"
portable-pty = "0.9"
vte = "0.15"
```

`vte` is needed for the ANSI processor that feeds bytes into `alacritty_terminal::Term`.

- [ ] **Step 2: Add vmux_terminal to workspace members**

In the `[workspace]` section, add `"crates/vmux_terminal"` to the `members` list.

- [ ] **Step 3: Verify workspace resolves**

Run: `cargo check --workspace 2>&1 | head -5`
Expected: May warn about missing vmux_terminal crate (not created yet). No errors from dependency resolution.

---

### Task 2: Create vmux_terminal Crate Scaffold

**Files:**
- Create: `crates/vmux_terminal/Cargo.toml`
- Create: `crates/vmux_terminal/Dioxus.toml`
- Create: `crates/vmux_terminal/src/main.rs`
- Create: `crates/vmux_terminal/src/app.rs`
- Create: `crates/vmux_terminal/src/event.rs`
- Create: `crates/vmux_terminal/src/lib.rs`

- [ ] **Step 1: Create Cargo.toml**

Model after `crates/vmux_header/Cargo.toml`. Dual-target crate (WASM for Dioxus app, native for Bevy plugin).

```toml
[package]
name = "vmux_terminal"
version = "0.1.0"
edition = "2021"

[lib]
name = "vmux_terminal"
path = "src/lib.rs"

[[bin]]
name = "vmux_terminal_app"
path = "src/main.rs"
required-features = ["web"]

[features]
default = []
web = ["dioxus", "wasm-bindgen"]

[dependencies]
serde = { version = "1", features = ["derive"] }

[target.'cfg(target_arch = "wasm32")'.dependencies]
dioxus = { workspace = true }
wasm-bindgen = { workspace = true }
vmux_ui = { path = "../vmux_ui", default-features = false }

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
bevy = { workspace = true }
bevy_cef = { workspace = true }
vmux_webview_app = { path = "../vmux_webview_app" }

[build-dependencies]
vmux_webview_app = { path = "../vmux_webview_app", features = ["build"] }
```

- [ ] **Step 2: Create Dioxus.toml**

```toml
[application]
name = "vmux_terminal"
default_platform = "web"

[web.app]
title = "vmux terminal"
```

- [ ] **Step 3: Create event.rs (shared types)**

These types are used by both the WASM app and Bevy-side systems.

```rust
use serde::{Deserialize, Serialize};

pub const TERM_VIEWPORT_EVENT: &str = "term_viewport";
pub const TERM_KEY_EVENT: &str = "term_key";
pub const TERM_MOUSE_EVENT: &str = "term_mouse";

pub const TERMINAL_WEBVIEW_URL: &str = "vmux://terminal/";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermViewportEvent {
    pub lines: Vec<TermLine>,
    pub cursor: TermCursor,
    pub cols: u16,
    pub rows: u16,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermLine {
    pub spans: Vec<TermSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermSpan {
    pub text: String,
    pub fg: Option<[u8; 3]>,
    pub bg: Option<[u8; 3]>,
    pub flags: u16,
}

/// Bitflags for TermSpan.flags
pub const FLAG_BOLD: u16 = 1;
pub const FLAG_ITALIC: u16 = 2;
pub const FLAG_UNDERLINE: u16 = 4;
pub const FLAG_STRIKETHROUGH: u16 = 8;
pub const FLAG_DIM: u16 = 16;
pub const FLAG_INVERSE: u16 = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermCursor {
    pub col: u16,
    pub row: u16,
    pub shape: CursorShape,
    pub visible: bool,
}

impl Default for TermCursor {
    fn default() -> Self {
        Self { col: 0, row: 0, shape: CursorShape::Block, visible: true }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Beam,
    Underline,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermKeyEvent {
    pub key: String,
    pub modifiers: u8,
    pub text: Option<String>,
}

pub const MOD_CTRL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;
pub const MOD_SUPER: u8 = 8;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermMouseEvent {
    pub button: u8,
    pub col: u16,
    pub row: u16,
    pub modifiers: u8,
    pub pressed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermResizeEvent {
    pub char_width: f32,
    pub char_height: f32,
}

pub const TERM_RESIZE_EVENT: &str = "term_resize";
```

- [ ] **Step 4: Create main.rs (WASM entry)**

```rust
#![allow(non_snake_case)]
mod app;
pub mod event;

fn main() {
    dioxus::launch(app::App);
}
```

- [ ] **Step 5: Create app.rs (minimal placeholder)**

```rust
#![allow(non_snake_case)]
use dioxus::prelude::*;
use vmux_ui::hooks::use_theme;

#[component]
pub fn App() -> Element {
    use_theme();

    rsx! {
        div {
            class: "h-full w-full bg-background text-foreground font-mono p-2",
            "Terminal loading..."
        }
    }
}
```

- [ ] **Step 6: Create lib.rs (Bevy plugin)**

```rust
pub mod event;

#[cfg(not(target_arch = "wasm32"))]
mod plugin;

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::TerminalWebviewPlugin;
```

- [ ] **Step 7: Create plugin.rs (native-side plugin)**

Create `crates/vmux_terminal/src/plugin.rs`:

```rust
use bevy::prelude::*;
use std::path::PathBuf;
use vmux_webview_app::{WebviewAppConfig, WebviewAppRegistry};

pub struct TerminalWebviewPlugin;

impl Plugin for TerminalWebviewPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<WebviewAppRegistry>()
            .register(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                &WebviewAppConfig::with_custom_host("terminal"),
            );
    }
}
```

- [ ] **Step 8: Create build.rs**

Create `crates/vmux_terminal/build.rs`, following the same pattern as `crates/vmux_header/build.rs`:

```rust
fn main() {
    vmux_webview_app::WebviewAppBuilder::new(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        "vmux_terminal",
        "vmux_terminal_app",
    )
    .dx_extra_args(&["--bin", "vmux_terminal_app", "--features", "web"])
    .run("terminal");
}
```

Note: Check `crates/vmux_header/build.rs` for any additional builder methods used (e.g., `cef_finalize`, `tailwind_postprocess_after_dx`, tracked paths). Copy those patterns.

- [ ] **Step 9: Commit scaffold**

```bash
git add crates/vmux_terminal/
git commit -m "feat: scaffold vmux_terminal crate"
```

---

### Task 3: Terminal Component and PTY Management

**Files:**
- Create: `crates/vmux_desktop/src/terminal.rs`
- Modify: `crates/vmux_desktop/Cargo.toml`
- Modify: `crates/vmux_desktop/src/lib.rs`

- [ ] **Step 1: Add dependencies to vmux_desktop**

In `crates/vmux_desktop/Cargo.toml`, add to `[dependencies]`:

```toml
vmux_terminal = { path = "../vmux_terminal" }
alacritty_terminal = { workspace = true }
portable-pty = { workspace = true }
vte = { workspace = true }
```

- [ ] **Step 2: Create terminal.rs**

```rust
use crate::{
    browser::PageMetadata,
    layout::tab::tab_bundle,
    settings::AppSettings,
};
use alacritty_terminal::{
    event::{Event as TermEvent, EventListener as TermEventListener},
    grid::Dimensions,
    term::{Config as TermConfig, Term, cell::Flags as CellFlags},
    vte::ansi::Processor,
};
use bevy::{prelude::*, render::alpha::AlphaMode};
use bevy_cef::prelude::*;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use vmux_history::LastActivatedAt;
use vmux_terminal::event::*;

/// Marker component for terminal tab content entities (analogous to Browser).
#[derive(Component)]
pub struct Terminal;

/// Holds the alacritty_terminal state for a terminal instance.
#[derive(Component)]
pub struct TerminalState {
    pub term: Term<VmuxEventProxy>,
    pub processor: Processor,
    pub dirty: bool,
}

/// Holds the PTY master fd and child process.
#[derive(Component)]
pub struct PtyHandle {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub child: Box<dyn portable_pty::Child + Send>,
}

/// Event proxy required by alacritty_terminal. We collect events but don't
/// need to forward them to a channel — we read Term state directly.
#[derive(Clone)]
pub struct VmuxEventProxy;

impl TermEventListener for VmuxEventProxy {
    fn send_event(&self, _event: TermEvent) {
        // Events like title change, bell, etc. — handle later if needed.
    }
}

pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsEmitEventPlugin::<TermKeyEvent>::new(TERM_KEY_EVENT))
            .add_plugins(JsEmitEventPlugin::<TermMouseEvent>::new(TERM_MOUSE_EVENT))
            .add_plugins(JsEmitEventPlugin::<TermResizeEvent>::new(TERM_RESIZE_EVENT))
            .add_systems(Update, (poll_pty_output, sync_terminal_viewport).chain())
            .add_observer(on_term_key_input)
            .add_observer(on_term_ready);
    }
}

impl Terminal {
    pub fn new(
        meshes: &mut ResMut<Assets<Mesh>>,
        webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
        settings: &AppSettings,
    ) -> impl Bundle {
        let cols = 80u16;
        let rows = 24u16;

        // Create PTY
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(&PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("failed to open PTY");

        let shell = settings.terminal.shell.clone();
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave.spawn_command(cmd).expect("failed to spawn shell");
        let reader = pair.master.try_clone_reader().expect("failed to clone PTY reader");
        let writer = pair.master.take_writer().expect("failed to take PTY writer");

        // Drop slave — the child process owns it now
        drop(pair.slave);

        // Create alacritty_terminal
        let size = alacritty_terminal::grid::Dimensions::default(); // We'll set proper size
        let term_config = TermConfig::default();
        let term = Term::new(term_config, &PtyDimensions { cols, rows }, VmuxEventProxy);
        let processor = Processor::new();

        (
            Terminal,
            TerminalState {
                term,
                processor,
                dirty: true,
            },
            PtyHandle {
                reader,
                writer,
                child,
            },
            PageMetadata {
                title: format!("Terminal - {}", shell),
                ..default()
            },
            WebviewSource::new(TERMINAL_WEBVIEW_URL),
            Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(Vec3::Z, Vec2::splat(0.5)))),
            MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial {
                base: StandardMaterial {
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    depth_bias: crate::layout::window::WEBVIEW_MESH_DEPTH_BIAS,
                    ..default()
                },
                ..default()
            })),
            WebviewSize(Vec2::new(800.0, 600.0)),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                ..default()
            },
            WebviewTransparent,
            Transform::default(),
            GlobalTransform::default(),
        )
    }
}

/// Helper to implement alacritty_terminal's Dimensions trait.
struct PtyDimensions {
    cols: u16,
    rows: u16,
}

impl Dimensions for PtyDimensions {
    fn total_lines(&self) -> usize { self.rows as usize }
    fn screen_lines(&self) -> usize { self.rows as usize }
    fn columns(&self) -> usize { self.cols as usize }
    fn last_column(&self) -> alacritty_terminal::index::Column {
        alacritty_terminal::index::Column(self.cols.saturating_sub(1) as usize)
    }
    fn bottommost_line(&self) -> alacritty_terminal::index::Line {
        alacritty_terminal::index::Line(self.rows as i32 - 1)
    }
    fn topmost_line(&self) -> alacritty_terminal::index::Line {
        alacritty_terminal::index::Line(0)
    }
}

/// Non-blocking read from PTY, feed bytes to alacritty_terminal.
fn poll_pty_output(mut q: Query<(&mut TerminalState, &mut PtyHandle), With<Terminal>>) {
    for (mut state, mut pty) in &mut q {
        let mut buf = [0u8; 4096];
        // Non-blocking read — PTY reader should be set non-blocking
        match pty.reader.read(&mut buf) {
            Ok(0) => {} // EOF
            Ok(n) => {
                state.processor.advance(&mut state.term, &buf[..n]);
                state.dirty = true;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => {}
        }
    }
}

/// Serialize visible viewport and send to webview.
fn sync_terminal_viewport(
    mut q: Query<(Entity, &mut TerminalState), With<Terminal>>,
    mut commands: Commands,
) {
    for (entity, mut state) in &mut q {
        if !state.dirty {
            continue;
        }
        state.dirty = false;

        let term = &state.term;
        let grid = term.grid();
        let mut lines = Vec::new();

        for row_idx in 0..grid.screen_lines() {
            let row = &grid[alacritty_terminal::index::Line(row_idx as i32)];
            let mut spans = Vec::new();
            let mut current_text = String::new();
            let mut current_fg: Option<[u8; 3]> = None;
            let mut current_bg: Option<[u8; 3]> = None;
            let mut current_flags: u16 = 0;

            for col_idx in 0..grid.columns() {
                let cell = &row[alacritty_terminal::index::Column(col_idx)];
                let fg = color_to_rgb(&cell.fg);
                let bg = color_to_rgb(&cell.bg);
                let flags = cell_flags_to_u16(cell.flags);

                if fg != current_fg || bg != current_bg || flags != current_flags {
                    if !current_text.is_empty() {
                        spans.push(TermSpan {
                            text: current_text.clone(),
                            fg: current_fg,
                            bg: current_bg,
                            flags: current_flags,
                        });
                        current_text.clear();
                    }
                    current_fg = fg;
                    current_bg = bg;
                    current_flags = flags;
                }
                current_text.push(cell.c);
            }
            if !current_text.is_empty() {
                spans.push(TermSpan {
                    text: current_text,
                    fg: current_fg,
                    bg: current_bg,
                    flags: current_flags,
                });
            }
            lines.push(TermLine { spans });
        }

        let cursor = term.grid().cursor.point;
        let viewport = TermViewportEvent {
            lines,
            cursor: TermCursor {
                col: cursor.column.0 as u16,
                row: cursor.line.0 as u16,
                shape: CursorShape::Block,
                visible: true,
            },
            cols: grid.columns() as u16,
            rows: grid.screen_lines() as u16,
            title: None,
        };

        commands.trigger_targets(
            HostEmitEvent::new(TERM_VIEWPORT_EVENT, &ron::to_string(&viewport).unwrap()),
            entity,
        );
    }
}

fn color_to_rgb(color: &alacritty_terminal::vte::ansi::Color) -> Option<[u8; 3]> {
    use alacritty_terminal::vte::ansi::Color;
    match color {
        Color::Spec(rgb) => Some([rgb.r, rgb.g, rgb.b]),
        Color::Indexed(idx) => {
            // Basic 16 colors — return None to use CSS theme defaults
            if *idx < 16 { None } else { Some(ansi_256_to_rgb(*idx)) }
        }
        _ => None,
    }
}

fn cell_flags_to_u16(flags: CellFlags) -> u16 {
    let mut f = 0u16;
    if flags.contains(CellFlags::BOLD) { f |= FLAG_BOLD; }
    if flags.contains(CellFlags::ITALIC) { f |= FLAG_ITALIC; }
    if flags.contains(CellFlags::UNDERLINE) { f |= FLAG_UNDERLINE; }
    if flags.contains(CellFlags::STRIKEOUT) { f |= FLAG_STRIKETHROUGH; }
    if flags.contains(CellFlags::DIM) { f |= FLAG_DIM; }
    if flags.contains(CellFlags::INVERSE) { f |= FLAG_INVERSE; }
    f
}

/// Convert ANSI 256-color index (16-255) to RGB.
fn ansi_256_to_rgb(idx: u8) -> [u8; 3] {
    if idx < 16 { return [0, 0, 0]; }
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
    trigger: Trigger<Receive<TermKeyEvent>>,
    mut q: Query<&mut PtyHandle, With<Terminal>>,
) {
    let event = &trigger.event().message;
    let entity = trigger.target();
    let Ok(mut pty) = q.get_mut(entity) else { return };

    // Convert key event to bytes for the PTY
    let bytes = key_event_to_bytes(event);
    if !bytes.is_empty() {
        let _ = pty.writer.write_all(&bytes);
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
    let seq = match event.key.as_str() {
        "Enter" => b"\r".to_vec(),
        "Backspace" => if ctrl { vec![0x08] } else { vec![0x7f] },
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
            // Ctrl+letter
            if ctrl {
                if let Some(ref text) = event.text {
                    if let Some(c) = text.chars().next() {
                        let code = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a').wrapping_add(1);
                        if code <= 26 {
                            let mut v = Vec::new();
                            if alt { v.push(0x1b); }
                            v.push(code);
                            return v;
                        }
                    }
                }
            }
            // Alt+char
            if alt {
                if let Some(ref text) = event.text {
                    let mut v = vec![0x1b];
                    v.extend_from_slice(text.as_bytes());
                    return v;
                }
            }
            // Fallback: send text if available
            if let Some(ref text) = event.text {
                text.as_bytes().to_vec()
            } else {
                Vec::new()
            }
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

/// Send initial viewport when webview is ready.
fn on_term_ready(
    trigger: Trigger<OnAdd, UiReady>,
    q: Query<&TerminalState, With<Terminal>>,
    mut commands: Commands,
) {
    let entity = trigger.target();
    if q.get(entity).is_ok() {
        // Mark dirty to trigger viewport sync on next frame
        // (The UiReady component was just added, viewport will sync in Update)
    }
}
```

**Note:** The exact `alacritty_terminal` API may differ from what's shown above. The implementor MUST check `docs.rs/alacritty_terminal/0.26.0` for the actual types and method signatures (e.g., `Term::new`, `Grid` access, `Cell` fields, `Color` enum variants). The structure and data flow are correct; the specific API calls may need adjustment.

- [ ] **Step 3: Register TerminalPlugin in lib.rs**

In `crates/vmux_desktop/src/lib.rs`, add:

```rust
mod terminal;
```

And in the `VmuxPlugin::build()` method, add:

```rust
.add_plugins(terminal::TerminalPlugin)
.add_plugins(vmux_terminal::TerminalWebviewPlugin)
```

Place the `TerminalWebviewPlugin` BEFORE `BrowserPlugin` (since BrowserPlugin reads the registry).

- [ ] **Step 4: Verify it compiles**

Run: `cargo check --package vmux_desktop 2>&1 | tail -10`

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: add Terminal component, PTY management, and viewport serialization"
```

---

### Task 4: Add TabCommand::NewTerminal

**Files:**
- Modify: `crates/vmux_desktop/src/command.rs`
- Modify: `crates/vmux_desktop/src/layout/tab.rs`

- [ ] **Step 1: Add NewTerminal variant to TabCommand**

In `command.rs`, add to the `TabCommand` enum:

```rust
#[menu(name = "New Terminal")]
#[bind(chord = "Ctrl+b, t")]
NewTerminal,
```

- [ ] **Step 2: Handle NewTerminal in tab.rs**

In the `TabCommand::New` handler section of `tab.rs`, add a parallel handler for `TabCommand::NewTerminal`:

```rust
AppCommand::Tab(TabCommand::NewTerminal) => {
    let Some(pane) = focused_pane() else { continue };
    let tab = commands.spawn((
        tab_bundle(),
        LastActivatedAt::now(),
        ChildOf(pane),
    )).id();
    commands.spawn((
        Terminal::new(&mut meshes, &mut webview_mt, &settings),
        ChildOf(tab),
    ));
}
```

Add the necessary imports: `use crate::terminal::Terminal;`

- [ ] **Step 3: Verify**

Run: `cargo check --package vmux_desktop 2>&1 | tail -5`

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: add TabCommand::NewTerminal with Ctrl+B,t binding"
```

---

### Task 5: Generalize Browser Systems for Terminal

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`

- [ ] **Step 1: Widen sync_children_to_ui query**

Find the query that filters `With<Browser>` in `sync_children_to_ui`. Change it to:

```rust
Or<(With<Browser>, With<Terminal>)>
```

This allows the system to position terminal webview meshes the same way as browser meshes.

- [ ] **Step 2: Widen sync_keyboard_target query**

Same pattern — the keyboard target system should also consider `Terminal` entities.

- [ ] **Step 3: Update push_tabs_host_emit**

The system that builds the tab list for the header needs to include terminal tabs. Terminal tabs have `PageMetadata` (set in `Terminal::new()`), so they should work with the existing `PageMetadata` query. Verify that the query doesn't filter on `With<Browser>` exclusively.

- [ ] **Step 4: Verify and commit**

```bash
cargo check --package vmux_desktop 2>&1 | tail -5
git add -A
git commit -m "feat: generalize browser systems to handle Terminal entities"
```

---

### Task 6: Terminal Viewport Rendering (Dioxus App)

**Files:**
- Modify: `crates/vmux_terminal/src/app.rs`

- [ ] **Step 1: Implement full terminal renderer**

```rust
#![allow(non_snake_case)]
use dioxus::prelude::*;
use crate::event::*;
use vmux_ui::hooks::{use_event_listener, use_theme, try_cef_emit_serde};

#[component]
pub fn App() -> Element {
    use_theme();
    let mut viewport = use_signal(TermViewportEvent::default);

    let _listener = use_event_listener::<TermViewportEvent, _>(
        TERM_VIEWPORT_EVENT,
        move |data| { viewport.set(data); },
    );

    let vp = viewport();

    // Install keydown/keyup handler via raw JS for reliable capture
    use_effect(|| {
        document::eval(r#"
            setTimeout(() => {
                var el = document.getElementById('term-input');
                if (!el) return;
                el.focus();
                if (el._bound) return;
                el._bound = true;
                el.addEventListener('keydown', function(e) {
                    e.preventDefault();
                    e.stopPropagation();
                    var mods = 0;
                    if (e.ctrlKey) mods |= 1;
                    if (e.altKey) mods |= 2;
                    if (e.shiftKey) mods |= 4;
                    if (e.metaKey) mods |= 8;
                    var payload = {key: e.code, modifiers: mods, text: e.key.length === 1 ? e.key : null};
                    window.__cef_emit('term_key', payload);
                }, true);
            }, 100);
        "#);
    });

    rsx! {
        div {
            class: "relative h-full w-full overflow-hidden bg-background font-mono text-sm leading-tight",
            onclick: move |_| {
                document::eval("document.getElementById('term-input')?.focus()");
            },
            // Hidden textarea for input capture
            textarea {
                id: "term-input",
                class: "absolute opacity-0 w-0 h-0",
                autofocus: true,
            }
            // Terminal viewport
            div { class: "p-1",
                for (row_idx, line) in vp.lines.iter().enumerate() {
                    div {
                        key: "{row_idx}",
                        class: "flex whitespace-pre",
                        style: "height: 1.2em;",
                        for (span_idx, span) in line.spans.iter().enumerate() {
                            span {
                                key: "{span_idx}",
                                style: span_style(span),
                                "{span.text}"
                            }
                        }
                        // Cursor overlay
                        if row_idx == vp.cursor.row as usize && vp.cursor.visible {
                            span {
                                class: "absolute",
                                style: format!(
                                    "left: calc(0.25rem + {}ch); background: var(--foreground); color: var(--background); animation: blink 1s step-end infinite;",
                                    vp.cursor.col
                                ),
                                {cursor_char(&vp, row_idx)}
                            }
                        }
                    }
                }
            }
        }
    }
}

fn span_style(span: &TermSpan) -> String {
    let mut parts = Vec::new();
    if let Some([r, g, b]) = span.fg {
        parts.push(format!("color:rgb({r},{g},{b})"));
    }
    if let Some([r, g, b]) = span.bg {
        parts.push(format!("background:rgb({r},{g},{b})"));
    }
    if span.flags & FLAG_BOLD != 0 { parts.push("font-weight:bold".into()); }
    if span.flags & FLAG_ITALIC != 0 { parts.push("font-style:italic".into()); }
    if span.flags & FLAG_UNDERLINE != 0 { parts.push("text-decoration:underline".into()); }
    if span.flags & FLAG_STRIKETHROUGH != 0 { parts.push("text-decoration:line-through".into()); }
    if span.flags & FLAG_DIM != 0 { parts.push("opacity:0.5".into()); }
    parts.join(";")
}

fn cursor_char(vp: &TermViewportEvent, row: usize) -> String {
    // Get the character under the cursor
    if let Some(line) = vp.lines.get(row) {
        let col = vp.cursor.col as usize;
        let mut pos = 0;
        for span in &line.spans {
            for c in span.text.chars() {
                if pos == col { return c.to_string(); }
                pos += 1;
            }
        }
    }
    " ".to_string()
}
```

- [ ] **Step 2: Add CSS for cursor blink**

Create `crates/vmux_terminal/assets/index.css`:

```css
@keyframes blink {
    0%, 100% { opacity: 1; }
    50% { opacity: 0; }
}
```

- [ ] **Step 3: Verify WASM build**

Run: `cargo check --package vmux_terminal --target wasm32-unknown-unknown 2>&1 | tail -5`

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: implement terminal viewport rendering in Dioxus app"
```

---

### Task 7: Settings — Terminal Configuration

**Files:**
- Modify: `crates/vmux_desktop/src/settings.rs`

- [ ] **Step 1: Add TerminalSettings struct**

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TerminalSettings {
    pub shell: String,
    pub scrollback: usize,
    pub font_family: String,
    pub font_size: f32,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string()),
            scrollback: 10000,
            font_family: "monospace".to_string(),
            font_size: 14.0,
        }
    }
}
```

- [ ] **Step 2: Add to AppSettings**

```rust
pub struct AppSettings {
    // ... existing fields ...
    #[serde(default)]
    pub terminal: TerminalSettings,
}
```

- [ ] **Step 3: Verify and commit**

```bash
cargo check --package vmux_desktop 2>&1 | tail -5
git add -A
git commit -m "feat: add terminal settings (shell, scrollback, font)"
```

---

### Task 8: Non-blocking PTY Reader

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Set PTY reader to non-blocking**

The `portable-pty` reader is blocking by default. Use a background thread to read and channel the data:

```rust
use std::sync::mpsc;

#[derive(Component)]
pub struct PtyHandle {
    pub rx: mpsc::Receiver<Vec<u8>>,
    pub writer: Box<dyn Write + Send>,
    pub child: Box<dyn portable_pty::Child + Send>,
}
```

In `Terminal::new()`, spawn a reader thread:

```rust
let (tx, rx) = mpsc::channel::<Vec<u8>>();
std::thread::spawn(move || {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if tx.send(buf[..n].to_vec()).is_err() { break; }
            }
            Err(_) => break,
        }
    }
});
```

Update `poll_pty_output` to drain the channel:

```rust
fn poll_pty_output(mut q: Query<(&mut TerminalState, &PtyHandle), With<Terminal>>) {
    for (mut state, pty) in &mut q {
        while let Ok(data) = pty.rx.try_recv() {
            state.processor.advance(&mut state.term, &data);
            state.dirty = true;
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add -A
git commit -m "feat: use background thread for non-blocking PTY reads"
```

---

### Task 9: Configurable Leader Key

**Files:**
- Modify: `crates/vmux_desktop/src/command.rs`
- Modify: `crates/vmux_desktop/src/settings.rs`
- Modify: `crates/vmux_desktop/src/keybinding.rs`

- [ ] **Step 1: Change default leader from Ctrl+B to Ctrl+V**

In `command.rs`, replace ALL occurrences of `Ctrl+b` in `#[bind(chord = "...")]` attributes with `Ctrl+v`:

```
Ctrl+b, h  →  Ctrl+v, h
Ctrl+b, l  →  Ctrl+v, l
Ctrl+b, k  →  Ctrl+v, k
Ctrl+b, j  →  Ctrl+v, j
...etc for all chord bindings
```

- [ ] **Step 2: Add leader key to settings**

In `settings.rs`, add to `KeyBindingSettings`:

```rust
#[serde(default = "default_leader")]
pub leader: String,

fn default_leader() -> String { "Ctrl+v".to_string() }
```

- [ ] **Step 3: Apply configured leader at runtime**

In `keybinding.rs`, when building `KeyBindingMap` from `AppCommand::default_key_bindings()`, replace the chord prefix with the configured leader from settings. This allows users to override the leader key in `settings.ron`.

- [ ] **Step 4: Verify and commit**

```bash
cargo check --package vmux_desktop 2>&1 | tail -5
git add -A
git commit -m "feat: change leader key to Ctrl+V, make configurable via settings"
```

---

### Task 10: SGR Mouse Reporting

**Files:**
- Modify: `crates/vmux_terminal/src/app.rs`
- Modify: `crates/vmux_desktop/src/terminal.rs`

- [ ] **Step 1: Add mouse event handlers to Dioxus app**

Add `onmousedown`, `onmouseup`, `onmousemove` handlers to the terminal viewport div. Compute cell coordinates from pixel position using character width/height. Emit `TermMouseEvent` via `window.__cef_emit`.

- [ ] **Step 2: Add mouse event observer in terminal.rs**

```rust
fn on_term_mouse_input(
    trigger: Trigger<Receive<TermMouseEvent>>,
    mut q: Query<(&TerminalState, &mut PtyHandle), With<Terminal>>,
) {
    let event = &trigger.event().message;
    let entity = trigger.target();
    let Ok((state, mut pty)) = q.get_mut(entity) else { return };

    // Only send mouse events if terminal has mouse reporting enabled
    // Check alacritty_terminal's mode flags for mouse reporting
    let press_char = if event.pressed { 'M' } else { 'm' };
    let sgr = format!("\x1b[<{};{};{}{}", event.button, event.col + 1, event.row + 1, press_char);
    let _ = pty.writer.write_all(sgr.as_bytes());
}
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat: add SGR-style mouse reporting for terminal"
```

---

### Task 11: Terminal Resize Handling

**Files:**
- Modify: `crates/vmux_desktop/src/terminal.rs`
- Modify: `crates/vmux_terminal/src/app.rs`

- [ ] **Step 1: Add resize observer in Dioxus app**

Use a `ResizeObserver` (via JS) on the terminal container to measure available pixels. Compute character width/height using a measurement element. Emit `TermResizeEvent` with the font metrics.

- [ ] **Step 2: Handle resize on Bevy side**

```rust
fn on_term_resize(
    trigger: Trigger<Receive<TermResizeEvent>>,
    mut q: Query<(&mut TerminalState, &PtyHandle), With<Terminal>>,
) {
    let event = &trigger.event().message;
    let entity = trigger.target();
    let Ok((mut state, pty)) = q.get_mut(entity) else { return };

    // Compute new cols/rows from ComputedNode size and font metrics
    // Update Term::resize() and PTY winsize
}
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat: handle terminal resize based on pane dimensions"
```

---

### Task 12: Integration Testing

- [ ] **Step 1: Full build and run**

Run: `make run-mac`

Test:
1. Launch vmux
2. Press `Ctrl+V, t` to open a new terminal tab
3. Verify shell prompt appears
4. Type commands, verify output renders with colors
5. Verify cursor position and blinking
6. Test Ctrl+C, Ctrl+D, Ctrl+Z
7. Switch between terminal and browser tabs
8. Resize the pane, verify terminal reflows

- [ ] **Step 2: Commit any fixes**

```bash
git add -A
git commit -m "fix: integration test fixes for terminal web app"
```

---

## File Map Summary

| Status | File | Purpose |
|--------|------|---------|
| CREATE | `crates/vmux_terminal/Cargo.toml` | Crate manifest |
| CREATE | `crates/vmux_terminal/Dioxus.toml` | Dioxus config |
| CREATE | `crates/vmux_terminal/build.rs` | Webview app build |
| CREATE | `crates/vmux_terminal/src/main.rs` | WASM entry |
| CREATE | `crates/vmux_terminal/src/app.rs` | Terminal UI renderer |
| CREATE | `crates/vmux_terminal/src/event.rs` | Shared event types |
| CREATE | `crates/vmux_terminal/src/lib.rs` | Bevy plugin re-export |
| CREATE | `crates/vmux_terminal/src/plugin.rs` | Webview app registration |
| CREATE | `crates/vmux_terminal/assets/index.css` | Cursor animation CSS |
| CREATE | `crates/vmux_desktop/src/terminal.rs` | Terminal component + PTY + systems |
| MODIFY | `Cargo.toml` | Workspace deps |
| MODIFY | `crates/vmux_desktop/Cargo.toml` | Add terminal deps |
| MODIFY | `crates/vmux_desktop/src/lib.rs` | Register plugins |
| MODIFY | `crates/vmux_desktop/src/command.rs` | NewTerminal + leader key |
| MODIFY | `crates/vmux_desktop/src/layout/tab.rs` | Terminal tab creation |
| MODIFY | `crates/vmux_desktop/src/browser.rs` | Generalize for Terminal |
| MODIFY | `crates/vmux_desktop/src/settings.rs` | Terminal settings |
| MODIFY | `crates/vmux_desktop/src/keybinding.rs` | Configurable leader |
