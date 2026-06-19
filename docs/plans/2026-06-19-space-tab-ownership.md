# Space-owned Tab Layout — Implementation Plan

> **For agentic workers:** This plan is executed **inline** (superpowers:executing-plans), NOT subagent-driven. Reason: vmux CEF builds are huge and long-running; fresh subagents drop sockets mid-build (see project memory). Implement directly in the warm worktree `.worktrees/space-tab-ownership`. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make space membership structural (each `Space` entity owns its tabs as children) so stack/tab commands, agent layout, and reconcile can never select or despawn across spaces, and unify "selected" state on a runtime `Active` marker.

**Architecture:** The `Space` entity becomes the per-space render container under `Main`; tabs are `ChildOf(Space)`. A generic `Active` marker (vmux_core, never persisted) marks the selected child at each level (Space/Tab/pane-branch/Stack), with `ensure_active` safety-net systems seeded by `LastActivatedAt`. Agent/reconcile scope to a space subtree resolved from the request anchor. No store migration: a `store.version` sidecar triggers a hard reset on schema change.

**Tech stack:** Rust, Bevy 0.19-rc ECS, moonshine_save, bevy_cef. Crates touched: `vmux_core`, `vmux_layout`, `vmux_space`, `vmux_desktop`, `vmux_setting`.

**Spec:** `docs/specs/2026-06-19-space-tab-ownership-design.md`

**Conventions (from AGENTS.md):** no code comments; no `mod.rs` (use `foo.rs` + `foo/`); chain consecutive `App` builder calls in one expression; prefer system+message integration in tests (register systems, send messages, run schedules, assert ECS state); `#[cfg(...)]`-gate platform APIs.

**Build note:** The first `cargo test -p <crate>` in this fresh worktree triggers a full CEF build (long). Subsequent runs are incremental. Do not share `CARGO_TARGET_DIR` with other worktrees.

---

## File structure

- `crates/vmux_core/src/lib.rs` — add `Active` marker component + registration.
- `crates/vmux_layout/src/space.rs` — `space_of` helper; `space_container_bundle`; switch active-space tracking from `ActiveSpaceTag` to `Active`; delete degrade-to-global helpers (final task).
- `crates/vmux_layout/src/active.rs` (new) — `ensure_active_*` safety-net systems + `walk_active_*` focused-path helpers; registered by a small `ActivePlugin` or folded into `SpacePlugin`.
- `crates/vmux_layout/src/tab.rs` — visibility scoped to active space's tab children; command paths use active-space focused path; drop tab `SpaceId`.
- `crates/vmux_layout/src/stack.rs` — `handle_stack_commands` uses `Active`-walk focused path, not global `active_among`.
- `crates/vmux_layout/src/window.rs` — `spawn_requested_tab_layouts` parents new tab under the target `Space` container; drop tab `SpaceId`.
- `crates/vmux_layout/src/reconcile.rs` — snapshot/`collect_existing_ids` scoped to a space subtree from the anchor.
- `crates/vmux_space/src/plugin.rs` — `on_space_command` spawns/uses Space containers; `space_rows_from_world` counts tab children; replace `ActiveSpaceTag`.
- `crates/vmux_desktop/src/persistence.rs` — allowlist (drop `ActiveSpaceTag`); rebuild `Space→Main` link; `store.version` schema guard + hard reset.
- `crates/vmux_setting/src/plugin/runtime.rs` — tolerant `resolve_startup_*` (canonical-key fallback).

---

## Task 1: `Active` marker component (vmux_core)

**Files:**
- Modify: `crates/vmux_core/src/lib.rs`
- Test: same file (`#[cfg(test)]`).

- [ ] **Step 1: Write the failing test**

In `crates/vmux_core/src/lib.rs` tests module:

```rust
#[test]
fn active_marker_is_registered_and_reflectable() {
    let mut app = App::new();
    app.add_plugins(CorePlugin);
    let registry = app.world().resource::<AppTypeRegistry>().read();
    assert!(registry.get(std::any::TypeId::of::<Active>()).is_some());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_core active_marker_is_registered -- --nocapture`
Expected: FAIL (`Active` undefined).

- [ ] **Step 3: Implement**

Add near the other marker components in `crates/vmux_core/src/lib.rs`:

```rust
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_core"]
pub struct Active;
```

In `CorePlugin::build`, chain the registration with the existing `register_type` calls:

```rust
app.register_type::<Active>();
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_core active_marker_is_registered`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/lib.rs
git commit -m "feat(core): add generic Active marker component"
```

---

## Task 2: `space_of` helper + space container bundle (vmux_layout)

**Files:**
- Modify: `crates/vmux_layout/src/space.rs`
- Test: same file.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn space_of_walks_up_to_nearest_space() {
    let mut app = App::new();
    let space = app.world_mut().spawn((Space, SpaceId("s".into()))).id();
    let tab = app.world_mut().spawn((crate::tab::Tab::default(), ChildOf(space))).id();
    let stack = app.world_mut().spawn(ChildOf(tab)).id();
    let found = app.world_mut().run_system_once(move |child_of: Query<&ChildOf>, spaces: Query<(), With<Space>>| {
        space_of(stack, &child_of, &spaces)
    }).unwrap();
    assert_eq!(found, Some(space));
}

#[test]
fn space_container_bundle_is_absolute_fill_node() {
    let bundle_node = space_container_node();
    assert_eq!(bundle_node.position_type, PositionType::Absolute);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_layout space_of_walks_up`
Expected: FAIL (`space_of` / `space_container_node` undefined).

- [ ] **Step 3: Implement**

Add to `crates/vmux_layout/src/space.rs`:

```rust
pub fn space_of(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    spaces: &Query<(), With<Space>>,
) -> Option<Entity> {
    let mut cur = entity;
    loop {
        if spaces.get(cur).is_ok() {
            return Some(cur);
        }
        match child_of.get(cur) {
            Ok(co) => cur = co.parent(),
            Err(_) => return None,
        }
    }
}

pub fn space_container_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        top: Val::Px(0.0),
        bottom: Val::Px(0.0),
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        ..default()
    }
}

pub fn space_view_bundle() -> impl Bundle {
    (
        space_container_node(),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::default(),
    )
}
```

Add `use bevy::ecs::system::RunSystemOnce;` to the test module if not present.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_layout space_of_walks_up space_container_bundle`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/space.rs
git commit -m "feat(layout): add space_of helper and space view container bundle"
```

---

## Task 3: `ensure_active` safety-net systems (vmux_layout/active.rs)

**Files:**
- Create: `crates/vmux_layout/src/active.rs`
- Modify: `crates/vmux_layout/src/lib.rs` (add `mod active;` + register in a plugin)
- Test: `crates/vmux_layout/src/active.rs`

**Design:** one system per level. Each finds parents whose selectable children lack an `Active`, and marks the max-`LastActivatedAt` child. Selectable kinds: `Space` (under `Main`), `Tab` (under `Space`), pane-branch (`Pane`/`PaneSplit` under a `PaneSplit`), `Stack` (under a leaf `Pane`).

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tab::Tab;
    use vmux_core::Active;
    use vmux_history::LastActivatedAt;

    #[test]
    fn ensure_active_tab_marks_max_last_activated_child() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, ensure_active_tab);
        let space = app.world_mut().spawn(crate::space::Space).id();
        let _old = app.world_mut().spawn((Tab::default(), LastActivatedAt(1), ChildOf(space))).id();
        let newer = app.world_mut().spawn((Tab::default(), LastActivatedAt(5), ChildOf(space))).id();
        app.update();
        assert!(app.world().entity(newer).contains::<Active>());
        let active_count = app.world_mut()
            .query_filtered::<Entity, (With<Tab>, With<Active>)>()
            .iter(app.world()).count();
        assert_eq!(active_count, 1);
    }

    #[test]
    fn ensure_active_tab_is_noop_when_one_already_active() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, ensure_active_tab);
        let space = app.world_mut().spawn(crate::space::Space).id();
        let a = app.world_mut().spawn((Tab::default(), LastActivatedAt(1), Active, ChildOf(space))).id();
        let _b = app.world_mut().spawn((Tab::default(), LastActivatedAt(5), ChildOf(space))).id();
        app.update();
        assert!(app.world().entity(a).contains::<Active>());
        let active_count = app.world_mut()
            .query_filtered::<Entity, (With<Tab>, With<Active>)>()
            .iter(app.world()).count();
        assert_eq!(active_count, 1);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_layout ensure_active_tab`
Expected: FAIL (module/system undefined).

- [ ] **Step 3: Implement `crates/vmux_layout/src/active.rs`**

```rust
use bevy::prelude::*;
use vmux_core::Active;
use vmux_history::LastActivatedAt;

