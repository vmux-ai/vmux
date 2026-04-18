# Active Component Refactor

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Change `Active` from "exactly 1 globally per type" to "exactly 1 per parent scope" — each Pane keeps its own Active Tab, pane focus switching no longer touches tabs.

**Architecture:** `Active` remains a single shared component. The invariant changes: Active+Tab means "selected tab within this pane" (1 per pane), Active+Pane means "focused pane" (1 globally, future: 1 per space). All `.single()` calls on Active+Tab are replaced with a helper that walks the Active chain (Active Pane → its Active Tab child). Pane focus switching only moves Active between Panes, never touches Tabs.

**Tech Stack:** Bevy 0.18 ECS, Rust

---

## File Map

| File | Role | Changes |
|------|------|---------|
| `crates/vmux_desktop/src/layout/tab.rs` | Active component, tab_bundle, tab commands | Add `active_tab_in_focused_pane` helper; scope tab commands to active pane |
| `crates/vmux_desktop/src/layout/pane.rs` | Pane split/close/cycle/hover | Stop removing Active from tabs on pane switch; ensure new panes spawn with Active tab |
| `crates/vmux_desktop/src/browser.rs` | All browser↔Active sync | Replace every `active_tab.single()` with helper |
| `crates/vmux_desktop/src/layout/focus_ring.rs` | Focus ring positioning | No change needed (already queries Active+Pane) |
| `crates/vmux_desktop/src/layout/display.rs` | Initial hierarchy | Ensure initial tab has Active (already does) |

## Current vs New Invariants

```
CURRENT:  Active+Tab  → exactly 1 globally     → .single()
NEW:      Active+Tab  → exactly 1 per Pane     → walk Active Pane → find Active Tab child

CURRENT:  Active+Pane → exactly 1 globally     → .single()  (unchanged)
NEW:      Active+Pane → exactly 1 globally     → .single()  (unchanged, future: 1 per space)
```

---

### Task 1: Add `focused_tab` helper function

**Files:**
- Modify: `crates/vmux_desktop/src/layout/tab.rs`

This helper replaces all `active_tab.single()` patterns. It walks: Active Pane → Children → find Active+Tab child.

- [ ] **Step 1: Add the helper function to tab.rs**

After the `Active` component definition (line 22), add:

```rust
pub(crate) fn focused_tab(
    active_pane: &Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: &Query<&Children, With<Pane>>,
    active_tabs: &Query<Entity, (With<Active>, With<Tab>)>,
) -> Option<Entity> {
    let pane = active_pane.single().ok()?;
    let children = pane_children.get(pane).ok()?;
    children.iter().find(|&e| active_tabs.contains(e))
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p vmux_desktop out+err>| tail -5`
Expected: compiles (function is unused for now, no warning in lib crate)

- [ ] **Step 3: Commit**

```
feat: add focused_tab helper for Active chain traversal
```

---

### Task 2: Refactor `handle_tab_commands` to use per-pane Active

**Files:**
- Modify: `crates/vmux_desktop/src/layout/tab.rs`

Currently `TabCommand::Close` uses `active_tabs.single()` which assumes 1 globally. Change it to find the Active tab within the Active pane.

- [ ] **Step 1: Replace `active_tabs.single()` with `focused_tab` in Close**

Replace lines 83-84:
```rust
                let Ok(active_tab) = active_tabs.single() else {
                    continue;
                };
```
with:
```rust
                let Some(active_tab) = focused_tab(&active_pane, &pane_children, &active_tabs) else {
                    continue;
                };
```

- [ ] **Step 2: In TabCommand::New, scope the Active removal to the active pane only**

The current code (lines 63-68) already iterates `pane_children.get(pane)` and checks `active_tabs.contains(child)`, which is already per-pane scoped. No change needed.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_desktop out+err>| tail -5`

- [ ] **Step 4: Commit**

```
refactor: tab commands use focused_tab instead of global single
```

---

### Task 3: Stop removing Active from tabs on pane focus switch

**Files:**
- Modify: `crates/vmux_desktop/src/layout/pane.rs`

Currently `on_pane_click` and `on_pane_cycle` remove Active from the old pane's tab and insert Active on the new pane's tab. With per-pane Active, each pane keeps its own Active tab — we only move Active between Panes.

- [ ] **Step 1: Simplify `on_pane_click` (was `on_pane_hover`)**

Current code (approximately lines 281-317) does:
1. Find new tab in target pane → bail if none
2. Remove Active from old pane's tab
3. Remove Active from old pane
4. Insert Active on new pane
5. Insert Active on new tab

Change to:
1. Remove Active from old pane
2. Insert Active on new pane
(tabs keep their own Active untouched)

Replace the body of `on_pane_click` with:

```rust
fn on_pane_click(
    trigger: On<Pointer<Over>>,
    pane_q: Query<(), (With<Pane>, Without<PaneSplit>)>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    if !pane_q.contains(entity) {
        return;
    }
    if let Ok(current) = active_pane.single() {
        if current == entity {
            return;
        }
        commands.entity(current).remove::<Active>();
    }
    commands.entity(entity).insert(Active);
}
```

- [ ] **Step 2: Simplify `on_pane_cycle`**

Current code removes Active from old tab, inserts on new tab. Change to only move Active between panes. Remove the tab-related queries and logic.

Replace the core of `on_pane_cycle` (the part after finding `target_pane`):

```rust
        commands.entity(current_pane).remove::<Active>();
        commands.entity(target_pane).insert(Active);
