# Commands and Keybindings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add all command variants from the spec to enum definitions with OS menu accelerators and keybindings, implement tab cycling/selection, and decouple pane cycling from tab commands.

**Architecture:** All command enums live in `command.rs`. Each enum uses `#[menu(...)]` for OS menu items and `#[bind(...)]` for custom keybindings. Handlers in their respective modules match new variants as no-op stubs unless the spec says "new" or "fix". The `on_pane_cycle` system in `pane.rs` stops intercepting `TabCommand::Next/Previous`; tab cycling moves to `handle_tab_commands` in `tab.rs`.

**Tech Stack:** Bevy 0.18, vmux_macro (`OsSubMenu`, `DefaultKeyBindings` derives), muda (OS menus)

**Important casing rules:**
- `accel` values use lowercase: `super+t`, `super+shift+]` (parsed by muda)
- `#[bind]` values use capitalized modifiers: `Ctrl+b`, `Shift+Tab`, `Alt+Left` (parsed by `parse_key_combo_tokens` in vmux_macro)

**Verify command:** `cargo check -p vmux_desktop out+err>| tail -20`

**Spec:** `docs/superpowers/specs/2025-07-13-commands-and-keybindings.md`

---

### Task 1: Add all command enum variants with accels and bindings

**Files:**
- Modify: `crates/vmux_desktop/src/command.rs`

This task replaces all enum definitions in command.rs. The file will not compile until Task 2 fixes the handler match arms.

- [ ] **Step 1: Replace the entire command.rs file**

Replace the full content of `crates/vmux_desktop/src/command.rs` with:

