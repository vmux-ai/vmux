# Persist Sibling Order + Swap Commands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist tab/pane/space ordering across restarts and implement swap commands to reorder siblings.

**Architecture:** Save Bevy's `Children` component (ordered sibling list) alongside `ChildOf` in the session file. On load, `Children` is deserialized to restore order. Swap commands mutate `Children` in-place. The derive macros only support unit variants, so we keep `SwapPrev`/`SwapNext` as-is instead of `Swap(SwapTarget)`.

**Tech Stack:** Rust, Bevy 0.18, moonshine-save, RON serialization.

---

### Task 1: Persist Children ordering

**Files:**
- Modify: `crates/vmux_core/src/lib.rs`
- Modify: `crates/vmux_desktop/src/persistence.rs`

- [ ] **Step 1: Register Children and ChildOf for reflection in CorePlugin**

In `crates/vmux_core/src/lib.rs`, add to `CorePlugin::build`:

```rust
app.register_type::<PageMetadata>()
    .register_type::<CreatedAt>()
    .register_type::<LastActivatedAt>()
    .register_type::<Visit>()
    .register_type::<Children>()
    .register_type::<ChildOf>();
```

- [ ] **Step 2: Add Children to the save allowlist**

In `crates/vmux_desktop/src/persistence.rs`, in `do_save`, add after `.allow::<ChildOf>()`:

```rust
.allow::<Children>()
```

- [ ] **Step 3: Add Changed<Children> to dirty triggers**

In `crates/vmux_desktop/src/persistence.rs`, in `mark_dirty_on_change`, add parameter:

```rust
changed_children: Query<(), Changed<Children>>,
```

And add to the condition:

```rust
|| !changed_children.is_empty()
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p vmux_desktop` (stash scene.rs if needed)

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/lib.rs crates/vmux_desktop/src/persistence.rs
git commit -m "feat: persist Children ordering in session save/load"
```

---

### Task 2: Verify load-time Children integrity

**Files:**
- None (verification only)

- [ ] **Step 1: Build and run the app**

Run: `cargo build && cargo run`

Create 3 tabs (A, B, C) in a single pane. Note their order. Quit the app.

- [ ] **Step 2: Inspect session.ron**

Check `~/Library/Application Support/ai.vmux.desktop/session.ron` for `Children` entries. Verify the entity IDs in the `Children` Vec match the expected tab order.

- [ ] **Step 3: Restart and verify order**

Run the app again. Confirm tabs appear in the same order (A, B, C).

- [ ] **Step 4: Document outcome**

If order is wrong, the `ChildOf` on_insert hook is re-appending after `Children` deserialization. In that case, add a post-load fixup system in `rebuild_session_views` that re-inserts `ChildOf` to trigger hooks in the correct order based on the deserialized `Children`. The re-insertion of `ChildOf` already happens in `rebuild_session_views` for splits, panes, and tabs — confirm this covers the ordering.

---

### Task 3: Implement shared swap helper

**Files:**
- Create: `crates/vmux_desktop/src/layout/swap.rs`
- Modify: `crates/vmux_desktop/src/layout/mod.rs`

- [ ] **Step 1: Create swap.rs with helper functions**

Create `crates/vmux_desktop/src/layout/swap.rs`:

```rust
use bevy::prelude::*;

/// Swap two same-type siblings within a parent's Children.
/// `kind_indices` are the positions within Children of entities matching the filter.
/// `a` and `b` are indices into that filtered list.
pub fn swap_siblings(
    commands: &mut Commands,
    parent: Entity,
    children: &Children,
    kind_positions: &[usize],
    a: usize,
    b: usize,
) {
    if a == b {
        return;
    }
    let Some(&pos_a) = kind_positions.get(a) else { return };
    let Some(&pos_b) = kind_positions.get(b) else { return };

    let entity_a = children[pos_a];
    let entity_b = children[pos_b];

    // Re-insert ChildOf to trigger Children reordering via Bevy hooks.
    // Bevy's relationship system removes from old position and appends,
    // so we detach both and re-insert all children in the swapped order.
    let mut ordered: Vec<Entity> = children.iter().collect();
    ordered.swap(pos_a, pos_b);

    // Detach all children then re-parent in new order
    for &child in &ordered {
        commands.entity(child).remove::<ChildOf>();
    }
    for &child in &ordered {
        commands.entity(child).insert(ChildOf(parent));
    }
}

