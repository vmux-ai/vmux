# OSC-driven dynamic tab titles — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let any PTY program drive its tab label via OSC title escape sequences, reverting to the default label when the program clears the title or exits.

**Architecture:** OSC titles are already parsed by `alacritty_terminal` and broadcast to the client as `ServiceMessage::ProcessTitle`. Add a non-persisted `OscTitle` component on the terminal entity, set/cleared via a typed Bevy message from the existing `ProcessTitle` arm, cleared on process exit, and overlaid over `PageMetadata.title` in the three `vmux_browser` tab-strip emitters. `OscTitle` stays out of the save allowlist, so `space.ron` keeps the stable default and auto-save never thrashes.

**Tech Stack:** Rust, Bevy ECS (this fork uses `Message`/`Messages`/`MessageReader`/`MessageWriter`), `alacritty_terminal`, CEF webviews.

**Spec:** `docs/specs/2026-06-12-osc-tab-titles-design.md`

---

## File Structure

- `crates/vmux_core/src/lib.rs` — new `OscTitle` component (next to `PageMetadata`).
- `crates/vmux_browser/src/lib.rs` — `effective_title` helper + overlay in `push_stacks_host_emit`, `push_pane_tree_emit`, `push_tabs_host_emit` (+ `first_browser_meta`).
- `crates/vmux_terminal/src/plugin.rs` — `OscTitleChanged` message, `apply_osc_title` + `clear_osc_title_on_exit` systems, write message from the `ProcessTitle` arm, registration.

No changes to `vmux_service` (source already emits `ProcessTitle`), `vmux_layout/snapshot.rs` (MCP/reconcile keep the default), or `persistence.rs` (omission from the allowlist is the intended behavior).

---

### Task 1: `OscTitle` component in `vmux_core`

**Files:**
- Modify: `crates/vmux_core/src/lib.rs` (immediately after the `PageMetadata` struct, ~line 68)

- [ ] **Step 1: Add the component**

Insert after the closing `}` of `PageMetadata`:

```rust
/// Live OSC (0/2) terminal title overriding the default tab label.
/// Absent when no program-set title is active. Never persisted.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Debug)]
pub struct OscTitle(pub String);
```

- [ ] **Step 2: Build**

Run: `cargo build -p vmux_core`
Expected: compiles clean.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_core/src/lib.rs
git commit -m "feat(core): add OscTitle component for terminal OSC titles"
```

---

### Task 2: `effective_title` helper in `vmux_browser`

**Files:**
- Modify: `crates/vmux_browser/src/lib.rs` (add helper near `first_browser_meta`, ~line 2242; add `OscTitle` to the `vmux_core` import at line 26; add test in the `tests` mod at ~line 3014)

- [ ] **Step 1: Write the failing test**

In the `#[cfg(test)] mod tests` block, add:

```rust
#[test]
fn effective_title_prefers_nonempty_osc() {
    use vmux_core::OscTitle;
    assert_eq!(effective_title(Some(&OscTitle("osc".to_string())), "def"), "osc");
    assert_eq!(effective_title(Some(&OscTitle(String::new())), "def"), "def");
    assert_eq!(effective_title(None, "def"), "def");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_browser effective_title_prefers_nonempty_osc`
Expected: FAIL — `cannot find function effective_title`.

- [ ] **Step 3: Add `OscTitle` to imports and implement the helper**

Edit the import at line 26 from:

```rust
use vmux_core::{
    CefPageAttachRequest, PageMetadata, PageOpenError, PageOpenHandled, PageOpenId,
```

to add `OscTitle`:

```rust
use vmux_core::{
    CefPageAttachRequest, OscTitle, PageMetadata, PageOpenError, PageOpenHandled, PageOpenId,
```

Add the helper next to `first_browser_meta` (above it, ~line 2242):