```rust
use bevy::prelude::*;
use vmux_macro::{DefaultKeyBindings, OsMenu, OsSubMenu};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WriteAppCommands;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ReadAppCommands;

pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AppCommand>()
            .configure_sets(Update, ReadAppCommands.after(WriteAppCommands));
    }
}

#[derive(Message, OsMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    #[menu(label = "Space")]
    Space(SpaceCommand),

    #[menu(label = "Pane")]
    Pane(PaneCommand),

    #[menu(label = "Tab")]
    Tab(TabCommand),

    #[menu(label = "Side Sheet")]
    SideSheet(SideSheetCommand),

    #[menu(label = "Camera")]
    Camera(CameraCommand),

    #[menu(label = "Browser")]
    Browser(BrowserCommand),

    #[menu(label = "Window")]
    Window(WindowCommand),
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabCommand {
    #[default]
    #[menu(id = "tab_new", label = "New Tab", accel = "super+t")]
    New,
    #[menu(id = "tab_close", label = "Close Tab", accel = "super+w")]
    Close,
    #[menu(id = "tab_next", label = "Next Tab", accel = "super+shift+]")]
    Next,
    #[menu(id = "tab_previous", label = "Previous Tab", accel = "super+shift+[")]
    Previous,
    #[menu(id = "tab_select_1", label = "Select Tab 1", accel = "super+1")]
    SelectIndex1,
    #[menu(id = "tab_select_2", label = "Select Tab 2", accel = "super+2")]
    SelectIndex2,
    #[menu(id = "tab_select_3", label = "Select Tab 3", accel = "super+3")]
    SelectIndex3,
    #[menu(id = "tab_select_4", label = "Select Tab 4", accel = "super+4")]
    SelectIndex4,
    #[menu(id = "tab_select_5", label = "Select Tab 5", accel = "super+5")]
    SelectIndex5,
    #[menu(id = "tab_select_6", label = "Select Tab 6", accel = "super+6")]
    SelectIndex6,
    #[menu(id = "tab_select_7", label = "Select Tab 7", accel = "super+7")]
    SelectIndex7,
    #[menu(id = "tab_select_8", label = "Select Tab 8", accel = "super+8")]
    SelectIndex8,
    #[menu(id = "tab_select_last", label = "Select Last Tab", accel = "super+9")]
    SelectLast,
    #[menu(id = "tab_reopen", label = "Reopen Closed Tab", accel = "super+shift+t")]
    Reopen,
    #[menu(id = "tab_duplicate", label = "Duplicate Tab")]
    Duplicate,
    #[menu(id = "tab_pin", label = "Pin Tab")]
    Pin,
    #[menu(id = "tab_mute", label = "Mute Tab")]
    Mute,
    #[menu(id = "tab_move_to_pane", label = "Move Tab to Pane")]
    MoveToPane,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserCommand {
    #[default]
    #[menu(id = "browser_prev_page", label = "Back", accel = "super+[")]
    PrevPage,
    #[menu(id = "browser_next_page", label = "Forward", accel = "super+]")]
    NextPage,
    #[menu(id = "browser_reload", label = "Reload", accel = "super+r")]
    Reload,
    #[menu(id = "browser_hard_reload", label = "Hard Reload", accel = "super+shift+r")]
    HardReload,
    #[menu(id = "browser_stop", label = "Stop Loading")]
    Stop,
    #[menu(id = "browser_focus_address_bar", label = "Open Location", accel = "super+l")]
    FocusAddressBar,
    #[menu(id = "browser_find", label = "Find", accel = "super+f")]
    Find,
    #[menu(id = "browser_zoom_in", label = "Zoom In", accel = "super+=")]
    ZoomIn,
    #[menu(id = "browser_zoom_out", label = "Zoom Out", accel = "super+-")]
    ZoomOut,
    #[menu(id = "browser_zoom_reset", label = "Actual Size", accel = "super+0")]
    ZoomReset,
    #[menu(id = "browser_dev_tools", label = "Developer Tools", accel = "super+alt+i")]
    DevTools,
    #[menu(id = "browser_view_source", label = "View Source", accel = "super+alt+u")]
    ViewSource,
    #[menu(id = "browser_print", label = "Print", accel = "super+p")]
    Print,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaneCommand {
    #[default]
    #[menu(id = "split_v", label = "Split Vertically\tCtrl+B, V")]
    #[bind(chord = "Ctrl+b, v")]
    SplitV,
    #[menu(id = "split_h", label = "Split Horizontally\tCtrl+B, H")]
    #[bind(chord = "Ctrl+b, h")]
    SplitH,
    #[menu(id = "toggle_pane", label = "Toggle Pane\tCtrl+B, T")]
    #[bind(chord = "Ctrl+b, t")]
    Toggle,
    #[menu(id = "close_pane", label = "Close Pane\tCtrl+B, X")]
    #[bind(chord = "Ctrl+b, x")]
    Close,
    #[menu(id = "zoom_pane", label = "Zoom Pane\tCtrl+B, Z")]
    #[bind(chord = "Ctrl+b, z")]
    Zoom,
    #[menu(id = "select_pane_left", label = "Select Left Pane\tCtrl+B, Left")]
    #[bind(chord = "Ctrl+b, Left")]
    SelectLeft,
    #[menu(id = "select_pane_right", label = "Select Right Pane\tCtrl+B, Right")]
    #[bind(chord = "Ctrl+b, Right")]
    SelectRight,
    #[menu(id = "select_pane_up", label = "Select Up Pane\tCtrl+B, Up")]
    #[bind(chord = "Ctrl+b, Up")]
    SelectUp,
    #[menu(id = "select_pane_down", label = "Select Down Pane\tCtrl+B, Down")]
    #[bind(chord = "Ctrl+b, Down")]
    SelectDown,
    #[menu(id = "swap_pane_prev", label = "Swap Pane Previous\tCtrl+B, {")]
    #[bind(chord = "Ctrl+b, {")]
    SwapPrev,
    #[menu(id = "swap_pane_next", label = "Swap Pane Next\tCtrl+B, }")]
    #[bind(chord = "Ctrl+b, }")]
    SwapNext,
    #[menu(id = "rotate_forward", label = "Rotate Forward\tCtrl+B, Ctrl+O")]
    #[bind(chord = "Ctrl+b, Ctrl+o")]
    RotateForward,
    #[menu(id = "rotate_backward", label = "Rotate Backward\tCtrl+B, Alt+O")]
    #[bind(chord = "Ctrl+b, Alt+o")]
    RotateBackward,
    #[menu(id = "equalize_pane_size", label = "Equalize Pane Size\tCtrl+B, =")]
    #[bind(chord = "Ctrl+b, =")]
    EqualizeSize,
    #[menu(id = "resize_pane_left", label = "Resize Pane Left\tCtrl+B, Alt+Left")]
    #[bind(chord = "Ctrl+b, Alt+Left")]
    ResizeLeft,
    #[menu(id = "resize_pane_right", label = "Resize Pane Right\tCtrl+B, Alt+Right")]
    #[bind(chord = "Ctrl+b, Alt+Right")]
    ResizeRight,
    #[menu(id = "resize_pane_up", label = "Resize Pane Up\tCtrl+B, Alt+Up")]
    #[bind(chord = "Ctrl+b, Alt+Up")]
    ResizeUp,
    #[menu(id = "resize_pane_down", label = "Resize Pane Down\tCtrl+B, Alt+Down")]
    #[bind(chord = "Ctrl+b, Alt+Down")]
    ResizeDown,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpaceCommand {
    #[default]
    #[menu(id = "new_space", label = "New Space\tCtrl+B, C")]
    #[bind(chord = "Ctrl+b, c")]
    New,
    #[menu(id = "close_space", label = "Close Space\tCtrl+B, &")]
    #[bind(chord = "Ctrl+b, &")]
    Close,
    #[menu(id = "next_space", label = "Next Space", accel = "ctrl+tab")]
    Next,
    #[menu(id = "prev_space", label = "Previous Space", accel = "ctrl+shift+tab")]
    Previous,
    #[menu(id = "rename_space", label = "Rename Space")]
    Rename,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SideSheetCommand {
    #[default]
    #[menu(id = "toggle_side_sheet", label = "Toggle Side Sheet\tCtrl+B, S")]
    #[bind(chord = "Ctrl+b, s")]
    Toggle,
    #[menu(id = "toggle_side_sheet_right", label = "Toggle Right Sheet")]
    ToggleRight,
    #[menu(id = "toggle_side_sheet_bottom", label = "Toggle Bottom Sheet")]
    ToggleBottom,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraCommand {
    #[default]
    #[menu(id = "reset_camera", label = "Reset Camera")]
    Reset,
    #[menu(id = "toggle_free_camera", label = "Toggle Free Camera")]
    ToggleFreeCamera,
}

#[derive(OsSubMenu, DefaultKeyBindings, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowCommand {
    #[default]
    #[menu(id = "new_window", label = "New Window", accel = "super+n")]
    NewWindow,
    #[menu(id = "close_window", label = "Close Window", accel = "super+shift+w")]
    CloseWindow,
    #[menu(id = "minimize_window", label = "Minimize", accel = "super+m")]
    Minimize,
    #[menu(id = "toggle_fullscreen", label = "Toggle Fullscreen", accel = "ctrl+super+f")]
    ToggleFullscreen,
    #[menu(id = "open_settings", label = "Settings", accel = "super+,")]
    Settings,
}
```

