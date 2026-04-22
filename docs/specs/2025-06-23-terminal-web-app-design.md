# Terminal Web App Design

## Overview

A terminal emulator integrated into vmux as a tab type alongside browser tabs. The terminal runs as a Dioxus WASM app inside a CEF webview, with PTY management and VT parsing on the Bevy native side using `alacritty_terminal`.

## Requirements

- TrueColor (24-bit RGB) support
- Kitty keyboard protocol (enhanced key event reporting for TUI apps)
- SGR-style mouse reporting
- Full viewport sync rendering (Bevy serializes terminal state, webview renders)
- Terminal tabs live alongside browser tabs in any pane
- Configurable leader key via `settings.ron`

## Architecture

### Data Flow

```
Output (PTY to screen):
  Shell → PTY fd → alacritty_terminal::Term::process_bytes()
    → serialize viewport → HostEmitEvent(RON) → Dioxus WASM app renders DOM

Input (keyboard to PTY):
  CEF keydown → Dioxus onkeydown → JsEmitEvent<TermKeyEvent>
    → Bevy observer → encode bytes → PTY write

Mouse (SGR reporting):
  CEF mouse event → Dioxus handler → JsEmitEvent<TermMouseEvent>
    → Bevy observer → SGR encode (CSI < Pb;Px;Py M/m) → PTY write
```

### Crate Structure

```
NEW  crates/vmux_terminal/          -- Dioxus WASM app (renders terminal viewport)
       src/main.rs                  -- entry point
       src/app.rs                   -- App component, viewport rendering, input capture
       src/event.rs                 -- TermViewportEvent, TermKeyEvent, TermMouseEvent
       Dioxus.toml, Cargo.toml

MOD  crates/vmux_desktop/
       NEW  src/terminal.rs         -- Terminal component, PTY management, Bevy systems
       MOD  src/command.rs          -- TabCommand::NewTerminal
       MOD  src/layout/tab.rs       -- handle terminal tab creation
       MOD  src/browser.rs          -- generalize sync_children_to_ui for Terminal entities
       MOD  src/settings.rs         -- terminal config + keybinding config
       MOD  src/input.rs            -- configurable leader key

DEP  alacritty_terminal             -- VT parsing, terminal grid, Kitty keyboard protocol
DEP  portable-pty                   -- cross-platform PTY spawning (macOS/Linux)
```

### Entity Hierarchy

```
Tab + tab_bundle()
└── Terminal::new()
    Components:
      - Terminal           (marker, analogous to Browser)
      - TerminalState      (holds alacritty_terminal::Term, NonSend)
      - PtyHandle          (owns PTY fd + child process handle)
      - WebviewSource      ("vmux://terminal/")
      - Mesh3d             (plane, same as Browser)
      - MeshMaterial3d<WebviewExtendStandardMaterial>
      - WebviewSize
      - PageMetadata       (title from OSC or shell name)
      - Node, Transform, GlobalTransform
```

## Serialization Format

### TermViewportEvent (Bevy → Webview, via HostEmitEvent)

Sent on each PTY read batch. Contains the full visible viewport.

```rust
struct TermViewportEvent {
    lines: Vec<TermLine>,
    cursor: TermCursor,
    size: (u16, u16),           // (cols, rows)
    title: Option<String>,      // OSC title set by shell
}

struct TermLine {
    spans: Vec<TermSpan>,
}

struct TermSpan {
    text: String,
    fg: Option<[u8; 3]>,       // TrueColor RGB, None = default
    bg: Option<[u8; 3]>,       // TrueColor RGB, None = default
    flags: u16,                // bitfield: bold|italic|underline|strikethrough|dim|reverse
}

struct TermCursor {
    col: u16,
    row: u16,
    shape: CursorShape,        // Block | Beam | Underline
    visible: bool,
}
```

### TermKeyEvent (Webview → Bevy, via JsEmitEvent)