```rust
fn effective_title<'a>(osc: Option<&'a OscTitle>, default: &'a str) -> &'a str {
    match osc {
        Some(OscTitle(t)) if !t.is_empty() => t,
        _ => default,
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_browser effective_title_prefers_nonempty_osc`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_browser/src/lib.rs
git commit -m "feat(browser): add effective_title OSC overlay helper"
```

---

### Task 3: Overlay `OscTitle` in the three tab-strip emitters

**Files:**
- Modify: `crates/vmux_browser/src/lib.rs`
  - `push_stacks_host_emit` (query ~1969, loop ~1995, title ~2007)
  - `push_pane_tree_emit` (query ~2056, match ~2092, title ~2099)
  - `push_tabs_host_emit` (query ~2166, body ~2195-2212) and `first_browser_meta` (~2242)

- [ ] **Step 1: `push_stacks_host_emit`**

Change the query (line 1969) from:

```rust
    browser_q: Query<(&PageMetadata, &ChildOf, Option<&NavigationState>), With<Browser>>,
```

to:

```rust
    browser_q: Query<(&PageMetadata, &ChildOf, Option<&NavigationState>, Option<&OscTitle>), With<Browser>>,
```

Change the loop (line 1995) from:

```rust
        for (meta, child_of, nav_state) in &browser_q {
```

to:

```rust
        for (meta, child_of, nav_state, osc) in &browser_q {
```

Change the title field (line 2007) from:

```rust
                title: meta.title.clone(),
```

to:

```rust
                title: effective_title(osc, &meta.title).to_string(),
```

- [ ] **Step 2: `push_pane_tree_emit`**

Change the query (line 2056) from:

```rust
    browser_meta: Query<(&PageMetadata, Has<Loading>), With<Browser>>,
```

to:

```rust
    browser_meta: Query<(&PageMetadata, Has<Loading>, Option<&OscTitle>), With<Browser>>,
```

Change the match (line 2092) from:

```rust
                        if let Ok((meta, loading)) = browser_meta.get(browser_e) {
```

to:

```rust
                        if let Ok((meta, loading, osc)) = browser_meta.get(browser_e) {
```

Change the title (lines 2096-2100) from:

```rust
                                title: if is_new_stack {
                                    "New Stack".to_string()
                                } else {
                                    meta.title.clone()
                                },
```

to:

```rust
                                title: if is_new_stack {
                                    "New Stack".to_string()
                                } else {
                                    effective_title(osc, &meta.title).to_string()
                                },
```

- [ ] **Step 3: `push_tabs_host_emit` + `first_browser_meta`**

Change `first_browser_meta` (lines 2242-2249) from:

```rust
fn first_browser_meta<'a>(
    stack: Entity,
    stack_children: &Query<&Children>,
    browser_meta: &'a Query<&PageMetadata, With<Browser>>,
) -> Option<&'a PageMetadata> {
    let kids = stack_children.get(stack).ok()?;
    kids.iter().find_map(|c| browser_meta.get(c).ok())
}
```

to:

```rust
fn first_browser_meta<'a>(
    stack: Entity,
    stack_children: &Query<&Children>,
    browser_meta: &'a Query<(&PageMetadata, Option<&OscTitle>), With<Browser>>,
) -> Option<(&'a PageMetadata, Option<&'a OscTitle>)> {
    let kids = stack_children.get(stack).ok()?;
    kids.iter().find_map(|c| browser_meta.get(c).ok())
}
```

Change the `push_tabs_host_emit` query (line 2166) from:

```rust
    browser_meta: Query<&PageMetadata, With<Browser>>,
```

to:

```rust
    browser_meta: Query<(&PageMetadata, Option<&OscTitle>), With<Browser>>,
```

Change the row body (lines 2195-2212) from:

```rust
            let meta = active_stack
                .and_then(|s| first_browser_meta(s, &stack_children, &browser_meta))
                .cloned()
                .unwrap_or_default();
            let name = if tab.name.is_empty() {
                "Tab".to_string()
            } else {
                tab.name.clone()
            };
            TabRow {
                id: entity.to_bits().to_string(),
                name,
                is_active: Some(entity) == active_tab,
                bg_color: meta.bg_color.clone(),
                title: meta.title.clone(),
                url: meta.url.clone(),
                favicon_url: meta.favicon_url.clone(),
            }
```

to:

```rust
            let found = active_stack
                .and_then(|s| first_browser_meta(s, &stack_children, &browser_meta));
            let title = found
                .map(|(meta, osc)| effective_title(osc, &meta.title).to_string())
                .unwrap_or_default();
            let (url, favicon_url, bg_color) = found
                .map(|(meta, _)| (meta.url.clone(), meta.favicon_url.clone(), meta.bg_color.clone()))
                .unwrap_or_default();
            let name = if tab.name.is_empty() {
                "Tab".to_string()
            } else {
                tab.name.clone()
            };
            TabRow {
                id: entity.to_bits().to_string(),
                name,
                is_active: Some(entity) == active_tab,
                bg_color,
                title,
                url,
                favicon_url,
            }
```

- [ ] **Step 4: Build + format**

Run: `cargo build -p vmux_browser && cargo fmt -p vmux_browser`
Expected: compiles clean.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_browser/src/lib.rs
git commit -m "feat(browser): overlay OSC title over default tab label in strip emitters"
```

---

### Task 4: `OscTitleChanged` message + `apply_osc_title` system

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs`
  - add `OscTitle` to the `vmux_core` import (line 23)
  - define `OscTitleChanged` near `ProcessExitedEvent` (~line 2800)
  - add `apply_osc_title` system
  - register message + system in `add_terminal_update_systems` (~line 329)
  - test in the `tests` mod

- [ ] **Step 1: Write the failing test**

In the `#[cfg(test)] mod tests` block, add:

```rust
#[test]
fn apply_osc_title_sets_and_clears() {
    use bevy::ecs::message::Messages;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<OscTitleChanged>()
        .add_systems(Update, apply_osc_title);
    let pid = ProcessId::new();
    let e = app.world_mut().spawn((Terminal, pid)).id();

    app.world_mut()
        .resource_mut::<Messages<OscTitleChanged>>()
        .write(OscTitleChanged {
            process_id: pid,
            title: "claude — repo".to_string(),
        });
    app.update();
    assert_eq!(
        app.world().get::<vmux_core::OscTitle>(e).map(|o| o.0.clone()),
        Some("claude — repo".to_string())
    );

    app.world_mut()
        .resource_mut::<Messages<OscTitleChanged>>()
        .write(OscTitleChanged {
            process_id: pid,
            title: String::new(),
        });
    app.update();
    assert!(app.world().get::<vmux_core::OscTitle>(e).is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_terminal apply_osc_title_sets_and_clears`
Expected: FAIL — `cannot find type/function OscTitleChanged` / `apply_osc_title`.

- [ ] **Step 3: Add `OscTitle` import, message, and system**

Add `OscTitle` to the import at line 23:

```rust
use vmux_core::{OscTitle, PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};
```

Define the message next to `ProcessExitedEvent` (~line 2800):

```rust
#[derive(Message, Debug, Clone)]
pub struct OscTitleChanged {
    pub process_id: ProcessId,
    pub title: String,
}
```

Add the system (place it next to `OscTitleChanged`):

```rust
pub fn apply_osc_title(
    mut reader: MessageReader<OscTitleChanged>,
    mut commands: Commands,
    terminals: Query<(Entity, &ProcessId, Option<&OscTitle>), With<Terminal>>,
) {
    for ev in reader.read() {
        let Some((entity, _, current)) =
            terminals.iter().find(|(_, pid, _)| **pid == ev.process_id)
        else {
            continue;
        };
        if ev.title.is_empty() {
            if current.is_some() {
                commands.entity(entity).remove::<OscTitle>();
            }
        } else if current.map(|o| o.0.as_str()) != Some(ev.title.as_str()) {
            commands.entity(entity).insert(OscTitle(ev.title.clone()));
        }
    }
}
```

- [ ] **Step 4: Register the message and system**

In `add_terminal_update_systems` (line 329), change:

```rust
    app.add_message::<ProcessExitedEvent>()
        .add_systems(Update, respawn_shell_on_vibe_exit)
```

to:

```rust
    app.add_message::<ProcessExitedEvent>()
        .add_message::<OscTitleChanged>()
        .add_systems(Update, apply_osc_title.after(poll_service_messages))
        .add_systems(Update, respawn_shell_on_vibe_exit)
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vmux_terminal apply_osc_title_sets_and_clears`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_terminal/src/plugin.rs
git commit -m "feat(terminal): apply OscTitle from OscTitleChanged message"
```

---

### Task 5: Emit `OscTitleChanged` from the `ProcessTitle` arm

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs`
  - `PollServiceWriters` (lines 957-963)
  - `ServiceMessage::ProcessTitle` arm (line 1079)

- [ ] **Step 1: Add the writer field**

Change `PollServiceWriters` (lines 957-963) from:

```rust
#[derive(bevy::ecs::system::SystemParam)]
struct PollServiceWriters<'w> {
    app_commands: MessageWriter<'w, AppCommand>,
    agent_commands: MessageWriter<'w, vmux_service::agent_events::AgentCommandRequest>,
    agent_queries: MessageWriter<'w, vmux_service::agent_events::AgentQueryRequest>,
    process_exited: MessageWriter<'w, ProcessExitedEvent>,
}
```

to:

```rust
#[derive(bevy::ecs::system::SystemParam)]
struct PollServiceWriters<'w> {
    app_commands: MessageWriter<'w, AppCommand>,
    agent_commands: MessageWriter<'w, vmux_service::agent_events::AgentCommandRequest>,
    agent_queries: MessageWriter<'w, vmux_service::agent_events::AgentQueryRequest>,
    process_exited: MessageWriter<'w, ProcessExitedEvent>,
    osc_title: MessageWriter<'w, OscTitleChanged>,
}
```

- [ ] **Step 2: Write the message in the arm**

Change the `ServiceMessage::ProcessTitle` arm (lines 1079-1094) from:

```rust
            ServiceMessage::ProcessTitle { process_id, title } => {
                for (entity, pid, _) in &terminals {
                    if *pid == process_id {
                        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                            continue;
                        }
                        let evt = TermTitleEvent { title };
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            TERM_TITLE_EVENT,
                            &evt,
                        ));
                        break;
                    }
                }
            }
```

to:

```rust
            ServiceMessage::ProcessTitle { process_id, title } => {
                writers.osc_title.write(OscTitleChanged {
                    process_id,
                    title: title.clone(),
                });
                for (entity, pid, _) in &terminals {
                    if *pid == process_id {
                        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                            continue;
                        }
                        let evt = TermTitleEvent { title };
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            TERM_TITLE_EVENT,
                            &evt,
                        ));
                        break;
                    }
                }
            }
```

- [ ] **Step 3: Build**

Run: `cargo build -p vmux_terminal`
Expected: compiles clean.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_terminal/src/plugin.rs
git commit -m "feat(terminal): emit OscTitleChanged from ProcessTitle service message"
```

---

### Task 6: Clear `OscTitle` on process exit

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs`
  - add `clear_osc_title_on_exit` system (next to `apply_osc_title`)
  - register it in `add_terminal_update_systems` (~line 330)
  - test in the `tests` mod

- [ ] **Step 1: Write the failing test**

In the `#[cfg(test)] mod tests` block, add:

```rust
#[test]
fn clear_osc_title_on_exit_removes_override() {
    use bevy::ecs::message::Messages;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<ProcessExitedEvent>()
        .add_systems(Update, clear_osc_title_on_exit);
    let pid = ProcessId::new();
    let e = app
        .world_mut()
        .spawn((Terminal, pid, vmux_core::OscTitle("working".to_string())))
        .id();

    app.world_mut()
        .resource_mut::<Messages<ProcessExitedEvent>>()
        .write(ProcessExitedEvent { process_id: pid });
    app.update();
    assert!(app.world().get::<vmux_core::OscTitle>(e).is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_terminal clear_osc_title_on_exit_removes_override`
Expected: FAIL — `cannot find function clear_osc_title_on_exit`.

- [ ] **Step 3: Add the system**

Next to `apply_osc_title`, add:

```rust
pub fn clear_osc_title_on_exit(
    mut reader: MessageReader<ProcessExitedEvent>,
    mut commands: Commands,
    terminals: Query<(Entity, &ProcessId), (With<Terminal>, With<OscTitle>)>,
) {
    for ev in reader.read() {
        if let Some((entity, _)) = terminals.iter().find(|(_, pid)| **pid == ev.process_id) {
            commands.entity(entity).remove::<OscTitle>();
        }
    }
}
```

- [ ] **Step 4: Register the system**

In `add_terminal_update_systems`, change:

```rust
        .add_systems(Update, apply_osc_title.after(poll_service_messages))
```

to:

```rust
        .add_systems(Update, apply_osc_title.after(poll_service_messages))
        .add_systems(Update, clear_osc_title_on_exit.after(poll_service_messages))
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vmux_terminal clear_osc_title_on_exit_removes_override`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_terminal/src/plugin.rs
git commit -m "feat(terminal): clear OscTitle when the process exits"
```

---

### Task 7: Whole-workspace checks + manual verification

- [ ] **Step 1: fmt + clippy + tests**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p vmux_core -p vmux_browser -p vmux_terminal`
Expected: no fmt diff, no clippy warnings, all tests pass.

- [ ] **Step 2: Manual verification (golden path)**

Build and run the desktop app. In a terminal tab:
- Run `printf '\033]2;hello vmux\007'` → the tab label changes to `hello vmux`.
- Run `printf '\033]2;\007'` (empty) → the tab label reverts to the default (`Terminal (…)`).
- Open a `claude` (or `codex`) agent tab; confirm its tab label tracks the title the CLI emits while running, and reverts to the default after the CLI exits / the shell respawns.
- Restart the app (with a saved space) → confirm the restored tab shows the default label, not a stale dynamic title.

Note: the desktop app embeds CEF and cannot be exercised by the automated test suite; this manual pass is the feature-level verification.

- [ ] **Step 3: Delete this plan file (per AGENTS.md) and commit**

```bash
git rm docs/plans/2026-06-12-osc-tab-titles.md
git commit -m "chore: remove completed OSC tab titles plan"
```

---

## Self-Review

**Spec coverage:**
- All PTY terminals in scope → overlay lives in the shared `With<Browser>` emitters (Task 3); applies to plain + agent tabs. ✓
- OSC overrides default; empty clears → `apply_osc_title` (Task 4). ✓
- Revert on exit → `clear_osc_title_on_exit` (Task 6). ✓
- No persistence staleness / no autosave churn → `OscTitle` is a separate component, never added to the save allowlist, never mutates `PageMetadata` (Tasks 1, 4, 6). ✓
- Source path unchanged → message written from existing `ProcessTitle` arm (Task 5). ✓
- MCP/reconcile keep default → `snapshot.rs` untouched. ✓

**Placeholder scan:** none — every code step contains full code and exact commands.

**Type consistency:** `OscTitle(pub String)`, `OscTitleChanged { process_id: ProcessId, title: String }`, `effective_title(Option<&OscTitle>, &str) -> &str`, `first_browser_meta(...) -> Option<(&PageMetadata, Option<&OscTitle>)>` used consistently across Tasks 1-6.