- [ ] **Step 2: Note — do NOT run cargo check yet**

The code will not compile because handler match arms in tab.rs, pane.rs, browser.rs, side_sheet.rs, and space.rs are not exhaustive. Proceed to Task 2.

---

### Task 2: Fix all handler match arms for new variants

**Files:**
- Modify: `crates/vmux_desktop/src/layout/tab.rs:120-121` (handle_tab_commands match)
- Modify: `crates/vmux_desktop/src/layout/pane.rs:174-184` (handle_pane_commands match)
- Modify: `crates/vmux_desktop/src/layout/space.rs:42-48` (handle_space_commands match)
- Modify: `crates/vmux_desktop/src/layout/side_sheet.rs:37-42` (handle_side_sheet_toggle)
- Modify: `crates/vmux_desktop/src/browser.rs:528-530` (handle_browser_commands match)

- [ ] **Step 1: Update handle_tab_commands match in tab.rs**

In `handle_tab_commands`, replace the final match arm:

```rust
            TabCommand::Next | TabCommand::Previous => {}
```

with:

```rust
            TabCommand::Next | TabCommand::Previous => {}
            TabCommand::SelectIndex1
            | TabCommand::SelectIndex2
            | TabCommand::SelectIndex3
            | TabCommand::SelectIndex4
            | TabCommand::SelectIndex5
            | TabCommand::SelectIndex6
            | TabCommand::SelectIndex7
            | TabCommand::SelectIndex8
            | TabCommand::SelectLast => {}
            TabCommand::Reopen
            | TabCommand::Duplicate
            | TabCommand::Pin
            | TabCommand::Mute
            | TabCommand::MoveToPane => {}
```