```rust
struct TermKeyEvent {
    key: String,               // DOM key code (e.g., "KeyA", "Enter", "ArrowUp")
    modifiers: u8,             // bitfield: ctrl|alt|shift|super
    text: Option<String>,      // character produced, if any
    kitty_flags: u8,           // Kitty protocol: press|repeat|release
}
```

### TermMouseEvent (Webview → Bevy, via JsEmitEvent)

```rust
struct TermMouseEvent {
    button: u8,                // 0=left, 1=middle, 2=right, 64=scroll_up, 65=scroll_down
    col: u16,
    row: u16,
    modifiers: u8,             // bitfield: ctrl|alt|shift
    pressed: bool,             // true = press (M), false = release (m)
}
```

## Bevy Systems (terminal.rs)

### Plugin Registration

```rust
pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsEmitEventPlugin::<TermKeyEvent>::default())
           .add_plugins(JsEmitEventPlugin::<TermMouseEvent>::default())
           .add_systems(Update, (poll_pty_output, sync_terminal_viewport))
           .add_systems(PostUpdate, handle_terminal_resize)
           .add_observer(on_term_key_input)
           .add_observer(on_term_mouse_input)
           .add_observer(on_term_ready);
    }
}
```

### System Descriptions

| System | Schedule | Description |
|--------|----------|-------------|
| `poll_pty_output` | Update | Non-blocking read from PTY fd. Feed bytes to `alacritty_terminal::Term::process_bytes()`. |
| `sync_terminal_viewport` | Update | For each Terminal with dirty state, serialize the visible viewport as `TermViewportEvent` and send via `HostEmitEvent`. |
| `handle_terminal_resize` | PostUpdate | When pane's `ComputedNode` size changes, compute cols/rows from font metrics, call `TIOCSWINSZ` on PTY + `Term::resize()`. |
| `on_term_key_input` | Observer | Receives `TermKeyEvent` from webview. Encodes key to byte sequence (respecting Kitty keyboard mode if active). Writes to PTY. |
| `on_term_mouse_input` | Observer | Receives `TermMouseEvent`. If terminal has mouse reporting enabled, encodes as SGR sequence and writes to PTY. |
| `on_term_ready` | Observer | On `Add<UiReady>` for Terminal entities, send initial viewport + theme event. |

### PTY Management

- Use `portable-pty` crate for cross-platform PTY creation
- Spawn shell from `settings.terminal.shell` (default: `$SHELL` env var or `/bin/zsh`)
- Set `TERM=xterm-256color` (or `xterm-kitty` when Kitty keyboard is active)
- PTY fd is stored in `PtyHandle` component (non-Send, owns the child process)
- On tab close, send SIGHUP to child process and close PTY fd
- Scrollback buffer size from `settings.terminal.scrollback` (default: 10000 lines)

## Webview App (vmux_terminal)

### Rendering

The Dioxus app receives `TermViewportEvent` and renders it as DOM elements:

- Each `TermLine` → `<div class="term-line">` with `display: flex`
- Each `TermSpan` → `<span>` with inline CSS for fg/bg colors and text decorations
- TrueColor: `style="color: rgb(r,g,b); background: rgb(r,g,b)"`
- Text flags: bold → `font-weight: bold`, italic → `font-style: italic`, etc.
- Cursor: rendered as a positioned overlay element at `(cursor.col, cursor.row)` with CSS animation for blinking
- Font: monospace, size from settings (sent via ThemeEvent)

### Input Capture

- The app has a hidden `<textarea>` that stays focused to capture all keyboard input
- `onkeydown` handler captures key events and emits `TermKeyEvent` via `window.__cef_emit()`
- For Kitty keyboard protocol: send `key`, `modifiers`, and event type (press/repeat/release)
- `onkeyup` also captured for Kitty release events
- Mouse events (`onmousedown`, `onmouseup`, `onmousemove`) on the terminal viewport compute cell coordinates from pixel position using font metrics and emit `TermMouseEvent`
- Scroll events (`onwheel`) emit scroll-up/scroll-down mouse events or adjust viewport offset