use crate::pane::{Pane, PaneSplit};
use crate::space::Space;
use crate::stack::Stack;
use crate::tab::Tab;

fn pick_active(
    candidates: impl IntoIterator<Item = (Entity, i64)>,
) -> Option<Entity> {
    candidates.into_iter().max_by_key(|(_, ts)| *ts).map(|(e, _)| e)
}

pub fn ensure_active_tab(
    spaces: Query<&Children, With<Space>>,
    tabs: Query<(Entity, &LastActivatedAt, Has<Active>), With<Tab>>,
    mut commands: Commands,
) {
    for children in &spaces {
        let mut tab_children = Vec::new();
        let mut has_active = false;
        for child in children.iter() {
            if let Ok((e, ts, active)) = tabs.get(child) {
                tab_children.push((e, ts.0));
                has_active |= active;
            }
        }
        if has_active || tab_children.is_empty() {
            continue;
        }
        if let Some(target) = pick_active(tab_children) {
            commands.entity(target).insert(Active);
        }
    }
}

pub fn ensure_active_stack(
    leaves: Query<&Children, (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<(Entity, &LastActivatedAt, Has<Active>), With<Stack>>,
    mut commands: Commands,
) {
    for children in &leaves {
        let mut stack_children = Vec::new();
        let mut has_active = false;
        for child in children.iter() {
            if let Ok((e, ts, active)) = stacks.get(child) {
                stack_children.push((e, ts.0));
                has_active |= active;
            }
        }
        if has_active || stack_children.is_empty() {
            continue;
        }
        if let Some(target) = pick_active(stack_children) {
            commands.entity(target).insert(Active);
        }
    }
}

pub fn ensure_active_branch(
    splits: Query<&Children, With<PaneSplit>>,
    branches: Query<(Entity, Option<&LastActivatedAt>, Has<Active>), With<Pane>>,
    mut commands: Commands,
) {
    for children in &splits {
        let mut branch_children = Vec::new();
        let mut has_active = false;
        for child in children.iter() {
            if let Ok((e, ts, active)) = branches.get(child) {
                branch_children.push((e, ts.map(|t| t.0).unwrap_or(0)));
                has_active |= active;
            }
        }
        if has_active || branch_children.is_empty() {
            continue;
        }
        if let Some(target) = pick_active(branch_children) {
            commands.entity(target).insert(Active);
        }
    }
}
```

(Active-space seeding lives in `space.rs`, Task 4, because spaces need `LastActivatedAt` and a `Main`-children scope.)

In `crates/vmux_layout/src/lib.rs` add `mod active;` (filename module) and register the systems in `SpacePlugin` (Task 4) or a new `ActivePlugin`. For this task, register in the layout plugin that already runs per-frame:

```rust
app.add_systems(Update, (active::ensure_active_tab, active::ensure_active_stack, active::ensure_active_branch));
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_layout ensure_active`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/active.rs crates/vmux_layout/src/lib.rs
git commit -m "feat(layout): add ensure_active safety-net systems for tab/stack/branch"
```

---

## Task 4: Active-space tracking via `Active` (replace `ActiveSpaceTag`)

**Files:**
- Modify: `crates/vmux_layout/src/space.rs` (systems + delete `ActiveSpaceTag`)
- Modify: `crates/vmux_layout/src/active.rs` (`ensure_active_space`)
- Modify: `crates/vmux_space/src/plugin.rs` (queries + deactivate/new/attach/delete)
- Modify: `crates/vmux_desktop/src/persistence.rs:13,164` (imports + allowlist)
- Modify: `crates/vmux_agent/src/plugin.rs:897` (`Has<ActiveSpaceTag>` → `Has<Active>` qualified to Space)
- Test: `space.rs`, `active.rs`

**Key idea:** "active space" = the entity `With<Space>` that also has `Active`. The `ActiveSpaceEntity`/`ActiveSpaceId` resources keep working; their source systems change their query filter.

- [ ] **Step 1: Write the failing tests**

In `space.rs` tests, update/add:

```rust
#[test]
fn active_space_entity_tracks_active_marked_space() {
    let mut app = App::new();
    app.init_resource::<ActiveSpaceEntity>()
        .add_systems(Update, sync_active_space_entity);
    let space = app.world_mut().spawn((Space, SpaceId("default".into()), vmux_core::Active)).id();
    app.update();
    assert_eq!(app.world().resource::<ActiveSpaceEntity>().0, Some(space));
}
```

In `active.rs` tests:

```rust
#[test]
fn ensure_active_space_marks_max_last_activated_space() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_systems(Update, ensure_active_space);
    let main = app.world_mut().spawn(crate::window::Main).id();
    let _a = app.world_mut().spawn((Space, LastActivatedAt(1), ChildOf(main))).id();
    let b = app.world_mut().spawn((Space, LastActivatedAt(9), ChildOf(main))).id();
    app.update();
    assert!(app.world().entity(b).contains::<Active>());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_layout active_space_entity_tracks_active_marked`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `space.rs`:
- Delete the `ActiveSpaceTag` struct (lines ~38-42) and its `register_type::<ActiveSpaceTag>()`.
- `sync_active_space_entity` (line 50): change query to `tagged: Query<Entity, (With<Space>, With<vmux_core::Active>)>`.
- `ensure_active_space_tagged` (line 60): rename to `ensure_active_space` semantics is moved to `active.rs` (next bullet); remove the old fn here.
- Delete `assign_orphan_tabs_to_active_space`, `same_space`, `in_active_space` now (or in Task 9 if other consumers still reference them — prefer deleting `assign_orphan_tabs_to_active_space` now since tabs no longer carry `SpaceId` after Task 5; keep `same_space`/`in_active_space` until Task 9 to keep consumers compiling). For this task, only remove `ActiveSpaceTag` + `assign_orphan_tabs_to_active_space` registration/usage if no longer referenced; otherwise defer the helper deletions to Task 9.
- Update the `SpacePlugin::build` system tuple accordingly (chain calls).

In `active.rs` add:

```rust
pub fn ensure_active_space(
    main: Query<&Children, With<crate::window::Main>>,
    spaces: Query<(Entity, Option<&LastActivatedAt>, Has<Active>), With<Space>>,
    mut commands: Commands,
) {
    for children in &main {
        let mut space_children = Vec::new();
        let mut has_active = false;
        for child in children.iter() {
            if let Ok((e, ts, active)) = spaces.get(child) {
                space_children.push((e, ts.map(|t| t.0).unwrap_or(0)));
                has_active |= active;
            }
        }
        if has_active || space_children.is_empty() {
            continue;
        }
        if let Some(target) = pick_active(space_children) {
            commands.entity(target).insert(Active);
        }
    }
}
```

In `vmux_space/src/plugin.rs`:
- `SpaceQuery`/`SpaceListQuery` (lines 162, 275): replace `Has<vmux_layout::space::ActiveSpaceTag>` with `Has<vmux_core::Active>`.
- `sync_active_space_record` (line 145): `With<vmux_layout::space::ActiveSpaceTag>` → `(With<vmux_layout::space::Space>, With<vmux_core::Active>)`.
- `deactivate_all_spaces` (line 304): `commands.entity(entity).remove::<vmux_core::Active>();`
- `on_space_command` "new"/"attach"/"delete" (lines 458, 480, 506): insert/remove `vmux_core::Active` instead of `ActiveSpaceTag`; also stamp `LastActivatedAt::now()` on the newly-active Space (needed for restore seed).

In `persistence.rs`: remove `ActiveSpaceTag` from imports (line 13) and from the save allowlist (line 164).

In `vmux_agent/src/plugin.rs:897`: `Has<vmux_layout::space::ActiveSpaceTag>` → `Has<vmux_core::Active>`.

Register `ensure_active_space` in `SpacePlugin`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_layout -p vmux_space active_space`
Expected: PASS. Then `cargo build -p vmux_desktop` to confirm cross-crate compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/space.rs crates/vmux_layout/src/active.rs crates/vmux_space/src/plugin.rs crates/vmux_desktop/src/persistence.rs crates/vmux_agent/src/plugin.rs
git commit -m "refactor(space): track active space via Active marker, drop ActiveSpaceTag"
```

---

## Task 5: Tabs owned by Space container; Space under Main

**Files:**
- Modify: `crates/vmux_layout/src/window.rs:469-498` (`spawn_requested_tab_layouts`)
- Modify: `crates/vmux_space/src/plugin.rs:484-519` (`on_space_command` "new") and `handle_open_in_new_space`
- Test: `window.rs`, `vmux_space/plugin.rs`

**Key idea:** `TabLayoutSpawnRequest` parents the new tab under the **active Space container** (not `Main`); the new tab no longer gets a `SpaceId`. A new space spawns a container (`Space` + `space_view_bundle()` + `ChildOf(Main)` + `Active` + `LastActivatedAt`) and the tab request resolves to it.

- [ ] **Step 1: Write the failing test**

In `window.rs` tests:

```rust
#[test]
fn spawned_tab_is_child_of_active_space_container() {
    let mut app = build_app();
    let window = app.world_mut().spawn(PrimaryWindow).id();
    let main = app.world_mut().spawn(Main).id();
    let space = app.world_mut().spawn((crate::space::Space, vmux_core::Active, ChildOf(main))).id();
    app.world_mut().resource_mut::<Messages<crate::TabLayoutSpawnRequest>>().write(crate::TabLayoutSpawnRequest {
        main,
        primary_window: window,
        name: None,
        content: crate::TabLayoutSpawnContent::StartupUrlOrPrompt,
        clear_pending_stack: false,
        focus: true,
    });
    app.update();
    let tab = app.world_mut().query_filtered::<Entity, With<Tab>>().iter(app.world()).next().unwrap();
    let parent = app.world().get::<ChildOf>(tab).unwrap().parent();
    assert_eq!(parent, space);
    assert!(app.world().get::<crate::space::SpaceId>(tab).is_none());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_layout spawned_tab_is_child_of_active_space`
Expected: FAIL (tab parented to `main`, or `SpaceId` present).

- [ ] **Step 3: Implement**

`spawn_requested_tab_layouts` (window.rs:469): add params `spaces: Query<Entity, (With<crate::space::Space>, With<vmux_core::Active>)>` (and a fallback to `request.main`). Replace the tab spawn parent + drop SpaceId block (lines 480-498):

```rust
let parent = spaces.iter().next().unwrap_or(request.main);
let tab_e = commands
    .spawn((tab_bundle(), LastActivatedAt::now(), CreatedAt::now(), Active, ChildOf(parent)))
    .id();
if let Some(name) = request.name.clone() {
    commands.entity(tab_e).insert(Tab { name });
}
```

(Remove the `active_space_id` → `SpaceId` insertion entirely.)

`on_space_command` "new" (plugin.rs:500-518): spawn the container with view layer + `Active` + `LastActivatedAt`, and ensure the `TabLayoutSpawnRequest` resolves to it (the request still passes `main`; the spawn system now finds the `Active` Space):

```rust
commands.entity(/* deactivated previous via deactivate_all_spaces */);
commands.spawn((
    vmux_layout::space::Space,
    vmux_layout::space::SpaceId(id.clone()),
    Name::new(id.clone()),
    vmux_core::Order(order),
    vmux_core::Active,
    vmux_history::LastActivatedAt::now(),
    vmux_layout::space::space_view_bundle(),
    ChildOf(main),
));
```

Apply the same container spawn in `handle_open_in_new_space` (plugin.rs:554-561).

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_layout spawned_tab_is_child_of_active_space`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/window.rs crates/vmux_space/src/plugin.rs
git commit -m "feat(layout): parent tabs under their Space container, spaces under Main"
```

---

## Task 6: Two-level visibility

**Files:**
- Modify: `crates/vmux_layout/src/space.rs` (new `sync_space_container_visibility`)
- Modify: `crates/vmux_layout/src/tab.rs:260-298` (`sync_tab_visibility`)
- Test: `space.rs`, `tab.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn inactive_space_container_is_hidden_but_alive() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_systems(Update, sync_space_container_visibility);
    let main = app.world_mut().spawn(crate::window::Main).id();
    let active = app.world_mut().spawn((Space, vmux_core::Active, space_container_node(), ChildOf(main))).id();
    let bg = app.world_mut().spawn((Space, space_container_node(), ChildOf(main))).id();
    app.update();
    assert_eq!(app.world().get::<Node>(active).unwrap().display, Display::Flex);
    assert_eq!(app.world().get::<Node>(bg).unwrap().display, Display::None);
    assert!(app.world().get_entity(bg).is_ok());
}
```

`sync_tab_visibility` test: active tab is the `Active` tab child of the active space; tabs in a background space are `Display::None`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_layout inactive_space_container_is_hidden`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `space.rs`:

```rust
pub fn sync_space_container_visibility(
    mut spaces: Query<(&mut Node, &mut Visibility, Has<vmux_core::Active>), With<Space>>,
) {
    for (mut node, mut vis, active) in &mut spaces {
        let display = if active { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        let target = if active { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != target {
            *vis = target;
        }
    }
}
```

Rewrite `sync_tab_visibility` (tab.rs:260): query tabs with `Has<Active>` and their parent; show the tab that is `Active` AND whose parent Space is `Active`; everything else `Display::None`. Use the parent's `Active` (space) + tab's `Active`. Drop `ActiveSpaceId`/`in_active_space` usage.

Register `sync_space_container_visibility` in `SpacePlugin` (PostUpdate before `UiSystems::Layout`, mirroring `sync_tab_visibility`).

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_layout inactive_space_container_is_hidden tab_visibility`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/space.rs crates/vmux_layout/src/tab.rs
git commit -m "feat(layout): two-level visibility (active space container then active tab)"
```

---

## Task 7: Focused path via Active walk (fix the cross-space command bug)

**Files:**
- Modify: `crates/vmux_layout/src/stack.rs:80-160,214` (focused path)
- Modify: `crates/vmux_layout/src/tab.rs:90,138-222,348` (command active tab)
- Test: `stack.rs`, `tab.rs`

**Key idea:** focused tab/pane/stack = walk `Active` from the active Space. Delete the global `active_among(tabs.iter())` path used by command handlers.

- [ ] **Step 1: Write the failing (regression) test**

In `stack.rs` tests:

```rust
#[test]
fn stack_close_does_not_touch_background_space() {
    // space A (background) with a stack; space B (active) with a stack.
    // Dispatch Layout(Stack(Close)). Only B's stack is affected; A intact.
    // Build with handle_stack_commands + Active markers; assert A's stack still exists.
}
```

(Write the full App setup: spawn `Main`, two `Space` containers — B with `Active` — each `Tab(Active)→Split→Pane(Active)→Stack(Active)`; B's stack also `Active`; send `AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close))`; assert A's stack entity still exists and B's was closed/replaced.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_layout stack_close_does_not_touch_background_space`
Expected: FAIL (global selection closes A's stack when A is globally most-recent).

- [ ] **Step 3: Implement**

Add to `active.rs` a focused-path resolver:

```rust
pub fn focused_path(
    spaces: &Query<(Entity, Has<Active>), With<Space>>,
    children: &Query<&Children>,
    tabs: &Query<(), With<Tab>>,
    tab_active: &Query<(), (With<Tab>, With<Active>)>,
    branch_active: &Query<(), (With<Pane>, With<Active>)>,
    leaves: &Query<(), (With<Pane>, Without<PaneSplit>)>,
    stack_active: &Query<(), (With<Stack>, With<Active>)>,
) -> (Option<Entity>, Option<Entity>, Option<Entity>) { /* walk Active down */ }
```

Simplest concrete implementation: find the active `Space`; its `Active` `Tab` child; descend through `Active` `Pane`/`PaneSplit` children to the `Active` leaf `Pane`; its `Active` `Stack`. Return `(tab, leaf_pane, stack)`.

In `stack.rs` `handle_stack_commands` (line 214): replace the `focused_stack(...)` call with `focused_path(...)` (active-space-scoped). Delete `fn focused_stack` (line 126) and the global `active_among`-based `compute_focused_stack` selection of cross-space tabs — keep `FocusedStack` resource but have `compute_focused_stack` set it from `focused_path` too.

In `tab.rs`: `handle_tab_commands` (line 90) and `on_tabs_command_emit` (line 348): compute `active_tab` as the `Active` tab child of the active Space (not global max). `active_tab_siblings` already filters siblings by parent; since tabs are now children of one Space, siblings are same-space by construction — drop the `tab_space`/`same_space` filter param (defer signature cleanup to Task 9 if other callers exist).

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_layout stack_close_does_not_touch_background_space active_tab`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/stack.rs crates/vmux_layout/src/tab.rs crates/vmux_layout/src/active.rs
git commit -m "fix(layout): resolve focused tab/pane/stack within active space, not globally"
```

---

## Task 8: Reconcile + agent layout scoped to a space subtree

**Files:**
- Modify: `crates/vmux_layout/src/reconcile.rs:325-383` (`serve_snapshot_requests`)
- Modify: `crates/vmux_layout/src/reconcile.rs:690-736` (`collect_existing_ids`, `active_space_id`)
- Test: `reconcile.rs`

**Key idea:** target space = `space_of(self_stack)` when the request has an anchor (agent), else the active Space. Snapshot retains only that space's tabs; `collect_existing_ids` collects only that space's subtree.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn apply_with_anchor_in_space_a_leaves_space_b_intact() {
    // Build two space subtrees A (with anchored stack) and B (active).
    // apply() a snapshot derived from A's anchor; assert B's stack entities survive.
}

#[test]
fn collect_existing_ids_scoped_to_target_space_subtree() {
    // Active space B; collect_existing_ids returns only B's ids, never A's.
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_layout apply_with_anchor_in_space_a collect_existing_ids_scoped`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `serve_snapshot_requests`: compute `target_space = self_stack.and_then(|s| space_of(s, ...)).or(active_space_entity)`. Replace the `snapshot.tabs.retain(... SpaceId == active ...)` (lines 361-374) with retain where the tab's owning space (`space_of(tab_entity)`) equals `target_space`.

In `collect_existing_ids` (line 722): take a `target: Option<Entity>` (the space). Replace the `in_active_space` filter with: include tabs whose `space_of(tab) == target` (or all tabs when `target` is `None`). `apply`/`apply_with_existing` already gate closes to `existing`, so scoping `existing` to the target subtree is sufficient.

`apply` needs the target space; thread it from the request anchor where available (the apply path runs via a world closure — resolve the active Space when no anchor is carried).

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_layout apply_with_anchor collect_existing_ids_scoped`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/reconcile.rs
git commit -m "fix(layout): scope snapshot/reconcile to the anchor's space subtree"
```

---

## Task 9: Remove degrade-to-global and tab `SpaceId`

**Files:**
- Modify: `crates/vmux_layout/src/space.rs` (delete `same_space`, `in_active_space`; keep `SpaceId` type for Space identity)
- Modify: `crates/vmux_layout/src/tab.rs` (drop `tab_space`/`SpaceId` params from `active_tab_siblings`, `sync_tab_order`)
- Modify: `crates/vmux_space/src/plugin.rs` (`space_rows_from_world` tab_count via children; rename/delete retag tab `SpaceId`)
- Modify: `crates/vmux_desktop/src/persistence.rs` (save allowlist: `SpaceId` retained — now only present on Space)
- Test: affected files

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn space_row_tab_count_counts_space_children() {
    // Space with 2 Tab children -> tab_count == 2; tabs carry no SpaceId.
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_space space_row_tab_count_counts_space_children`
Expected: FAIL (count still keyed by `SpaceId`).

- [ ] **Step 3: Implement**

- `space_rows_from_world` (plugin.rs:168): replace `tab_spaces` `SpaceId` matching with counting each Space's `Tab` children via `Children` + `With<Tab>`.
- `on_space_command` "rename"/"delete": operate on the Space's tab children (despawn descendants) instead of matching tab `SpaceId`; drop tab `SpaceId` retag in "rename".
- `tab.rs`: remove `tab_space: Query<&SpaceId>` params and `same_space` filtering from `active_tab_siblings` (line 228) and `sync_tab_order` (line 300) — siblings are same-space by parent.
- `space.rs`: delete `same_space` and `in_active_space`. `SpaceId` stays (Space identity).
- Grep `crate::space::SpaceId` / `vmux_layout::space::SpaceId` for remaining tab-context uses and remove (reconcile.rs:507 `materialize` new-tab `SpaceId` insert — remove; new tabs derive space from parent).

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_layout -p vmux_space`; then `cargo build -p vmux_desktop`.
Expected: PASS / clean build.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(layout): remove degrade-to-global SpaceId label from tabs"
```

---

## Task 10: Persistence — Space↔Main rebuild + schema-version hard reset

**Files:**
- Modify: `crates/vmux_desktop/src/persistence.rs` (`rebuild_space_views`, `load_space_on_startup`, save path)
- Test: `persistence.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn store_version_mismatch_triggers_reset() {
    // Write a store.ron + store.version with an old number; load guard deletes store.ron.
}

#[test]
fn space_container_relinked_to_main_on_rebuild() {
    // Loaded Space (no ChildOf) gets ChildOf(Main) + container Node after rebuild.
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_desktop store_version_mismatch space_container_relinked`
Expected: FAIL.

- [ ] **Step 3: Implement**

- Add `const STORE_SCHEMA_VERSION: u32 = 2;` and `fn store_version_path()` (sibling `store.version`).
- In `save_space_to_path`: after triggering save, write `STORE_SCHEMA_VERSION` to `store.version`.
- In `load_space_on_startup` (line 183): before `trigger_load`, read `store.version`; if missing or `< STORE_SCHEMA_VERSION`, delete `store.ron` + `store.version`, set `SpaceFilePresent(false)`, spawn the default space, and skip load. (Compose with existing `remove_stale_space_if_needed`.)
- In `rebuild_space_views` (line 279): add a query for `Space` `Without<Node>`; for each, insert `space_view_bundle()` + `ChildOf(main)`; ensure tabs already `ChildOf(Space)` round-trip (they do — both saved). Fix the `Space→Main` link the same way tabs→main is fixed today.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_desktop store_version space_container_relinked`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/persistence.rs
git commit -m "feat(persistence): relink space containers to Main and add store schema reset guard"
```

---

## Task 11: Startup-dir slug fix (tolerant resolve)

**Files:**
- Modify: `crates/vmux_setting/src/plugin/runtime.rs:77-114`
- Test: `runtime.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn resolve_startup_dir_matches_canonical_when_key_slug_differs() {
    let mut settings = /* AppSettings with spaces["mistralai-dashboard"].startup_dir = existing temp dir */;
    let dir = resolve_startup_dir(&settings, "mistralai/dashboard");
    assert_eq!(dir, /* the temp dir */);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_setting resolve_startup_dir_matches_canonical`
Expected: FAIL (exact-match misses the dash key).

- [ ] **Step 3: Implement**

Add a private `fn space_override<'a>(settings: &'a AppSettings, space_id: &str) -> Option<&'a SpaceOverrides>` that tries exact `settings.spaces.get(space_id)`, then falls back to a normalized match where `/` and `-` are treated equivalently (`normalize(k) == normalize(space_id)`). Use it in both `resolve_startup_url` and `resolve_startup_dir`.

```rust
fn normalize_space_key(s: &str) -> String {
    s.chars().map(|c| if c == '/' { '-' } else { c }).collect::<String>().to_lowercase()
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p vmux_setting resolve_startup_dir_matches_canonical resolve_startup`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_setting/src/plugin/runtime.rs
git commit -m "fix(setting): tolerate slug-variant per-space override keys in startup resolution"
```

---

## Task 12: Full build, lint, and runtime verification

**Files:** none (verification only).

- [ ] **Step 1: Workspace checks**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: clean. Fix any failures before proceeding.

- [ ] **Step 2: Build + launch the app**

Build and run the desktop app from this worktree (warm target). Confirm boot with the migrated/empty store (schema reset expected on first launch).

- [ ] **Step 3: Reproduce the original failure flow**

- Open a Vibe agent in space A; ask it to `create_space` "mistralai/dashboard" and set the per-space `startup_dir` to the repo path.
- Verify: new space becomes active; A's pages/agent stay live (hidden) and are NOT despawned; switching back to A shows A's tabs intact.
- Close tabs in the active space repeatedly: verify no terminal-respawn storm and no tabs from the other space appear.
- Verify the new space's startup honors `startup_dir` (opens in the repo).
- With the Vibe agent still in A (background), confirm its `read_layout`/`update_layout` act on A, not the active space.

- [ ] **Step 4: Observable-behavior check**

Confirm via the layout the frontend renders and the snapshot the agent receives (not just ECS state) — the user always runtime-tests; match that.

- [ ] **Step 5: Commit any fixes; open PR when green**

```bash
git add -A && git commit -m "test: verify space-owned tab layout end to end"
```

---

## Self-review notes

- **Spec coverage:** §1 ownership → T2,T5,T9; §Active selection → T1,T3,T4,T7; §switch+visibility → T6; §command scoping → T7; §agent/reconcile → T8; §persistence+reset → T10; §slug fix → T11; §testing → per-task + T12. All covered.
- **Sequencing:** `SpaceId` type survives until T9 so intermediate tasks compile; `same_space`/`in_active_space` deleted only in T9 after all consumers move to hierarchy. Each task keeps the build green and its unit tests green; full integration verified in T12.
- **Type consistency:** `Active` (vmux_core) used uniformly; `space_of`, `space_view_bundle`, `space_container_node`, `focused_path`, `ensure_active_{space,tab,stack,branch}`, `normalize_space_key`, `STORE_SCHEMA_VERSION` referenced consistently across tasks.
