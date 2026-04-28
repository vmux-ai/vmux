# Confirm Close Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a native macOS confirmation dialog when closing tabs/panes with live terminals or quitting the app.

**Architecture:** Add `rfd` for native dialogs. Guard each close/quit path with a sync confirmation check. Replace the OS-native quit menu item with a custom one routed through the app's command system.

**Tech Stack:** Rust, Bevy 0.18, rfd (native dialogs), muda (menus)

**Worktree:** `.worktrees/jun/vmx-89-ask-to-terminate-terminal-process`

---

### Task 1: Add `rfd` dependency

**Files:**
- Modify: `Cargo.toml` (workspace root, line ~34)
- Modify: `crates/vmux_desktop/Cargo.toml` (line ~30)

- [ ] **Step 1: Add `rfd` to workspace dependencies**

In workspace root `Cargo.toml`, add to `[workspace.dependencies]`:

```toml
rfd = "0.15"
```

- [ ] **Step 2: Add `rfd` to vmux_desktop dependencies**

In `crates/vmux_desktop/Cargo.toml`, add under `[dependencies]`:

```toml
rfd = { workspace = true }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | tail -5`
Expected: compiles without errors (warnings OK)

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/vmux_desktop/Cargo.toml
git commit -m "chore: add rfd dependency for native dialogs"
```

---

### Task 2: Add `confirm_close` setting and make `PtyExited` visible

**Files:**
- Modify: `crates/vmux_desktop/src/settings.rs:147-165`
- Modify: `crates/vmux_desktop/src/settings.ron`
- Modify: `crates/vmux_desktop/src/terminal.rs:41-43`

- [ ] **Step 1: Add `confirm_close` field to `TerminalSettings`**

In `settings.rs`, add the field to `TerminalSettings` struct (after `custom_themes`):

```rust
#[derive(Clone, Debug, Deserialize)]
pub struct TerminalSettings {
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default = "default_theme_name")]
    pub default_theme: String,
    #[serde(default)]
    pub themes: Vec<TerminalTheme>,
    #[serde(default)]
    pub custom_themes: Vec<crate::themes::TerminalColorScheme>,
    #[serde(default = "default_true")]
    pub confirm_close: bool,
}
```

Add the default function (if not already present):

```rust
fn default_true() -> bool {
    true
}
```

- [ ] **Step 2: Add `confirm_close` to settings.ron**

In `settings.ron`, inside the `terminal: Some(( ... ))` block, add:

```ron
    terminal: Some((
        default_theme: "default",
        confirm_close: true,
        themes: [
            // ... existing themes ...
        ],
    )),
```

- [ ] **Step 3: Make `PtyExited` pub(crate)**

In `terminal.rs`, change line 42-43 from:

```rust
#[derive(Component)]
struct PtyExited;
```

to:

```rust
#[derive(Component)]
pub(crate) struct PtyExited;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | tail -5`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/settings.rs crates/vmux_desktop/src/settings.ron crates/vmux_desktop/src/terminal.rs
git commit -m "feat: add confirm_close setting and expose PtyExited"
```

---

### Task 3: Create `confirm_close` module

**Files:**
- Create: `crates/vmux_desktop/src/confirm_close.rs`
- Modify: `crates/vmux_desktop/src/lib.rs` (add module declaration)

- [ ] **Step 1: Create `confirm_close.rs`**

```rust
use crate::settings::AppSettings;
use crate::terminal::PtyExited;
use bevy::prelude::*;
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

/// Check if confirmation is needed based on settings.
pub fn should_confirm(settings: &AppSettings) -> bool {
    settings
        .terminal
        .as_ref()
        .map_or(true, |t| t.confirm_close)
}

/// Check if a tab entity has any child terminal that is still running.
pub fn has_live_terminal(
    tab: Entity,
    children_q: &Query<&Children>,
    terminal_q: &Query<(), (With<crate::terminal::Terminal>, Without<PtyExited>)>,
) -> bool {
    if let Ok(children) = children_q.get(tab) {
        children.iter().any(|child| terminal_q.contains(child))
    } else {
        false
    }
}

/// Check if a pane has any tab with a live terminal.
pub fn pane_has_live_terminal(
    pane: Entity,
    pane_children_q: &Query<&Children, With<crate::layout::pane::Pane>>,
    all_children_q: &Query<&Children>,
    terminal_q: &Query<(), (With<crate::terminal::Terminal>, Without<PtyExited>)>,
) -> bool {
    if let Ok(tabs) = pane_children_q.get(pane) {
        tabs.iter()
            .any(|tab| has_live_terminal(tab, all_children_q, terminal_q))
    } else {
        false
    }
}

/// Show confirmation dialog for closing a terminal tab/pane.
/// Returns `true` if user confirms the close.
pub fn confirm_close_dialog() -> bool {
    let result = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Close Terminal?")
        .set_description("A process is still running in this terminal. Close anyway?")
        .set_buttons(MessageButtons::OkCancel)
        .show();
    matches!(result, MessageDialogResult::Ok)
}

/// Show confirmation dialog for quitting with N running terminals.
/// Returns `true` if user confirms the quit.
pub fn confirm_quit_dialog(count: usize) -> bool {
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
```