- [ ] **Step 2: Update handle_pane_commands match in pane.rs**

In `handle_pane_commands`, after the existing empty arms, replace:

```rust
            PaneCommand::RotateForward => {}
            PaneCommand::RotateBackward => {}
```

with:

```rust
            PaneCommand::RotateForward => {}
            PaneCommand::RotateBackward => {}
            PaneCommand::EqualizeSize => {}
            PaneCommand::ResizeLeft => {}
            PaneCommand::ResizeRight => {}
            PaneCommand::ResizeUp => {}
            PaneCommand::ResizeDown => {}
```

- [ ] **Step 3: Update handle_space_commands match in space.rs**

In `handle_space_commands`, after `SpaceCommand::Previous => {}`, add:

```rust
            SpaceCommand::Rename => {}
```

So the full match becomes:

```rust
        match space_cmd {
            SpaceCommand::New => {}
            SpaceCommand::Close => {}
            SpaceCommand::Next => {}
            SpaceCommand::Previous => {}
            SpaceCommand::Rename => {}
        }
```

- [ ] **Step 4: Update handle_side_sheet_toggle in side_sheet.rs**

Replace the for-loop body in `handle_side_sheet_toggle`. Current:

```rust
    for cmd in reader.read() {
        if matches!(cmd, AppCommand::SideSheet(SideSheetCommand::Toggle)) {
            open.0 = !open.0;
        }
    }
```

New:

```rust
    for cmd in reader.read() {
        match cmd {
            AppCommand::SideSheet(SideSheetCommand::Toggle) => {
                open.0 = !open.0;
            }
            AppCommand::SideSheet(SideSheetCommand::ToggleRight) => {}
            AppCommand::SideSheet(SideSheetCommand::ToggleBottom) => {}
            _ => {}
        }
    }
```

- [ ] **Step 5: Update handle_browser_commands match in browser.rs**

In `handle_browser_commands`, after the existing three arms, add the new stub arms. Replace:

```rust
        match browser_cmd {
            BrowserCommand::PrevPage => commands.trigger(RequestGoBack { webview }),
            BrowserCommand::NextPage => commands.trigger(RequestGoForward { webview }),
            BrowserCommand::Reload => commands.trigger(RequestReload { webview }),
        }
```

with:

```rust
        match browser_cmd {
            BrowserCommand::PrevPage => commands.trigger(RequestGoBack { webview }),
            BrowserCommand::NextPage => commands.trigger(RequestGoForward { webview }),
            BrowserCommand::Reload => commands.trigger(RequestReload { webview }),
            BrowserCommand::HardReload => {}
            BrowserCommand::Stop => {}
            BrowserCommand::FocusAddressBar => {}
            BrowserCommand::Find => {}
            BrowserCommand::ZoomIn => {}
            BrowserCommand::ZoomOut => {}
            BrowserCommand::ZoomReset => {}
            BrowserCommand::DevTools => {}
            BrowserCommand::ViewSource => {}
            BrowserCommand::Print => {}
        }
```

- [ ] **Step 6: Run cargo check**

Run: `cargo check -p vmux_desktop out+err>| tail -30`

