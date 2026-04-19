# Command Palette Design

## Overview

A centered modal overlay rendered as a CEF webview (Dioxus WASM app). `Cmd+L` opens it with the current URL pre-selected. Supports URL navigation, command execution, and open tab search in a single unified input.

## UX

### Opening

- `Cmd+L` or `Cmd+K` opens the palette
- Input is pre-filled with the active tab's URL, text fully selected
- Below the input: a filtered list of results (tabs, commands)

### Input Behavior

| Input | Behavior |
|-------|----------|
| URL or domain | Navigate active tab on Enter |
| Search text (no scheme) | Match against open tab titles/URLs and command names |
| `>` prefix | Filter to commands only (VS Code convention) |
| Arrow keys | Navigate result list |
| Enter | Execute selected result |
| Esc | Dismiss palette, return focus to content |
| Click outside | Dismiss palette |

### Result Types

Results are displayed in a single flat list, ordered by relevance:

| Type | Display | Action on Enter |
|------|---------|-----------------|
| Open tab | Tab icon + title + URL (dimmed) | Switch to that tab/pane |
| Command | Command name + shortcut (right-aligned) | Execute the command |
| URL navigation | "Navigate to {url}" | Load URL in active tab |

## Architecture

### Data Flow

```
Cmd+L
  │
  ▼
keybinding.rs → AppCommand::Browser(FocusAddressBar)
  │
  ▼
command_palette.rs (new) :: handle_open_palette
  ├── Set Modal Display::Flex
  ├── Add CefKeyboardTarget to palette webview
  ├── Remove CefKeyboardTarget from content browser
  ├── HostEmitEvent "open_palette" with payload:
  │     { url, tabs: [{title, url, pane_id, tab_index, is_active}],
  │       commands: [{id, name, shortcut}] }
  │
  ▼
vmux_command_palette (Dioxus WASM)
  ├── Focus <input>, select all
  ├── Render filtered results as user types
  ├── On Enter/click: emit PaletteCommandEvent
  │     { action: "navigate"|"command"|"switch_tab", value: "..." }
  │
  ▼
command_palette.rs :: on_palette_command_emit
  ├── "navigate" → change active browser's WebviewSource
  ├── "command"  → write AppCommand to Messages
  ├── "switch_tab" → activate target pane + tab
  ├── Hide Modal (Display::None)
  ├── Remove CefKeyboardTarget from palette
  └── Restore CefKeyboardTarget to content browser
```

### Dismiss Flow

Esc key or click outside triggers a `PaletteCommandEvent` with action `"dismiss"`. Same cleanup: hide modal, restore keyboard target.

## New Crate: vmux_command_palette

Follows the pattern of `vmux_header` and `vmux_side_sheet`.

```
crates/vmux_command_palette/
├── Cargo.toml
├── Dioxus.toml
├── src/
│   ├── main.rs       # Dioxus app entry point
│   ├── app.rs        # Palette UI component
│   └── event.rs      # PaletteOpenEvent, PaletteCommandEvent
└── assets/
    └── tailwind.css
```

### Dependencies

Same as `vmux_header`: `dioxus`, `serde`, `ron`, shared JS bridge utilities.

### UI Component (app.rs)

Single component:
- `<input>` element, auto-focused, with the URL pre-filled and selected
- `<div>` list of results, filtered by input value
- Keyboard navigation: ArrowUp/ArrowDown move selection, Enter executes
- Styling: dark background overlay, centered card (max-width ~600px), Tailwind classes matching existing header/side sheet theme

### Events (event.rs)

```rust
// Bevy → Palette (open with context)
pub struct PaletteOpenEvent {
    pub url: String,
    pub tabs: Vec<PaletteTab>,
    pub commands: Vec<PaletteCommand>,
}

pub struct PaletteTab {
    pub title: String,
    pub url: String,
    pub pane_id: u64,
    pub tab_index: usize,
    pub is_active: bool,
}

pub struct PaletteCommand {
    pub id: String,        // e.g. "split_right", "reload"
    pub name: String,      // e.g. "Split Pane Right"
    pub shortcut: String,  // e.g. "⌘D"
}

// Palette → Bevy (user action)
pub struct PaletteCommandEvent {
    pub action: String,    // "navigate" | "command" | "switch_tab" | "dismiss"
    pub value: String,     // URL, command id, or "pane_id:tab_index"
}
```

## Bevy-Side: CommandPalettePlugin

New file: `crates/vmux_desktop/src/command_palette.rs`

### Components

```rust
#[derive(Component)]
pub struct CommandPalette;  // Marker on the modal entity
```

### Plugin Registration

```rust
pub struct CommandPalettePlugin;

impl Plugin for CommandPalettePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JsEmitEventPlugin::<PaletteCommandEvent>::default())
            .add_observer(on_palette_command_emit)
            .add_systems(Update, handle_open_palette.in_set(ReadAppCommands));
    }
}
```