/// Find the index of `entity` within the filtered positions list.
pub fn find_kind_index(
    entity: Entity,
    children: &Children,
    kind_positions: &[usize],
) -> Option<usize> {
    kind_positions.iter().position(|&pos| children[pos] == entity)
}

/// Resolve prev/next to (active_idx, target_idx) pair.
pub fn resolve_prev(active_idx: usize) -> Option<(usize, usize)> {
    active_idx.checked_sub(1).map(|p| (active_idx, p))
}

pub fn resolve_next(active_idx: usize, len: usize) -> Option<(usize, usize)> {
    (active_idx + 1 < len).then(|| (active_idx, active_idx + 1))
}
```

- [ ] **Step 2: Register swap module**

In `crates/vmux_desktop/src/layout/mod.rs`, add:

```rust
pub(crate) mod swap;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_desktop`

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/layout/swap.rs crates/vmux_desktop/src/layout/mod.rs
git commit -m "feat: add shared swap helper for sibling reordering"
```

---

### Task 4: Implement PaneCommand::SwapPrev / SwapNext

**Files:**
- Modify: `crates/vmux_desktop/src/layout/pane.rs`

- [ ] **Step 1: Add imports**

At the top of `pane.rs`, add:

```rust
use crate::layout::swap::{find_kind_index, resolve_prev, resolve_next, swap_siblings};
```

- [ ] **Step 2: Implement SwapPrev and SwapNext handlers**

Replace the empty match arms:

```rust
PaneCommand::SwapPrev => {}
PaneCommand::SwapNext => {}
```

With:

```rust
PaneCommand::SwapPrev | PaneCommand::SwapNext => {
    let Ok(co) = child_of_q.get(active) else { continue };
    let parent = co.get();
    if !split_dir_q.contains(parent) { continue; }
    let Ok(children) = all_children.get(parent) else { continue };
    let kind_positions: Vec<usize> = children.iter()
        .enumerate()
        .filter(|(_, e)| leaf_panes.contains(e) || split_dir_q.contains(e))
        .map(|(i, _)| i)
        .collect();
    let Some(active_idx) = find_kind_index(active, children, &kind_positions) else { continue };
    let pair = if pane_cmd == PaneCommand::SwapPrev {
        resolve_prev(active_idx)
    } else {
        resolve_next(active_idx, kind_positions.len())
    };
    if let Some((a, b)) = pair {
        swap_siblings(&mut commands, parent, children, &kind_positions, a, b);
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_desktop`

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/layout/pane.rs
git commit -m "feat: implement PaneCommand::SwapPrev/SwapNext"
```

---

### Task 5: Add TabCommand::SwapPrev / SwapNext

**Files:**
- Modify: `crates/vmux_desktop/src/command.rs`
- Modify: `crates/vmux_desktop/src/layout/tab.rs`

- [ ] **Step 1: Add SwapPrev and SwapNext variants to TabCommand**

In `crates/vmux_desktop/src/command.rs`, add to `TabCommand` enum after `MoveToPane`:

```rust
#[menu(id = "tab_swap_prev", label = "Move Tab Left\t<leader> <")]
#[shortcut(chord = "Ctrl+g, <")]
SwapPrev,
#[menu(id = "tab_swap_next", label = "Move Tab Right\t<leader> >")]
#[shortcut(chord = "Ctrl+g, >")]
SwapNext,
```

- [ ] **Step 2: Implement swap in handle_tab_commands**

In `crates/vmux_desktop/src/layout/tab.rs`, add import:

```rust
use crate::layout::swap::{find_kind_index, resolve_prev, resolve_next, swap_siblings};
```

Add match arms in `handle_tab_commands` (after existing arms, before the closing `}`):

```rust
TabCommand::SwapPrev | TabCommand::SwapNext => {
    let Some(pane) = active_pane else { continue };
    let Some(tab) = active_tab else { continue };
    let Ok(children) = pane_children.get(pane) else { continue };
    let kind_positions: Vec<usize> = children.iter()
        .enumerate()
        .filter(|(_, e)| tab_q.contains(e))
        .map(|(i, _)| i)
        .collect();
    let Some(active_idx) = find_kind_index(tab, children, &kind_positions) else { continue };
    let pair = if tab_cmd == TabCommand::SwapPrev {
        resolve_prev(active_idx)
    } else {
        resolve_next(active_idx, kind_positions.len())
    };
    if let Some((a, b)) = pair {
        swap_siblings(&mut commands, pane, children, &kind_positions, a, b);
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_desktop`

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/command.rs crates/vmux_desktop/src/layout/tab.rs
git commit -m "feat: add TabCommand::SwapPrev/SwapNext"
```

---

### Task 6: Add SpaceCommand::SwapPrev / SwapNext

**Files:**
- Modify: `crates/vmux_desktop/src/command.rs`
- Modify: `crates/vmux_desktop/src/layout/space.rs`

- [ ] **Step 1: Add SwapPrev and SwapNext variants to SpaceCommand**

In `crates/vmux_desktop/src/command.rs`, add to `SpaceCommand` enum after `Rename`:

```rust
#[menu(id = "swap_space_prev", label = "Move Space Left")]
SwapPrev,
#[menu(id = "swap_space_next", label = "Move Space Right")]
SwapNext,
```

- [ ] **Step 2: Read handle_space_commands to understand current structure**

Read `crates/vmux_desktop/src/layout/space.rs` to see the system signature and available queries.

- [ ] **Step 3: Implement swap in handle_space_commands**

Add import:

```rust
use crate::layout::swap::{find_kind_index, resolve_prev, resolve_next, swap_siblings};
```

The system will need access to `Children` query and `ChildOf` query. Add params if not present, then add match arms:

```rust
SpaceCommand::SwapPrev | SpaceCommand::SwapNext => {
    // Space's parent is Profile
    let Some(active_space) = active_space else { continue };
    let Ok(co) = child_of_q.get(active_space) else { continue };
    let parent = co.get();
    let Ok(children) = all_children.get(parent) else { continue };
    let kind_positions: Vec<usize> = children.iter()
        .enumerate()
        .filter(|(_, e)| space_q.contains(e))
        .map(|(i, _)| i)
        .collect();
    let Some(active_idx) = find_kind_index(active_space, children, &kind_positions) else { continue };
    let pair = if space_cmd == SpaceCommand::SwapPrev {
        resolve_prev(active_idx)
    } else {
        resolve_next(active_idx, kind_positions.len())
    };
    if let Some((a, b)) = pair {
        swap_siblings(&mut commands, parent, children, &kind_positions, a, b);
    }
}
```

Note: `handle_space_commands` may need additional query params (`Query<&Children>`, `Query<&ChildOf>`, `Query<Entity, With<Space>>`). Add them based on what's already available in the function signature.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p vmux_desktop`

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/command.rs crates/vmux_desktop/src/layout/space.rs
git commit -m "feat: add SpaceCommand::SwapPrev/SwapNext"
```

---

### Task 7: Full build and verification

**Files:**
- None (verification only)

- [ ] **Step 1: Full build**

Run: `cargo build`

- [ ] **Step 2: Manual test — tab swap**

Open the app. Create 3 tabs. Use `Ctrl+g, <` and `Ctrl+g, >` to swap tab positions. Verify the active tab follows the swap (stays selected but moves position).

- [ ] **Step 3: Manual test — pane swap**

Split vertically (`Ctrl+g, %`). Use `Ctrl+g, {` and `Ctrl+g, }` to swap pane positions. Verify panes swap visually.

- [ ] **Step 4: Manual test — persistence**

Arrange tabs and panes in a specific order. Quit the app. Restart. Verify the order is preserved.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "feat: persist sibling order and swap commands"
```