```

Remove the guard that bails if target pane has no tabs — pane focus switch is always valid now. Remove `active_tabs`, `pane_children`, `tab_q` from the query parameters if no longer used.

The full function becomes:

```rust
fn on_pane_cycle(
    mut reader: MessageReader<AppCommand>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        let delta: i32 = match cmd {
            AppCommand::Tab(TabCommand::Next) => 1,
            AppCommand::Tab(TabCommand::Previous) => -1,
            _ => continue,
        };
        let mut panes: Vec<Entity> = leaf_panes.iter().collect();
        if panes.len() < 2 {
            continue;
        }
        panes.sort_by_key(|e| e.to_bits());
        let Ok(current_pane) = active_pane.single() else {
            continue;
        };
        let Some(pos) = panes.iter().position(|&e| e == current_pane) else {
            continue;
        };
        let n = panes.len() as i32;
        let idx = (pos as i32 + delta).rem_euclid(n) as usize;
        let target_pane = panes[idx];

        commands.entity(current_pane).remove::<Active>();
        commands.entity(target_pane).insert(Active);
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_desktop out+err>| tail -5`

- [ ] **Step 4: Commit**

```
refactor: pane focus switch no longer touches tab Active
```

---

### Task 4: Fix pane split to give new pane's tab Active

**Files:**
- Modify: `crates/vmux_desktop/src/layout/pane.rs`

When splitting, the old pane's tab already has Active (and keeps it). The new pane gets a new tab that also needs Active. Currently the code removes Active from the old tab — stop doing that.

- [ ] **Step 1: Simplify split handler's Active management**

In `handle_pane_commands`, the `SplitV | SplitH` branch currently does (around lines 139-141):
```rust
if let Some(old_active_tab) = active_tab_in_pane(active, &pane_children, &active_tabs) {
    commands.entity(old_active_tab).remove::<Active>();
}
commands.entity(pane2).insert(Active);
```

Change to just:
```rust
commands.entity(pane2).insert(Active);
```

The old pane's tab keeps Active (it's the selected tab in that pane). The new tab in pane2 is spawned with Active (line 126). The old pane loses Active (line 133: `remove::<Active>()`). pane2 gets Active.

Also remove `active_tabs` from `handle_pane_commands` query parameters if it's no longer used anywhere in the function body (check the Close branch — it uses `active_tab_in_pane` which needs `active_tabs`). If Close still needs it, keep the parameter.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p vmux_desktop out+err>| tail -5`

- [ ] **Step 3: Commit**

```
refactor: split keeps old pane's Active tab untouched
```

---

### Task 5: Fix pane close Active management

**Files:**
- Modify: `crates/vmux_desktop/src/layout/pane.rs`

When closing a pane, the surviving pane already has its own Active tab. We just need to set Active on the surviving pane. No tab Active manipulation needed.

- [ ] **Step 1: Simplify close handler's Active on surviving pane**

In the Close branch, after determining `new_active_pane`, the current code does:
```rust
commands.entity(new_active_pane).insert(Active);
let tab = active_tab_in_pane(new_active_pane, &pane_children, &active_tabs)
    .or_else(|| first_tab_in_pane(new_active_pane, &pane_children, &tab_filter))
    .or_else(|| sibling_children.iter().copied().find(|&e| tab_filter.contains(e)));
if let Some(tab) = tab {
    commands.entity(tab).insert(Active);
}
```

The tab lookup is still needed as a safety net — if the surviving pane somehow has no Active tab, we activate one. Keep this logic but understand it's a fallback, not the primary path. No change needed here.

- [ ] **Step 2: Commit (if any changes made)**

```
refactor: pane close relies on surviving pane's existing Active tab
```

---

### Task 6: Refactor `sync_keyboard_target` in browser.rs

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`

Replace `active_tab.single()` with the `focused_tab` helper.

- [ ] **Step 1: Add imports**

Add to the imports at the top of browser.rs:
```rust
use crate::layout::tab::focused_tab;
```

- [ ] **Step 2: Add `active_pane` and `pane_children` queries to `sync_keyboard_target`**

Change the function signature from:
```rust
fn sync_keyboard_target(
    active_tab: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    browser_q: Query<(Entity, Has<CefKeyboardTarget>), With<Browser>>,
    mut commands: Commands,
) {
    let Ok(active_tab_entity) = active_tab.single() else {
        return;
    };
```

to:
```rust
fn sync_keyboard_target(
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
    status_q: Query<(), With<Header>>,
    side_sheet_q: Query<(), With<SideSheet>>,
    browser_q: Query<(Entity, Has<CefKeyboardTarget>), With<Browser>>,
    mut commands: Commands,
) {
    let Some(active_tab_entity) = focused_tab(&active_pane, &pane_children, &active_tabs) else {
        return;
    };
```

Rest of the function body stays the same — it already uses `active_tab_entity` as a local variable.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_desktop out+err>| tail -5`

- [ ] **Step 4: Commit**

```
refactor: sync_keyboard_target uses focused_tab helper
```

---

### Task 7: Refactor `sync_osr_webview_focus` in browser.rs

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`

- [ ] **Step 1: Replace the active tab lookup**

Change:
```rust
fn sync_osr_webview_focus(
    browsers: NonSend<Browsers>,
    webviews: Query<Entity, With<WebviewSource>>,
    active_tab: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
```
to:
```rust
fn sync_osr_webview_focus(
    browsers: NonSend<Browsers>,
    webviews: Query<Entity, With<WebviewSource>>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children_q: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    child_of_q: Query<&ChildOf>,
```

And replace the active resolution block:
```rust
    let active = active_tab
        .single()
        .ok()
        .and_then(|tab| {
            ready.iter().copied().find(|&b| {
                child_of_q.get(b).ok().map(|co| co.get()) == Some(tab)
            })
        })
        .unwrap_or(ready[0]);
```
with:
```rust
    let active = focused_tab(&active_pane, &pane_children_q, &active_tabs)
        .and_then(|tab| {
            ready.iter().copied().find(|&b| {
                child_of_q.get(b).ok().map(|co| co.get()) == Some(tab)
            })
        })
        .unwrap_or(ready[0]);
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p vmux_desktop out+err>| tail -5`

- [ ] **Step 3: Commit**

```
refactor: sync_osr_webview_focus uses focused_tab helper
```

---

### Task 8: Refactor `push_tabs_host_emit` in browser.rs

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`

- [ ] **Step 1: Replace active tab query**

Change query parameter from:
```rust
    active_tab: Query<Entity, (With<Active>, With<Tab>)>,
```
to:
```rust
    active_pane_q: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children_q: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
```

Replace:
```rust
    let Ok(active_tab_entity) = active_tab.single() else {
        return;
    };
    let active_pane = child_of_q
        .get(active_tab_entity)
        .map(|co| co.get());
```
with:
```rust
    let Some(active_tab_entity) = focused_tab(&active_pane_q, &pane_children_q, &active_tabs) else {
        return;
    };
    let active_pane = active_pane_q.single().ok();
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p vmux_desktop out+err>| tail -5`

- [ ] **Step 3: Commit**

```
refactor: push_tabs_host_emit uses focused_tab helper
```

---

### Task 9: Refactor `push_pane_tree_emit` in browser.rs

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`

- [ ] **Step 1: Update tab active detection to be per-pane**

Currently:
```rust
    let active_tab = active_tab_q.single().ok();
```
and later:
```rust
    is_active: Some(tab_entity) == active_tab,
```

This only marks 1 tab as active globally. Change to check per-pane: for each pane being iterated, find its Active tab child.

Replace:
```rust
    let active_tab = active_tab_q.single().ok();
```
with:
```rust
    // active_tab_q is used per-pane below
```

And in the inner loop where tabs are iterated per pane, replace:
```rust
    is_active: Some(tab_entity) == active_tab,
```
with:
```rust
    is_active: active_tab_q.contains(tab_entity),
```

This correctly marks the Active tab in EACH pane, not just one globally.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p vmux_desktop out+err>| tail -5`

- [ ] **Step 3: Commit**

```
refactor: push_pane_tree_emit marks Active tab per-pane
```

---

### Task 10: Refactor `handle_browser_commands` in browser.rs

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`

- [ ] **Step 1: Replace active tab query**

Change:
```rust
fn handle_browser_commands(
    mut reader: MessageReader<AppCommand>,
    active_tab: Query<Entity, (With<Active>, With<Tab>)>,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut commands: Commands,
) {
    ...
        let Ok(active) = active_tab.single() else {
```
to:
```rust
fn handle_browser_commands(
    mut reader: MessageReader<AppCommand>,
    active_pane: Query<Entity, (With<Active>, With<Pane>)>,
    pane_children: Query<&Children, With<Pane>>,
    active_tabs: Query<Entity, (With<Active>, With<Tab>)>,
    browsers: Query<(Entity, &ChildOf), (With<Browser>, Without<Header>, Without<SideSheet>)>,
    mut commands: Commands,
) {
    ...
        let Some(active) = focused_tab(&active_pane, &pane_children, &active_tabs) else {
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p vmux_desktop out+err>| tail -5`

- [ ] **Step 3: Commit**

```
refactor: handle_browser_commands uses focused_tab helper
```

---

### Task 11: Refactor `on_side_sheet_command_emit` in browser.rs

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`

This observer handles "activate_tab" from the side sheet. It needs to:
1. Set Active on the target pane (pane focus)
2. Set Active on the target tab within that pane (tab selection)
3. Remove Active from the OLD focused pane only — not from any tab

- [ ] **Step 1: Simplify Active management**

Replace the Active swap logic:
```rust
    if let Ok(old_tab) = active_tab_q.single() {
        if old_tab != target_tab {
            commands.entity(old_tab).remove::<Active>();
        }
    }
    if let Ok(old_pane) = active_pane_q.single() {
        if old_pane != target_pane {
            commands.entity(old_pane).remove::<Active>();
        }
    }
    commands.entity(target_pane).insert(Active);
    commands.entity(target_tab).insert(Active);
```
with:
```rust
    if let Ok(old_pane) = active_pane_q.single() {
        if old_pane != target_pane {
            commands.entity(old_pane).remove::<Active>();
        }
    }
    if let Some(old_tab) = active_tab_in_pane(target_pane, &pane_children, &active_tabs) {
        if old_tab != target_tab {
            commands.entity(old_tab).remove::<Active>();
        }
    }
    commands.entity(target_pane).insert(Active);
    commands.entity(target_tab).insert(Active);
```

This removes Active from the old tab **within the same pane** (tab switching), not from a tab in a different pane. Add `active_tab_in_pane` import from `crate::layout::pane`.

Also add `pane_children: Query<&Children, With<Pane>>` and `active_tabs: Query<Entity, (With<Active>, With<Tab>)>` to the function parameters (rename existing `active_tab_q` to `active_tabs`).

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p vmux_desktop out+err>| tail -5`

- [ ] **Step 3: Commit**

```
refactor: side sheet tab activation scoped to target pane
```

---

### Task 12: Make `active_tab_in_pane` and `first_tab_in_pane` public

**Files:**
- Modify: `crates/vmux_desktop/src/layout/pane.rs`

- [ ] **Step 1: Add `pub(crate)` to both helper functions**

Change:
```rust
fn first_tab_in_pane(
```
to:
```rust
pub(crate) fn first_tab_in_pane(
```

Change:
```rust
fn active_tab_in_pane(
```
to:
```rust
pub(crate) fn active_tab_in_pane(
```

Note: This task should be done BEFORE Task 11 since Task 11 imports `active_tab_in_pane`. Reorder execution accordingly.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p vmux_desktop out+err>| tail -5`

- [ ] **Step 3: Commit**

```
refactor: expose pane tab helpers as pub(crate)
```

---

### Task 13: Clean up unused imports and dead code

**Files:**
- Modify: `crates/vmux_desktop/src/browser.rs`
- Modify: `crates/vmux_desktop/src/layout/pane.rs`

- [ ] **Step 1: Run cargo check and fix all warnings**

Run: `cargo check -p vmux_desktop out+err>| tail -30`

Look for unused import warnings, unused variable warnings. Fix each one.

- [ ] **Step 2: Verify clean compile**

Run: `cargo check -p vmux_desktop out+err>| tail -5`
Expected: no warnings except the profile package spec one

- [ ] **Step 3: Commit**

```
chore: remove unused imports after Active refactor
```

---

## Execution Order

Task 12 must run before Task 11. All other tasks can run in listed order:

1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → **12** → 11 → 13

## Verification

After all tasks, the following should hold:
- Each pane has exactly 1 Active tab (persists across pane focus switches)
- Switching panes only moves Active between Panes
- Keyboard target = browser in Active Tab of Active Pane
- Focus ring follows Active Pane
- Side sheet shows Active tab per-pane (not just one globally)
- Split creates new pane with Active tab; old pane keeps its Active tab
- Close pane: surviving pane already has Active tab, just gets pane Active