Expected: PASS. If there are errors about non-exhaustive match on `AppCommand` (due to new `Window` variant), find each system that directly matches `AppCommand` variants and add `AppCommand::Window(_) => continue,` or ensure the `else { continue }` pattern handles it.

Note: All existing handlers use `let AppCommand::X(x_cmd) = *cmd else { continue; }` which naturally ignores unmatched variants — so `AppCommand::Window(...)` should be fine.

- [ ] **Step 7: Commit**

```bash
git commit -m "Add all command variants with accels and bindings"
```

---

### Task 3: Decouple pane cycling from TabCommand

**Files:**
- Modify: `crates/vmux_desktop/src/layout/pane.rs:1-8` (imports)
- Modify: `crates/vmux_desktop/src/layout/pane.rs:222-246` (on_pane_cycle)

Currently `on_pane_cycle` intercepts `TabCommand::Next/Previous` to cycle between panes. This must change: tab cycling goes to tab.rs, pane cycling uses `PaneCommand` variants.

- [ ] **Step 1: Update imports in pane.rs**

Replace:

```rust
use crate::{
    browser::browser_bundle,
    command::{AppCommand, PaneCommand, ReadAppCommands, TabCommand},
    layout::space::Space,
    layout::tab::{Active, Tab, tab_bundle},
    settings::AppSettings,
};
```

with:

```rust
use crate::{
    browser::browser_bundle,
    command::{AppCommand, PaneCommand, ReadAppCommands},
    layout::space::Space,
    layout::tab::{Active, Tab, tab_bundle},
    settings::AppSettings,
};
```

- [ ] **Step 2: Update on_pane_cycle match to use PaneCommand**

In `on_pane_cycle`, replace:

```rust
        let delta: i32 = match cmd {
            AppCommand::Tab(TabCommand::Next) => 1,
            AppCommand::Tab(TabCommand::Previous) => -1,
            _ => continue,
        };
```

with:

```rust
        let delta: i32 = match cmd {
            AppCommand::Pane(PaneCommand::SelectRight) => 1,
            AppCommand::Pane(PaneCommand::SelectLeft) => -1,
            _ => continue,
        };
```

- [ ] **Step 3: Remove the now-duplicate stub arms from handle_pane_commands**

In `handle_pane_commands`, the `SelectLeft` and `SelectRight` arms are now handled by `on_pane_cycle`. Remove them from the match:

```rust
            PaneCommand::SelectLeft => {}
            PaneCommand::SelectRight => {}
```

Both systems read from the same MessageReader independently, so having both match is fine. But since `on_pane_cycle` now handles the actual logic, the empty stubs in `handle_pane_commands` are redundant noise. Remove them.

Wait — actually, removing them would make the match non-exhaustive. Instead, keep them as-is. Both systems independently read messages; the empty stub in `handle_pane_commands` is harmless.

Leave `handle_pane_commands` unchanged for `SelectLeft`/`SelectRight`.

- [ ] **Step 4: Run cargo check**

Run: `cargo check -p vmux_desktop out+err>| tail -20`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git commit -m "Decouple pane cycling from TabCommand"
```

---

### Task 4: Implement tab cycling (Next/Previous) in tab.rs

**Files:**
- Modify: `crates/vmux_desktop/src/layout/tab.rs:120-121` (handle_tab_commands match)

- [ ] **Step 1: Replace the Next/Previous stub with cycling logic**

In `handle_tab_commands`, replace:

```rust
            TabCommand::Next | TabCommand::Previous => {}
```

with:

```rust
            TabCommand::Next | TabCommand::Previous => {
                let Ok(pane) = active_pane.single() else {
                    continue;
                };
                let Ok(children) = pane_children.get(pane) else {
                    continue;
                };
                let tabs: Vec<Entity> = children
                    .iter()
                    .filter(|&e| tab_q.contains(e))
                    .collect();
                if tabs.len() < 2 {
                    continue;
                }
                let Some(current) = tabs.iter().position(|&e| active_tabs.contains(e)) else {
                    continue;
                };
                let delta: i32 = if tab_cmd == TabCommand::Next { 1 } else { -1 };
                let n = tabs.len() as i32;
                let idx = (current as i32 + delta).rem_euclid(n) as usize;
                commands.entity(tabs[current]).remove::<Active>();
                commands.entity(tabs[idx]).insert(Active);
            }