- [ ] **Step 2: Register the module in `lib.rs`**

In `crates/vmux_desktop/src/lib.rs`, add after the existing module declarations:

```rust
mod confirm_close;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | tail -5`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/confirm_close.rs crates/vmux_desktop/src/lib.rs
git commit -m "feat: add confirm_close module with dialog helpers"
```

---

### Task 4: Add confirmation to `TabCommand::Close`

**Files:**
- Modify: `crates/vmux_desktop/src/layout/tab.rs:1-15` (imports)
- Modify: `crates/vmux_desktop/src/layout/tab.rs:158-177` (system params)
- Modify: `crates/vmux_desktop/src/layout/tab.rs:225-230` (close handler)

- [ ] **Step 1: Add imports**

In `tab.rs`, add to the import block:

```rust
use crate::confirm_close;
use crate::terminal::PtyExited;
```

- [ ] **Step 2: Add query params to `handle_tab_commands`**

Add these parameters to the `handle_tab_commands` function signature (after `mut pending_warp`):

```rust
    all_children: Query<&Children>,
    live_terminal_q: Query<(), (With<Terminal>, Without<PtyExited>)>,
```

Note: `Terminal` is already imported in this file.

- [ ] **Step 3: Add confirmation guard to `TabCommand::Close`**

At the top of the `TabCommand::Close` arm, after getting `active` (the active tab entity), add the guard before any despawn logic:

```rust
            TabCommand::Close => {
                let Some(pane) = active_pane else {
                    continue;
                };
                let Some(active) = active_tab else {
                    continue;
                };

                // Confirm close if terminal is still running
                if confirm_close::should_confirm(&settings)
                    && confirm_close::has_live_terminal(active, &all_children, &live_terminal_q)
                    && !confirm_close::confirm_close_dialog()
                {
                    continue;
                }

                let Ok(children) = pane_children.get(pane) else {
                    continue;
                };
                // ... rest of existing close logic unchanged ...
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | tail -5`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/layout/tab.rs
git commit -m "feat: add close confirmation to TabCommand::Close"
```

---

### Task 5: Add confirmation to `PaneCommand::Close`

**Files:**
- Modify: `crates/vmux_desktop/src/layout/pane.rs:1-25` (imports)
- Modify: `crates/vmux_desktop/src/layout/pane.rs:176-196` (system params)
- Modify: `crates/vmux_desktop/src/layout/pane.rs:264-268` (close handler)

- [ ] **Step 1: Add imports**

In `pane.rs`, add to the import block:

```rust
use crate::confirm_close;
use crate::terminal::{Terminal, PtyExited};
```

- [ ] **Step 2: Add query params to `handle_pane_commands`**

Add these parameters to the `handle_pane_commands` function signature:

```rust
    all_children: Query<&Children>,
    live_terminal_q: Query<(), (With<Terminal>, Without<PtyExited>)>,
```

- [ ] **Step 3: Add confirmation guard to `PaneCommand::Close`**

At the top of the `PaneCommand::Close` arm, before any despawn logic, add:

```rust
            PaneCommand::Close => {
                // Confirm close if any tab in this pane has a live terminal
                if confirm_close::should_confirm(&settings)
                    && confirm_close::pane_has_live_terminal(
                        active,
                        &pane_children,
                        &all_children,
                        &live_terminal_q,
                    )
                    && !confirm_close::confirm_close_dialog()
                {
                    continue;
                }

                let Ok(pane_co) = child_of_q.get(active) else {
                    continue;
                };
                // ... rest of existing close logic unchanged ...
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | tail -5`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/layout/pane.rs
git commit -m "feat: add close confirmation to PaneCommand::Close"
```

---

### Task 6: Add confirmation to browser.rs `close_tab`

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs` (imports + `on_side_sheet_command_emit` params + close_tab handler)

- [ ] **Step 1: Add imports**

In `browser.rs`, add to imports:

```rust
use crate::confirm_close;
use crate::terminal::PtyExited;
```

(`Terminal` should already be imported in browser.rs since it's used for `Browser` — verify and add `use crate::terminal::Terminal;` if needed.)

- [ ] **Step 2: Add query params to `on_side_sheet_command_emit`**

Add to the function signature (after `mut commands: Commands`):

```rust
    live_terminal_q: Query<(), (With<crate::terminal::Terminal>, Without<PtyExited>)>,
```

Note: The function already has `all_children: Query<&Children>` which can be used to check tab children.

- [ ] **Step 3: Add confirmation guard to `"close_tab"` arm**

At the top of the `"close_tab"` match arm (line 857), after getting `target_tab`, add:

```rust
        "close_tab" => {
            let Some(&target_tab) = tab_entities.get(evt.tab_index) else {
                return;
            };

            // Confirm close if terminal is still running
            if confirm_close::should_confirm(&settings)
                && confirm_close::has_live_terminal(target_tab, &all_children, &live_terminal_q)
                && !confirm_close::confirm_close_dialog()
            {
                return;
            }

            if tab_entities.len() > 1 {
                // ... existing logic unchanged ...
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | tail -5`
Expected: compiles without errors

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/browser.rs
git commit -m "feat: add close confirmation to header close_tab"
```

---

### Task 7: Replace quit menu item and add confirmation

**Files:**
- Modify: `crates/vmux_macro/src/lib.rs:186-191` (replace PredefinedMenuItem::quit)
- Modify: `crates/vmux_desktop/src/os_menu.rs` (handle "app_quit" event)

- [ ] **Step 1: Replace quit menu item in proc macro**

In `crates/vmux_macro/src/lib.rs`, replace the quit item in the `append_items` call. Change from:

```rust
                app_native_submenu.append_items(&[
                    &::muda::PredefinedMenuItem::about(None, None),
                    &::muda::PredefinedMenuItem::separator(),
                    &::muda::PredefinedMenuItem::quit(None),
                ])?;
```

to:

```rust
                let quit_label = format!("Quit {}", &app_name);
                let quit_item = ::muda::MenuItem::with_id(
                    "app_quit",
                    &quit_label,
                    true,
                    Some("super+q".parse().unwrap()),
                );
                app_native_submenu.append_items(&[
                    &::muda::PredefinedMenuItem::about(None, None),
                    &::muda::PredefinedMenuItem::separator(),
                    &quit_item,
                ])?;
```

This routes Cmd+Q through the MenuEvent system instead of the OS-native quit.

- [ ] **Step 2: Handle "app_quit" in `forward_menu_events`**

In `os_menu.rs`, add imports at the top:

```rust
use crate::confirm_close;
use crate::settings::AppSettings;
use crate::terminal::{Terminal, PtyExited};
use bevy::app::AppExit;
```

Modify `forward_menu_events` to intercept "app_quit":

```rust
fn forward_menu_events(world: &mut World) {
    let drained = {
        let mut events = PENDING_MENU_EVENTS.lock();
        if events.is_empty() {
            return;
        }
        std::mem::take(&mut *events)
    };
    for event_id in drained {
        if event_id == "app_quit" {
            handle_quit_request(world);
        } else if let Some(cmd) = AppCommand::from_menu_id(event_id.as_str()) {
            world.resource_mut::<Messages<AppCommand>>().write(cmd);
        } else {
            warn!(len = event_id.len(), "unknown native menu item");
        }
    }
}

fn handle_quit_request(world: &mut World) {
    let should_confirm = world
        .get_resource::<AppSettings>()
        .and_then(|s| s.terminal.as_ref())
        .map_or(true, |t| t.confirm_close);

    if should_confirm {
        let mut query = world.query_filtered::<(), (With<Terminal>, Without<PtyExited>)>();
        let count = query.iter(world).count();

        if count > 0 && !confirm_close::confirm_quit_dialog(count) {
            return;
        }
    }

    world.resource_mut::<Messages<AppExit>>().write(AppExit);
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_desktop 2>&1 | tail -10`
Expected: compiles without errors

Note: If `AppExit` is not a unit struct, check Bevy 0.18 source for the correct construction (e.g., `AppExit::Success` or `AppExit::default()`).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_macro/src/lib.rs crates/vmux_desktop/src/os_menu.rs
git commit -m "feat: add quit confirmation via custom menu item"
```

---

### Task 8: Handle window close button

**Files:**
- Modify: `crates/vmux_desktop/src/os_menu.rs` (or `main.rs`)

- [ ] **Step 1: Add a system to intercept `WindowCloseRequested`**

In `os_menu.rs`, add the system and register it in `OsMenuPlugin::build`:

```rust
impl Plugin for OsMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, forward_menu_events.in_set(WriteAppCommands))
            .add_observer(on_window_close_requested);
    }
}

fn on_window_close_requested(
    trigger: Trigger<WindowCloseRequested>,
    settings: Res<AppSettings>,
    live_terminals: Query<(), (With<Terminal>, Without<PtyExited>)>,
    mut writer: MessageWriter<AppExit>,
) {
    // Prevent default close behavior — we handle it ourselves.
    // In Bevy 0.18, consuming the trigger may be enough; otherwise
    // check if there's a propagate(false) or similar API.

    let should_confirm = settings
        .terminal
        .as_ref()
        .map_or(true, |t| t.confirm_close);

    if should_confirm {
        let count = live_terminals.iter().count();
        if count > 0 && !confirm_close::confirm_quit_dialog(count) {
            return;
        }
    }

    writer.write(AppExit);
}
```

Note: Bevy 0.18's `WindowCloseRequested` behavior may differ. If the default behavior auto-exits, you may need to disable the built-in `close_when_requested` system or use `bevy_winit::WinitSettings` to configure close behavior. Verify with:

```bash
cargo doc -p bevy_winit --open
```

Look for `close_when_requested` or `WindowCloseRequested` handling.

If Bevy 0.18 uses `close_when_requested` as a named system, disable it:

```rust
// In VmuxPlugin or OsMenuPlugin build:
// app.configure_sets(...) or remove the system
```

- [ ] **Step 2: Add necessary imports**

Add to `os_menu.rs` imports (if not already present from Task 7):

```rust
use bevy::window::WindowCloseRequested;
```

- [ ] **Step 3: Verify it compiles and test manually**

Run: `cargo check -p vmux_desktop 2>&1 | tail -10`
Expected: compiles without errors

Manual test: build and run the app, click the red window close button with a terminal open. Expected: confirmation dialog appears.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/os_menu.rs
git commit -m "feat: add close confirmation for window close button"
```

---

### Task 9: Manual integration test

No automated tests for native dialog behavior. Manual verification:

- [ ] **Step 1: Build the app**

```bash
cargo build -p vmux_desktop 2>&1 | tail -5
```

- [ ] **Step 2: Test tab close with live terminal**

1. Open app, open a terminal tab
2. Press Cmd+W
3. Expected: "Close Terminal?" dialog appears
4. Click Cancel → tab stays open
5. Click OK → tab closes

- [ ] **Step 3: Test tab close with dead terminal**

1. Open a terminal, type `exit` to exit the shell
2. Tab should auto-close (PtyExited path — no dialog)

- [ ] **Step 4: Test pane close**

1. Split pane (so there are 2 panes), one with a live terminal
2. Close the pane with `<leader> x`
3. Expected: confirmation dialog

- [ ] **Step 5: Test header tab close button**

1. Open a terminal tab
2. Click the X button on the tab in the header
3. Expected: confirmation dialog

- [ ] **Step 6: Test Cmd+Q quit**

1. Open 2+ terminal tabs
2. Press Cmd+Q
3. Expected: "2 terminals are still running. Quit anyway?" dialog
4. Cancel → app stays
5. OK → app quits

- [ ] **Step 7: Test window close button**

1. Open a terminal tab
2. Click the red close button
3. Expected: confirmation dialog

- [ ] **Step 8: Test with `confirm_close: false`**

1. Edit settings.ron, set `confirm_close: false`
2. Repeat tests above
3. Expected: no dialogs, immediate close/quit (original behavior)

- [ ] **Step 9: Commit any fixes**

```bash
git add -A
git commit -m "fix: address issues found during integration testing"
```