### URL

Registered as `vmux://terminal/` in the CEF scheme handler, built and embedded the same way as `vmux://header/`, `vmux://command-bar/`, etc.

## Tab Integration

### TabCommand::NewTerminal

New command variant that creates a terminal tab:

```rust
TabCommand::NewTerminal
```

Bound to a configurable shortcut (e.g., leader + `t`).

### Creation Flow

```
TabCommand::NewTerminal received
  → find active pane (same logic as TabCommand::New)
  → spawn Tab + tab_bundle() + ChildOf(pane)
  → spawn Terminal::new() + TerminalState + PtyHandle + ChildOf(tab)
  → PTY spawns shell process
  → CEF creates webview for vmux://terminal/
  → UiReady triggers initial viewport sync
```

### Browser System Generalization

Systems in `browser.rs` that filter `With<Browser>` need to also handle `With<Terminal>`:

- `sync_children_to_ui` — position/scale 3D mesh to match UI node (add `Or<(With<Browser>, With<Terminal>)>`)
- `sync_cef_webview_resize_after_ui` — resize CEF viewport (same generalization)
- `push_tabs_host_emit` — read `PageMetadata` from terminal tabs too (title = shell name or OSC title)
- `sync_keyboard_target` — route keyboard focus to terminal webview when its tab is active

## Settings

### New Fields in settings.ron

```ron
(
    keybinding: (
        leader: "<leader>",
    ),
    terminal: (
        shell: "/opt/homebrew/bin/nu",
        scrollback: 10000,
        font_family: "Berkeley Mono",
        font_size: 14.0,
    ),
)
```

### Leader Key Configuration

The leader key (currently hardcoded as `<leader>` in `input.rs`) becomes configurable:

- Parse `settings.keybinding.leader` string into a key combination
- Update chord detection logic to use the configured leader
- Default: `<leader>`
- Terminal tabs forward all non-leader key events to the PTY

## Feature Details

### TrueColor

- `alacritty_terminal` parses SGR sequences `38;2;r;g;b` (foreground) and `48;2;r;g;b` (background)
- Colors serialized as `Option<[u8; 3]>` in `TermSpan`
- Webview renders as CSS `color: rgb(r,g,b)` / `background-color: rgb(r,g,b)`
- Named ANSI colors (0-15) mapped to theme palette colors via CSS variables
- 256-color palette (16-255) converted to RGB on the Bevy side

### Kitty Keyboard Protocol

- `alacritty_terminal` tracks the keyboard mode flag set by the application (CSI > flags u)
- When Kitty mode is active, `TermKeyEvent` includes:
  - Full key code (not just character)
  - All modifier keys
  - Event type: press, repeat, release
- Bevy encodes these as Kitty CSI u sequences when writing to PTY
- Graceful degradation: when Kitty mode is not active, encode as standard xterm sequences

### SGR Mouse Reporting

- `alacritty_terminal` tracks mouse reporting mode (CSI ? 1006 h = SGR mode)
- When enabled, webview mouse events are forwarded as `TermMouseEvent`
- Bevy encodes as SGR format: `CSI < button ; col ; row M` (press) or `m` (release)
- Supports: click, drag, scroll, modifier keys
- When mouse reporting is disabled, mouse events are handled locally (text selection)

### Resize

- Pane resize detected in `handle_terminal_resize` (PostUpdate, after UI layout)
- Available pixel size from `ComputedNode`
- Compute cols/rows: `cols = width_px / char_width`, `rows = height_px / line_height`
- Font metrics sent from webview to Bevy (or computed from settings)
- Update PTY window size via `TIOCSWINSZ` ioctl
- Update `alacritty_terminal::Term` size via `Term::resize()`
- Triggers re-render of viewport

## Out of Scope (Future Work)

- Kitty graphics protocol (inline images)
- Shell integration / OSC 133 block extraction (Warp-like structured blocks)
- Terminal multiplexing within vmux (handled by pane splits instead)
- Sixel graphics
- Ligature rendering