```

- [ ] **Step 2: Run cargo check**

Run: `cargo check -p vmux_desktop out+err>| tail -20`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git commit -m "Implement tab cycling within active pane"
```

---

### Task 5: Implement tab index selection (SelectIndex1..8 / SelectLast)

**Files:**
- Modify: `crates/vmux_desktop/src/layout/tab.rs` (handle_tab_commands match)

- [ ] **Step 1: Replace the SelectIndex/SelectLast stub with implementation**

In `handle_tab_commands`, replace:

```rust
            TabCommand::SelectIndex1
            | TabCommand::SelectIndex2
            | TabCommand::SelectIndex3
            | TabCommand::SelectIndex4
            | TabCommand::SelectIndex5
            | TabCommand::SelectIndex6
            | TabCommand::SelectIndex7
            | TabCommand::SelectIndex8
            | TabCommand::SelectLast => {}
```

with:

```rust
            TabCommand::SelectIndex1
            | TabCommand::SelectIndex2
            | TabCommand::SelectIndex3
            | TabCommand::SelectIndex4
            | TabCommand::SelectIndex5
            | TabCommand::SelectIndex6
            | TabCommand::SelectIndex7
            | TabCommand::SelectIndex8
            | TabCommand::SelectLast => {
                let Ok(pane) = active_pane.single() else {
                    continue;
                };
                let Ok(children) = pane_children.get(pane) else {
                    continue;
                };
                let tabs: Vec<Entity> = children
                    .iter()
                    .filter(|&e| tab_q.contains(e))
                    .collect();
                if tabs.is_empty() {
                    continue;
                }
                let target_idx = match tab_cmd {
                    TabCommand::SelectIndex1 => 0,
                    TabCommand::SelectIndex2 => 1,
                    TabCommand::SelectIndex3 => 2,
                    TabCommand::SelectIndex4 => 3,
                    TabCommand::SelectIndex5 => 4,
                    TabCommand::SelectIndex6 => 5,
                    TabCommand::SelectIndex7 => 6,
                    TabCommand::SelectIndex8 => 7,
                    TabCommand::SelectLast => tabs.len() - 1,
                    _ => continue,
                };
                if target_idx >= tabs.len() {
                    continue;
                }
                for &t in &tabs {
                    if active_tabs.contains(t) {
                        commands.entity(t).remove::<Active>();
                    }
                }
                commands.entity(tabs[target_idx]).insert(Active);
            }
```

- [ ] **Step 2: Run cargo check**

Run: `cargo check -p vmux_desktop out+err>| tail -20`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git commit -m "Implement tab index selection (Cmd+1-9)"
```

---

### Task 6: Final verification

**Files:** None (read-only)

- [ ] **Step 1: Full compilation check**

Run: `cargo check -p vmux_desktop out+err>| tail -20`
Expected: PASS with no warnings related to our changes.

- [ ] **Step 2: Verify no comments in command.rs**

Run: `rg "// " crates/vmux_desktop/src/command.rs`
Expected: No matches.

- [ ] **Step 3: Verify WindowCommand is exported if needed**

Run: `rg "WindowCommand" crates/vmux_desktop/src/`
Expected: Appears in command.rs (definition + AppCommand variant). No handler needed — all variants are stubs, and unmatched `AppCommand::Window(...)` messages are ignored by other handlers' `else { continue }` patterns.

- [ ] **Step 4: Verify bind attribute casing**

Run: `rg '#\[bind' crates/vmux_desktop/src/command.rs`
Expected: All chord bindings use capitalized modifiers (`Ctrl+b`, `Alt+Left`, etc.), matching `parse_key_combo_tokens` expectations in vmux_macro.