### Modal Entity Setup

Reuse the existing `Modal` entity from window setup. Attach:
- `CommandPalette` marker
- `Browser::new()` with `WebviewSource` pointing to `vmux://command-palette/`
- `WebviewSize` appropriate for the overlay (e.g. 600x400)
- Start with `Display::None`

The palette webview is created once at startup and reused. Opening/closing toggles `Display` and sends `HostEmitEvent` to reset state.

### handle_open_palette System

Triggered by `AppCommand::Browser(BrowserCommand::FocusAddressBar)`:

1. Gather current URL from active tab's `PageMetadata`
2. Gather all open tabs across panes (same pattern as `push_pane_tree_emit`)
3. Build static command list from known `AppCommand` variants
4. Set modal `Display::Flex`
5. Add `CefKeyboardTarget` to palette browser entity
6. Remove `CefKeyboardTarget` from active content browser
7. Send `HostEmitEvent` with `PaletteOpenEvent` payload

### on_palette_command_emit Observer

Receives `PaletteCommandEvent` from JS:

| Action | Handler |
|--------|---------|
| `"navigate"` | Parse URL (add `https://` if no scheme), set active browser's `WebviewSource` |
| `"command"` | Match command id to `AppCommand`, write to `Messages<AppCommand>` |
| `"switch_tab"` | Parse `pane_id:tab_index`, activate target pane and tab |
| `"dismiss"` | No-op (cleanup below runs for all actions) |

After any action:
1. Set modal `Display::None`
2. Remove `CefKeyboardTarget` from palette browser
3. Restore `CefKeyboardTarget` to active content browser

## Keyboard Routing

The palette needs exclusive keyboard input while open. The mechanism:

- `CefKeyboardTarget` component controls which webview receives key events
- `sync_keyboard_target` in `browser.rs` runs every frame and assigns it to the active tab's browser
- When palette is open, `sync_keyboard_target` must skip reassignment

Add a guard: if `CommandPalette` entity has `Display::Flex` (palette is open), `sync_keyboard_target` returns early without changing keyboard targets. The palette's `CefKeyboardTarget` set by `handle_open_palette` remains until the palette closes.

## Command Registry

Command metadata is auto-generated by the `CommandPalette` derive macro in `vmux_macro`. Each command enum (`TabCommand`, `BrowserCommand`, `PaneCommand`, etc.) derives `CommandPalette`, which generates `palette_entries()` from existing `#[menu(id, label, accel)]` attributes. The `accel` format is converted to display symbols at compile time (e.g. `super+shift+r` becomes `⌘⇧R`).

`crates/vmux_desktop/src/palette.rs` provides the public API:

```rust
use crate::command::AppCommand;

pub struct PaletteEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub shortcut: &'static str,
}

/// All commands from all sub-enums, auto-generated from #[menu] attrs.
pub fn command_list() -> Vec<PaletteEntry> {
    AppCommand::palette_entries()
        .into_iter()
        .map(|(id, name, shortcut)| PaletteEntry { id, name, shortcut })
        .collect()
}

/// Map a menu id string back to an AppCommand variant.
pub fn match_command(id: &str) -> Option<AppCommand> {
    AppCommand::from_menu_id(id)
}
```

No hand-written command lists or match arms are needed. Adding a new command to any enum with `#[menu(...)]` automatically makes it available in the palette.

## Webview App Registration

Register the palette Dioxus app in `WebviewAppRegistry` (same pattern as header/side sheet). The `vmux://command-palette/` scheme serves the built WASM app.

In `crates/vmux_webview_app/`:
- Add `command-palette` to the embedded host list
- Build step (`build.rs` or `dx build`) compiles `vmux_command_palette` to WASM

## Files to Create

| File | Purpose |
|------|---------|
| `crates/vmux_command_palette/Cargo.toml` | Dioxus WASM crate |
| `crates/vmux_command_palette/Dioxus.toml` | Dioxus build config |
| `crates/vmux_command_palette/src/main.rs` | App entry |
| `crates/vmux_command_palette/src/app.rs` | Palette UI |
| `crates/vmux_command_palette/src/event.rs` | Event types |
| `crates/vmux_command_palette/assets/tailwind.css` | Styles |
| `crates/vmux_desktop/src/command_palette.rs` | Bevy plugin |

## Files to Modify

| File | Change |
|------|--------|
| `crates/vmux_desktop/src/lib.rs` | Register `CommandPalettePlugin` |
| `crates/vmux_desktop/src/browser.rs` | Guard `sync_keyboard_target` when palette is open |
| `crates/vmux_desktop/src/layout/window.rs` | Attach `CommandPalette` + `Browser` to Modal entity |
| `crates/vmux_webview_app/` | Register `command-palette` embedded host |
| `Cargo.toml` (workspace) | Add `vmux_command_palette` member |
