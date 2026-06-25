# Reopen Closed Tab — Full Position Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reopen (`cmd+shift+t`) restores a closed page to its original space → tab → pane → stack position, reconstructing collapsed splits, robust across `store.ron` save/load.

**Architecture:** Add one persisted `PaneId` (UUID) to every pane, auto-assigned by a system. At close, archive a sibling `ArchivedPagePosition` component capturing the leaf `PaneId`, stack index, and the root→leaf split path (ids/axes/child-indices/flex weights). On reopen, resolve the position through a fallback ladder: live leaf pane → reconstruct split path → recreate tab by index → fallback space.

**Tech Stack:** Rust, Bevy 0.19-rc.2 ECS, moonshine_save (RON `store.ron`), uuid v4.

**Spec:** `docs/specs/2026-06-24-reopen-closed-tab-position-design.md`

**Working dir:** worktree `.worktrees/reopen-position` (branch `fix/reopen-position`). All paths below are repo-relative; run all commands from the worktree root.

---

## File Structure

- `crates/vmux_layout/src/pane.rs` — new `PaneId` component, `assign_pane_ids` system, registration.
- `crates/vmux_core/src/archive.rs` — new `ArchivedPagePosition`, `PaneStep`, `SplitAxis`; extend `PageArchiveRequest`.
- `crates/vmux_core/src/lib.rs` — register new reflect types in `CorePlugin`.
- `crates/vmux_layout/src/archive.rs` — capture full position at close; reopen ladder + reconstruction helper.
- `crates/vmux_desktop/src/persistence.rs` — allowlist `PaneId` + `ArchivedPagePosition`; round-trip test.

---

## Task 1: `PaneId` component + `assign_pane_ids` system

**Files:**
- Modify: `crates/vmux_layout/src/pane.rs` (plugin build ~`53`, add component + system)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `crates/vmux_layout/src/pane.rs`:

```rust
#[test]
fn assign_pane_ids_fills_missing_and_keeps_existing() {
    let mut app = App::new();
    app.add_systems(Update, super::assign_pane_ids);
    let bare = app.world_mut().spawn(super::Pane).id();
    let kept = app
        .world_mut()
        .spawn((super::Pane, super::PaneId("fixed".to_string())))
        .id();
    app.update();
    let assigned = app.world().get::<super::PaneId>(bare).expect("id assigned");
    assert!(!assigned.0.is_empty());
    assert_eq!(app.world().get::<super::PaneId>(kept).unwrap().0, "fixed");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout assign_pane_ids_fills_missing_and_keeps_existing 2>&1 | tail -20`
Expected: FAIL — `PaneId` / `assign_pane_ids` not found.

- [ ] **Step 3: Add the component and system**

In `crates/vmux_layout/src/pane.rs`, add near the `Pane` definition (after `pub struct Pane;`, ~line 155):

```rust
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct PaneId(pub String);

pub fn assign_pane_ids(
    panes: Query<Entity, (With<Pane>, Without<PaneId>)>,
    mut commands: Commands,
) {
    for entity in &panes {
        commands
            .entity(entity)
            .insert(PaneId(uuid::Uuid::new_v4().to_string()));
    }
}
```

Confirm `use moonshine_save::prelude::*;` (for `Save`) is already imported at the top of `pane.rs`; it is used by `Pane`. `uuid` is already a dependency of `vmux_layout`.

- [ ] **Step 4: Register the type and the system**

In `impl Plugin for PanePlugin` (`pane.rs:52`), add `.register_type::<PaneId>()` next to the other `register_type` calls, and add the system to an existing `Update` set:

```rust
app.register_type::<Pane>()
    .register_type::<PaneId>()
    .register_type::<PaneSplit>()
    // … unchanged …
    .add_systems(Update, stamp_spawn_seq)
    .add_systems(Update, assign_pane_ids)
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vmux_layout assign_pane_ids_fills_missing_and_keeps_existing 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/pane.rs
git commit -m "feat(layout): add persisted PaneId + assign_pane_ids"
```

---

## Task 2: `vmux_core` position types + extend `PageArchiveRequest`

**Files:**
- Modify: `crates/vmux_core/src/archive.rs` (add types, extend message)
- Modify: `crates/vmux_core/src/lib.rs` (register reflect types ~`44`)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `crates/vmux_core/src/archive.rs`:

```rust
#[test]
fn archived_position_types_registered_by_core_plugin() {
    let mut app = App::new();
    app.add_plugins(crate::CorePlugin);
    let registry = app.world().resource::<AppTypeRegistry>().read();
    assert!(registry.get(std::any::TypeId::of::<ArchivedPagePosition>()).is_some());
    assert!(registry.get(std::any::TypeId::of::<PaneStep>()).is_some());
    assert!(registry.get(std::any::TypeId::of::<SplitAxis>()).is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_core archived_position_types_registered_by_core_plugin 2>&1 | tail -20`
Expected: FAIL — types not found.

- [ ] **Step 3: Add the types**

In `crates/vmux_core/src/archive.rs`, after the `ArchivedPage` struct, add:

```rust
#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_core::archive"]
pub struct ArchivedPagePosition {
    pub leaf_pane_id: String,
    pub stack_index: usize,
    pub pane_path: Vec<PaneStep>,
}

#[derive(Clone, Debug, Reflect, Default, PartialEq)]
#[type_path = "vmux_core::archive"]
pub struct PaneStep {
    pub split_id: String,
    pub axis: SplitAxis,
    pub child_index: usize,
    pub flex_weights: Vec<f32>,
}

#[derive(Clone, Copy, Debug, Reflect, Default, PartialEq, Eq)]
#[type_path = "vmux_core::archive"]
pub enum SplitAxis {
    #[default]
    Row,
    Column,
}
```

- [ ] **Step 4: Extend `PageArchiveRequest`**

In the same file, add the three fields to `PageArchiveRequest`:

```rust
#[derive(Message, Clone, Debug)]
pub struct PageArchiveRequest {
    pub url: String,
    pub title: String,
    pub space_id: String,
    pub launch: Option<TerminalLaunch>,
    pub tab_index: Option<usize>,
    pub leaf_pane_id: String,
    pub stack_index: usize,
    pub pane_path: Vec<PaneStep>,
}
```

- [ ] **Step 5: Register the reflect types**

In `crates/vmux_core/src/lib.rs`, extend the `CorePlugin` registration (`~44`):

```rust
app.register_type::<PageMetadata>()
    .register_type::<ArchivedPage>()
    .register_type::<crate::archive::ArchivedPagePosition>()
    .register_type::<crate::archive::PaneStep>()
    .register_type::<crate::archive::SplitAxis>()
    .register_type::<Vec<crate::archive::PaneStep>>()
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p vmux_core archived_position_types_registered_by_core_plugin 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 7: Fix existing `PageArchiveRequest` constructors**

The new fields break existing literals. Update every `PageArchiveRequest { … }` in `crates/vmux_core/src/archive.rs` tests and in `crates/vmux_layout/src/archive.rs` (capture/close tests) to append:

```rust
    leaf_pane_id: String::new(),
    stack_index: 0,
    pane_path: Vec::new(),
```

Run: `cargo build -p vmux_core -p vmux_layout 2>&1 | tail -20`
Expected: compiles (no missing-field errors).

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_core/src/archive.rs crates/vmux_core/src/lib.rs crates/vmux_layout/src/archive.rs
git commit -m "feat(core): add ArchivedPagePosition + PaneStep/SplitAxis"
```

---

## Task 3: Spawn `ArchivedPagePosition` in `capture_archived_pages`

**Files:**
- Modify: `crates/vmux_layout/src/archive.rs` (`capture_archived_pages`)

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/vmux_layout/src/archive.rs`:

```rust
#[test]
fn capture_spawns_position_component() {
    let mut app = App::new();
    app.add_message::<PageArchiveRequest>()
        .add_systems(Update, capture_archived_pages);
    app.world_mut()
        .resource_mut::<Messages<PageArchiveRequest>>()
        .write(PageArchiveRequest {
            url: "https://a.example".to_string(),
            title: "A".to_string(),
            space_id: "s".to_string(),
            launch: None,
            tab_index: Some(0),
            leaf_pane_id: "leaf-1".to_string(),
            stack_index: 2,
            pane_path: vec![vmux_core::archive::PaneStep {
                split_id: "root".to_string(),
                axis: vmux_core::archive::SplitAxis::Row,
                child_index: 1,
                flex_weights: vec![1.0, 2.0],
            }],
        });
    app.update();
    let mut q = app
        .world_mut()
        .query::<(&ArchivedPage, &vmux_core::archive::ArchivedPagePosition)>();
    let (page, pos) = q.single(app.world()).expect("archived page + position");
    assert_eq!(page.url, "https://a.example");
    assert_eq!(pos.leaf_pane_id, "leaf-1");
    assert_eq!(pos.stack_index, 2);
    assert_eq!(pos.pane_path.len(), 1);
    assert_eq!(pos.pane_path[0].child_index, 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout capture_spawns_position_component 2>&1 | tail -20`
Expected: FAIL — only `ArchivedPage` is spawned; query for position returns none.

- [ ] **Step 3: Update `capture_archived_pages`**

In `crates/vmux_layout/src/archive.rs`, change the spawn in `capture_archived_pages` to spawn both components:

```rust
fn capture_archived_pages(mut reader: MessageReader<PageArchiveRequest>, mut commands: Commands) {
    for req in reader.read() {
        if req.url.is_empty() {
            continue;
        }
        commands.spawn((
            ArchivedPage {
                url: req.url.clone(),
                title: req.title.clone(),
                space_id: req.space_id.clone(),
                closed_at: now_millis(),
                launch: req.launch.clone(),
                tab_index: req.tab_index,
            },
            vmux_core::archive::ArchivedPagePosition {
                leaf_pane_id: req.leaf_pane_id.clone(),
                stack_index: req.stack_index,
                pane_path: req.pane_path.clone(),
            },
        ));
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_layout capture_spawns_position_component 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/archive.rs
git commit -m "feat(layout): capture spawns ArchivedPagePosition alongside ArchivedPage"
```

---

## Task 4: Compute the full position in `archive_on_stack_close`

**Files:**
- Modify: `crates/vmux_layout/src/archive.rs` (`archive_on_stack_close`, add `pane_path_of` helper)

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/vmux_layout/src/archive.rs`. This builds Space → Tab → root split → leaf(0) | leaf(1, with the closing stack), and asserts the captured path + leaf id + stack index:

```rust
#[test]
fn close_records_pane_path_and_leaf() {
    use crate::pane::{Pane, PaneId, PaneSize, PaneSplit, PaneSplitDirection};
    let mut app = App::new();
    app.add_message::<AppCommand>()
        .add_message::<PageArchiveRequest>()
        .init_resource::<FocusedStack>()
        .add_systems(Update, super::archive_on_stack_close);
    let space = app.world_mut().spawn((Space, SpaceId("s1".to_string()))).id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit { direction: PaneSplitDirection::Row },
            PaneId("root".to_string()),
            ChildOf(tab),
        ))
        .id();
    let leaf0 = app
        .world_mut()
        .spawn((Pane, PaneId("leaf0".to_string()), PaneSize { flex_grow: 1.0 }, ChildOf(root)))
        .id();
    let leaf1 = app
        .world_mut()
        .spawn((Pane, PaneId("leaf1".to_string()), PaneSize { flex_grow: 3.0 }, ChildOf(root)))
        .id();
    let _ = leaf0;
    app.world_mut().spawn((Stack::default(), ChildOf(leaf1))); // sibling stack at index 0
    let stack = app
        .world_mut()
        .spawn((
            Stack::default(),
            PageMetadata { url: "https://z.example".to_string(), ..default() },
            ChildOf(leaf1),
        ))
        .id();
    app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close)));
    app.update();
    let reqs = drain_archive_reqs(&mut app);
    assert_eq!(reqs.len(), 1);
    let req = &reqs[0];
    assert_eq!(req.leaf_pane_id, "leaf1");
    assert_eq!(req.stack_index, 1);
    assert_eq!(req.pane_path.len(), 1);
    assert_eq!(req.pane_path[0].split_id, "root");
    assert_eq!(req.pane_path[0].child_index, 1);
    assert_eq!(req.pane_path[0].flex_weights, vec![1.0, 3.0]);
    assert!(matches!(req.pane_path[0].axis, vmux_core::archive::SplitAxis::Row));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout close_records_pane_path_and_leaf 2>&1 | tail -20`
Expected: FAIL — request fields are empty/default.

- [ ] **Step 3: Add the `pane_path_of` helper**

In `crates/vmux_layout/src/archive.rs`, add imports at the top:

```rust
use crate::pane::{Pane, PaneId, PaneSize, PaneSplit, PaneSplitDirection};
use vmux_core::archive::{PaneStep, SplitAxis};
```

Add the helper (returns the leaf `PaneId`, the stack index, and the root→leaf split path):

```rust
#[allow(clippy::type_complexity)]
fn pane_path_of(
    stack: Entity,
    child_of: &Query<&ChildOf>,
    children_q: &Query<&Children>,
    pane_ids: &Query<&PaneId>,
    splits: &Query<&PaneSplit>,
    pane_sizes: &Query<&PaneSize>,
    panes: &Query<(), With<Pane>>,
    stacks: &Query<(), With<Stack>>,
    tabs: &Query<(), With<Tab>>,
) -> Option<(String, usize, Vec<PaneStep>)> {
    let leaf = child_of.get(stack).ok()?.parent();
    if !panes.contains(leaf) {
        return None;
    }
    let leaf_pane_id = pane_ids.get(leaf).ok()?.0.clone();
    let stack_index = children_q
        .get(leaf)
        .ok()?
        .iter()
        .filter(|&e| stacks.contains(e))
        .position(|e| e == stack)?;

    let mut steps_rev: Vec<PaneStep> = Vec::new();
    let mut cur = leaf;
    loop {
        let parent = child_of.get(cur).ok()?.parent();
        if tabs.contains(parent) {
            break;
        }
        let Ok(split) = splits.get(parent) else {
            return None;
        };
        let pane_children: Vec<Entity> = children_q
            .get(parent)
            .map(|c| c.iter().filter(|&e| panes.contains(e)).collect())
            .unwrap_or_default();
        let child_index = pane_children.iter().position(|&e| e == cur)?;
        let flex_weights = pane_children
            .iter()
            .map(|&e| pane_sizes.get(e).map(|s| s.flex_grow).unwrap_or(1.0))
            .collect();
        steps_rev.push(PaneStep {
            split_id: pane_ids.get(parent).ok()?.0.clone(),
            axis: match split.direction {
                PaneSplitDirection::Row => SplitAxis::Row,
                PaneSplitDirection::Column => SplitAxis::Column,
            },
            child_index,
            flex_weights,
        });
        cur = parent;
    }
    steps_rev.reverse();
    Some((leaf_pane_id, stack_index, steps_rev))
}
```

- [ ] **Step 4: Wire it into `archive_on_stack_close`**

Add the new queries to the `archive_on_stack_close` system signature:

```rust
    pane_ids: Query<&PaneId>,
    splits: Query<&PaneSplit>,
    pane_sizes: Query<&PaneSize>,
    panes: Query<(), With<Pane>>,
```

Then, replace the `writer.write(PageArchiveRequest { … })` block at the end so it computes and includes the position:

```rust
    let (leaf_pane_id, stack_index, pane_path) = pane_path_of(
        stack, &child_of, &children_q, &pane_ids, &splits, &pane_sizes, &panes, &tabs,
    )
    .unwrap_or_default();
    writer.write(PageArchiveRequest {
        url: meta.url.clone(),
        title: meta.title.clone(),
        space_id,
        launch: launch.cloned(),
        tab_index,
        leaf_pane_id,
        stack_index,
        pane_path,
    });
```

Note: `children_q` and `tabs` already exist in the system; `child_of` exists as `child_of`. Keep the existing `tab_index` computation.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vmux_layout close_records_pane_path_and_leaf close_records_tab_index_of_closing_stack 2>&1 | tail -25`
Expected: PASS (new test + existing tab-index test still green).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/archive.rs
git commit -m "feat(layout): capture full pane path + leaf id + stack index at close"
```

---

## Task 5: Persistence allowlist + round-trip test

**Files:**
- Modify: `crates/vmux_desktop/src/persistence.rs` (allowlist in `save_space_to_path`, add test)

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/vmux_desktop/src/persistence.rs` (mirrors `window_geometry_round_trips_through_store`):

```rust
#[test]
fn pane_id_and_position_round_trip_through_store() {
    use vmux_core::archive::{ArchivedPage, ArchivedPagePosition, PaneStep, SplitAxis};
    use vmux_layout::pane::PaneId;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("store.ron");

    let mut app_save = App::new();
    app_save.add_plugins(MinimalPlugins);
    app_save.add_plugins(vmux_core::CorePlugin);
    app_save.register_type::<PaneId>();
    app_save.add_observer(save_on_default_event);
    app_save.world_mut().spawn((Save, Pane, PaneId("p-1".to_string())));
    app_save.world_mut().spawn((
        Save,
        ArchivedPage { url: "https://x".into(), ..default() },
        ArchivedPagePosition {
            leaf_pane_id: "p-1".into(),
            stack_index: 1,
            pane_path: vec![PaneStep {
                split_id: "root".into(),
                axis: SplitAxis::Column,
                child_index: 2,
                flex_weights: vec![1.0, 4.0],
            }],
        },
    ));
    save_space_to_path(&mut app_save.world_mut().commands(), path.clone());
    app_save.update();
    assert!(path.exists());

    let mut app_load = App::new();
    app_load.add_plugins(MinimalPlugins);
    app_load.add_plugins(vmux_core::CorePlugin);
    app_load.register_type::<PaneId>();
    app_load.add_observer(load_on_default_event);
    app_load.update();
    app_load
        .world_mut()
        .commands()
        .trigger_load(LoadWorld::default_from_file(path));
    app_load.update();

    let pid = app_load
        .world_mut()
        .query::<&PaneId>()
        .single(app_load.world())
        .expect("PaneId round-tripped");
    assert_eq!(pid.0, "p-1");
    let pos = app_load
        .world_mut()
        .query::<&ArchivedPagePosition>()
        .single(app_load.world())
        .expect("position round-tripped");
    assert_eq!(pos.leaf_pane_id, "p-1");
    assert_eq!(pos.pane_path[0].child_index, 2);
    assert!(matches!(pos.pane_path[0].axis, SplitAxis::Column));
}
```

Add `use vmux_layout::pane::Pane;` to the test module imports if not already present (the module already imports `Pane` via the top-level `use` list — confirm `Pane` is in scope; if not, qualify as `vmux_layout::pane::Pane`).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_desktop pane_id_and_position_round_trip_through_store 2>&1 | tail -25`
Expected: FAIL — `PaneId` / `ArchivedPagePosition` not in the save allowlist, so the loaded world has no such components.

- [ ] **Step 3: Extend the allowlist**

In `save_space_to_path` (`persistence.rs:178`), add two `.allow` lines to the `SceneFilter`:

```rust
        .allow::<PageMetadata>()
        .allow::<ArchivedPage>()
        .allow::<vmux_core::archive::ArchivedPagePosition>()
        .allow::<vmux_layout::pane::PaneId>()
```

Add imports at the top of `persistence.rs` as needed: extend the existing `use vmux_core::{…}` to include `archive::ArchivedPagePosition` (or reference fully-qualified as above), and the existing `pane::{…}` import to include `PaneId`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_desktop pane_id_and_position_round_trip_through_store 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_desktop/src/persistence.rs
git commit -m "feat(desktop): persist PaneId + ArchivedPagePosition in store.ron"
```

---

## Task 6: Reopen ladder — step 1 (leaf alive), step 3 (recreate tab), step 4 (fallback), no-position passthrough

**Files:**
- Modify: `crates/vmux_layout/src/archive.rs` (`handle_reopen_closed_page`, add helpers)

This task restructures the reopen handler to read the optional position and target a `Stack` entity via the ladder, then funnels into the existing content-respawn code. Reconstruction (step 2) is stubbed to fall through to step 3 here and implemented in Task 7.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/vmux_layout/src/archive.rs`:

```rust
#[test]
fn reopen_into_surviving_leaf_pane_at_index() {
    use crate::pane::{Pane, PaneId};
    let mut app = reopen_app();
    let space = app.world_mut().spawn((Space, SpaceId("s1".to_string()))).id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let leaf = app
        .world_mut()
        .spawn((Pane, PaneId("leaf-A".to_string()), ChildOf(tab)))
        .id();
    app.world_mut().spawn((crate::stack::Stack::default(), ChildOf(leaf))); // existing stack idx 0
    app.world_mut().spawn((
        ArchivedPage {
            url: "https://z.example".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            ..default()
        },
        vmux_core::archive::ArchivedPagePosition {
            leaf_pane_id: "leaf-A".to_string(),
            stack_index: 0,
            pane_path: Vec::new(),
        },
    ));
    dispatch_reopen(&mut app);

    let children = app.world().entity(leaf).get::<Children>().unwrap();
    let stacks: Vec<Entity> = children
        .iter()
        .filter(|&e| app.world().entity(e).contains::<crate::stack::Stack>())
        .collect();
    assert_eq!(stacks.len(), 2, "stack added into the existing leaf pane");
    let opens = drain_opens(&mut app);
    assert_eq!(opens.len(), 1);
    assert_eq!(opens[0].url, "https://z.example");
}

#[test]
fn reopen_without_position_recreates_tab() {
    let mut app = reopen_app();
    app.world_mut()
        .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
    app.world_mut().spawn(ArchivedPage {
        url: "https://a.example".to_string(),
        space_id: "s1".to_string(),
        closed_at: 5,
        ..default()
    }); // no ArchivedPagePosition component
    dispatch_reopen(&mut app);
    let opens = drain_opens(&mut app);
    assert_eq!(opens.len(), 1);
    let mut tabs = app.world_mut().query::<&crate::tab::Tab>();
    assert_eq!(tabs.iter(app.world()).count(), 1, "a tab was recreated");
}
```

The existing tests `reopen_web_opens_in_origin_space_and_consumes_entry`, `reopen_falls_back_to_active_space_when_origin_gone`, `reopen_restores_tab_at_original_index`, and `reopen_appends_when_origin_space_gone` will be adjusted in Step 4 to add the (absent) position component or keep relying on the recreate path.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_layout reopen_into_surviving_leaf_pane_at_index reopen_without_position_recreates_tab 2>&1 | tail -25`
Expected: FAIL — handler ignores `ArchivedPagePosition`; always recreates a tab (so leaf test fails the count assertion).

- [ ] **Step 3: Restructure `handle_reopen_closed_page`**

Add queries to the system signature:

```rust
    positions: Query<&vmux_core::archive::ArchivedPagePosition>,
    pane_ids: Query<(Entity, &crate::pane::PaneId)>,
    leaf_panes: Query<(), (With<crate::pane::Pane>, Without<crate::pane::PaneSplit>)>,
    child_of: Query<&ChildOf>,
    children_q: Query<&Children>,
    stacks_q: Query<(), With<Stack>>,
```

Replace the body after the entry is selected (keep newest-by-`closed_at` selection, but also read the optional position). Use a helper that returns the target `Stack` entity:

```rust
    let Some((entry_entity, page)) = archived
        .iter()
        .max_by_key(|(_, p)| p.closed_at)
        .map(|(e, p)| (e, p.clone()))
    else {
        return;
    };
    let position = positions.get(entry_entity).ok().cloned();

    let origin_space = spaces.iter().find(|(_, id)| id.0 == page.space_id).map(|(e, _)| e);
    let target_space = origin_space
        .or_else(|| active_space.0.filter(|e| any_space.get(*e).is_ok()))
        .or_else(|| any_space.iter().next());
    let Some(space) = target_space else {
        return;
    };

    let stack = resolve_reopen_stack(
        space,
        origin_space == Some(space),
        page.tab_index,
        position.as_ref(),
        &pane_ids,
        &leaf_panes,
        &child_of,
        &children_q,
        &stacks_q,
        &mut commands,
        *primary_window,
        settings.pane.gap,
    );

    commands.entity(space).insert(vmux_history::LastActivatedAt::now());
    commands.entity(stack).insert(vmux_history::LastActivatedAt::now());
```

Then keep the existing agent / terminal / page_open dispatch block **unchanged**, but replace every `scaffold.stack` reference with `stack`, and remove the earlier `let scaffold = spawn_tab_scaffold_in_space(...)` + the `PageMetadata`/`insert_children`/origin-index block (that logic moves into `resolve_reopen_stack`). Keep the final `commands.entity(entry_entity).despawn();`.

Add the resolver helper (step 1, step 3, step 4; step 2 stubbed to `None` for now so it falls to recreate-tab):

```rust
#[allow(clippy::too_many_arguments)]
fn resolve_reopen_stack(
    space: Entity,
    origin_matches: bool,
    tab_index: Option<usize>,
    position: Option<&vmux_core::archive::ArchivedPagePosition>,
    pane_ids: &Query<(Entity, &crate::pane::PaneId)>,
    leaf_panes: &Query<(), (With<crate::pane::Pane>, Without<crate::pane::PaneSplit>)>,
    child_of: &Query<&ChildOf>,
    children_q: &Query<&Children>,
    stacks_q: &Query<(), With<Stack>>,
    commands: &mut Commands,
    primary_window: Entity,
    gap: f32,
) -> Entity {
    use bevy::ecs::relationship::Relationship;
    if let Some(pos) = position.filter(|p| !p.leaf_pane_id.is_empty()) {
        // Step 1: original leaf pane still alive and within this space.
        if let Some(leaf) = pane_ids
            .iter()
            .find(|(e, id)| id.0 == pos.leaf_pane_id && leaf_panes.contains(*e))
            .map(|(e, _)| e)
            .filter(|&leaf| pane_in_space(leaf, space, child_of))
        {
            return spawn_stack_in_leaf(leaf, pos.stack_index, children_q, stacks_q, commands);
        }
        // Step 2: reconstruction — implemented in Task 7. For now, fall through.
        if let Some(leaf) = reattach_along_path(space, pos, pane_ids, child_of, children_q, commands)
        {
            return spawn_stack_in_leaf(leaf, pos.stack_index, children_q, stacks_q, commands);
        }
    }
    // Step 3 / 4: recreate a tab scaffold in the space.
    let scaffold = spawn_tab_scaffold_in_space(commands, space, primary_window, gap);
    if origin_matches
        && let Some(idx) = tab_index
    {
        commands.entity(space).insert_children(idx, &[scaffold.tab]);
    }
    scaffold.stack
}

fn pane_in_space(pane: Entity, space: Entity, child_of: &Query<&ChildOf>) -> bool {
    use bevy::ecs::relationship::Relationship;
    let mut cur = pane;
    while let Ok(rel) = child_of.get(cur) {
        let parent = rel.get();
        if parent == space {
            return true;
        }
        cur = parent;
    }
    false
}

fn spawn_stack_in_leaf(
    leaf: Entity,
    stack_index: usize,
    children_q: &Query<&Children>,
    stacks_q: &Query<(), With<Stack>>,
    commands: &mut Commands,
) -> Entity {
    let stack = commands
        .spawn((crate::stack::stack_bundle(), vmux_history::LastActivatedAt::now(), ChildOf(leaf)))
        .id();
    let stack_count = children_q
        .get(leaf)
        .map(|c| c.iter().filter(|&e| stacks_q.contains(e)).count())
        .unwrap_or(0);
    let idx = stack_index.min(stack_count);
    commands.entity(leaf).insert_children(idx, &[stack]);
    stack
}

// Task 7 implements this; stub returns None so step 2 falls through to recreate-tab.
#[allow(clippy::too_many_arguments)]
fn reattach_along_path(
    _space: Entity,
    _pos: &vmux_core::archive::ArchivedPagePosition,
    _pane_ids: &Query<(Entity, &crate::pane::PaneId)>,
    _child_of: &Query<&ChildOf>,
    _children_q: &Query<&Children>,
    _commands: &mut Commands,
) -> Option<Entity> {
    None
}
```

Add the needed imports to `archive.rs` (some may already be present from Task 4): `use crate::pane::{Pane, PaneId, PaneSplit};` and `use crate::stack::stack_bundle;`.

- [ ] **Step 4: Update existing reopen tests for the new shape**

The existing tests spawn `ArchivedPage` without a position component — they already exercise step 3/4. Confirm they still pass; the `reopen_restores_tab_at_original_index` and `reopen_appends_when_origin_space_gone` tests rely on `tab_index` and the recreate path, which is preserved. Add `..default()` to any `ArchivedPage { … }` literals missing fields if the compiler complains.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vmux_layout reopen_ 2>&1 | tail -40`
Expected: PASS for all `reopen_*` tests (new + existing).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/archive.rs
git commit -m "feat(layout): reopen ladder step 1 (leaf alive) + recreate-tab fallback"
```

---

## Task 7: Reopen step 2 — split path reconstruction

**Files:**
- Modify: `crates/vmux_layout/src/archive.rs` (implement `reattach_along_path`)

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/vmux_layout/src/archive.rs`:

```rust
#[test]
fn reopen_readds_leaf_under_surviving_split() {
    // Split kept its other child (≥2 children case): root split alive, closed leaf gone.
    use crate::pane::{Pane, PaneId, PaneSplit, PaneSplitDirection};
    let mut app = reopen_app();
    let space = app.world_mut().spawn((Space, SpaceId("s1".to_string()))).id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit { direction: PaneSplitDirection::Row },
            PaneId("root".to_string()),
            ChildOf(tab),
        ))
        .id();
    app.world_mut()
        .spawn((Pane, PaneId("survivor".to_string()), ChildOf(root))); // child idx 0 survives
    app.world_mut().spawn((
        ArchivedPage { url: "https://z".to_string(), space_id: "s1".to_string(), closed_at: 5, ..default() },
        vmux_core::archive::ArchivedPagePosition {
            leaf_pane_id: "gone-leaf".to_string(),
            stack_index: 0,
            pane_path: vec![vmux_core::archive::PaneStep {
                split_id: "root".to_string(),
                axis: vmux_core::archive::SplitAxis::Row,
                child_index: 1,
                flex_weights: vec![1.0, 1.0],
            }],
        },
    ));
    dispatch_reopen(&mut app);

    let root_children = app.world().entity(root).get::<Children>().unwrap();
    let panes: Vec<Entity> = root_children
        .iter()
        .filter(|&e| app.world().entity(e).contains::<Pane>())
        .collect();
    assert_eq!(panes.len(), 2, "reopened leaf re-added under surviving split");
    // The new leaf holds the reopened stack.
    let has_stack = panes.iter().any(|&p| {
        app.world()
            .entity(p)
            .get::<Children>()
            .map(|c| c.iter().any(|e| app.world().entity(e).contains::<crate::stack::Stack>()))
            .unwrap_or(false)
    });
    assert!(has_stack);
    assert_eq!(drain_opens(&mut app).len(), 1);
}

#[test]
fn reopen_reconstructs_collapsed_split_level() {
    // root exists; the nested split that held the leaf was collapsed away.
    use crate::pane::{Pane, PaneId, PaneSplit, PaneSplitDirection};
    let mut app = reopen_app();
    let space = app.world_mut().spawn((Space, SpaceId("s1".to_string()))).id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit { direction: PaneSplitDirection::Row },
            PaneId("root".to_string()),
            ChildOf(tab),
        ))
        .id();
    app.world_mut().spawn((Pane, PaneId("root-leaf".to_string()), ChildOf(root)));
    app.world_mut().spawn((
        ArchivedPage { url: "https://z".to_string(), space_id: "s1".to_string(), closed_at: 5, ..default() },
        vmux_core::archive::ArchivedPagePosition {
            leaf_pane_id: "deep-leaf".to_string(),
            stack_index: 0,
            pane_path: vec![
                vmux_core::archive::PaneStep {
                    split_id: "root".to_string(),
                    axis: vmux_core::archive::SplitAxis::Row,
                    child_index: 1,
                    flex_weights: vec![1.0, 1.0],
                },
                vmux_core::archive::PaneStep {
                    split_id: "nested".to_string(),
                    axis: vmux_core::archive::SplitAxis::Column,
                    child_index: 0,
                    flex_weights: vec![1.0, 1.0],
                },
            ],
        },
    ));
    dispatch_reopen(&mut app);

    // A nested split was recreated under root, and the reopened stack exists somewhere under the tab.
    let mut split_ids = app.world_mut().query::<&PaneId>();
    let recreated_nested = split_ids
        .iter(app.world())
        .any(|id| id.0 == "nested");
    assert!(recreated_nested, "nested split recreated by id");
    let stack_count = app.world_mut().query::<&crate::stack::Stack>().iter(app.world()).count();
    assert_eq!(stack_count, 1);
    assert_eq!(drain_opens(&mut app).len(), 1);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_layout reopen_readds_leaf_under_surviving_split reopen_reconstructs_collapsed_split_level 2>&1 | tail -30`
Expected: FAIL — `reattach_along_path` stub returns `None`, so a separate tab is recreated (root children count / nested id assertions fail).

- [ ] **Step 3: Implement `reattach_along_path`**

Replace the stub in `crates/vmux_layout/src/archive.rs` with:

```rust
#[allow(clippy::too_many_arguments)]
fn reattach_along_path(
    space: Entity,
    pos: &vmux_core::archive::ArchivedPagePosition,
    pane_ids: &Query<(Entity, &crate::pane::PaneId)>,
    child_of: &Query<&ChildOf>,
    children_q: &Query<&Children>,
    commands: &mut Commands,
) -> Option<Entity> {
    use vmux_core::archive::SplitAxis;
    let path = &pos.pane_path;
    let root_step = path.first()?;
    let root = pane_ids
        .iter()
        .find(|(_, id)| id.0 == root_step.split_id)
        .map(|(e, _)| e)?;
    if !pane_in_space(root, space, child_of) {
        return None;
    }

    // Node id chain root→leaf: path[0..].split_id, then leaf_pane_id.
    let node_id = |i: usize| -> String {
        if i + 1 < path.len() {
            path[i + 1].split_id.clone()
        } else {
            pos.leaf_pane_id.clone()
        }
    };
    let find_child_by_id = |parent: Entity, id: &str| -> Option<Entity> {
        children_q.get(parent).ok()?.iter().find(|&child| {
            pane_ids
                .iter()
                .any(|(e, pid)| e == child && pid.0 == id)
        })
    };

    // Walk as deep as the stored chain still exists.
    let mut parent = root;
    let mut depth = 0usize;
    while depth < path.len() {
        match find_child_by_id(parent, &node_id(depth)) {
            Some(child) => {
                parent = child;
                depth += 1;
            }
            None => break,
        }
    }
    if depth == path.len() {
        // Whole chain incl. leaf already exists (handled by step 1 normally); reuse it.
        return Some(parent);
    }

    // Recreate splits for levels depth+1..path.len() under `parent`, then the leaf.
    for level in depth..path.len() {
        let step = &path[level];
        let is_last = level + 1 == path.len();
        let child_id = node_id(level);
        let flex = step.flex_weights.get(step.child_index).copied().unwrap_or(1.0);
        let new_child = if is_last {
            commands
                .spawn((
                    crate::pane::leaf_pane_bundle(),
                    crate::pane::PaneId(child_id),
                    crate::pane::PaneSize { flex_grow: flex },
                    vmux_history::LastActivatedAt::now(),
                    ChildOf(parent),
                ))
                .id()
        } else {
            let axis = match path[level + 1].axis {
                SplitAxis::Row => crate::pane::PaneSplitDirection::Row,
                SplitAxis::Column => crate::pane::PaneSplitDirection::Column,
            };
            commands
                .spawn((
                    crate::pane::split_root_bundle(axis),
                    crate::pane::PaneId(child_id),
                    crate::pane::PaneSize { flex_grow: flex },
                    ChildOf(parent),
                ))
                .id()
        };
        let insert_at = clamp_child_index(parent, step.child_index, children_q);
        commands.entity(parent).insert_children(insert_at, &[new_child]);
        parent = new_child;
    }
    Some(parent)
}

fn clamp_child_index(parent: Entity, idx: usize, children_q: &Query<&Children>) -> usize {
    let count = children_q.get(parent).map(|c| c.iter().count()).unwrap_or(0);
    idx.min(count)
}
```

Note: `node_id(level)` for the split-creation loop names the **child** being created at that level. The leaf reuses `pos.leaf_pane_id`; recreated nested splits reuse their recorded `split_id` (so future reopens resolve them).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_layout reopen_readds_leaf_under_surviving_split reopen_reconstructs_collapsed_split_level 2>&1 | tail -30`
Expected: PASS.

- [ ] **Step 5: Run the full layout reopen suite**

Run: `cargo test -p vmux_layout reopen_ 2>&1 | tail -40`
Expected: PASS for all `reopen_*`.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/archive.rs
git commit -m "feat(layout): reopen step 2 split path reconstruction"
```

---

## Task 8: Focus restoration + full verification

**Files:**
- Modify: `crates/vmux_layout/src/archive.rs` (focus the reopened leaf + tab)

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/vmux_layout/src/archive.rs`:

```rust
#[test]
fn reopen_focuses_restored_stack_and_ancestors() {
    use crate::pane::{Pane, PaneId};
    let mut app = reopen_app();
    let space = app.world_mut().spawn((Space, SpaceId("s1".to_string()))).id();
    let tab = app.world_mut().spawn((Tab::default(), ChildOf(space))).id();
    let leaf = app
        .world_mut()
        .spawn((Pane, PaneId("leaf-A".to_string()), ChildOf(tab)))
        .id();
    app.world_mut().spawn((
        ArchivedPage { url: "https://z".to_string(), space_id: "s1".to_string(), closed_at: 5, ..default() },
        vmux_core::archive::ArchivedPagePosition {
            leaf_pane_id: "leaf-A".to_string(),
            stack_index: 0,
            pane_path: Vec::new(),
        },
    ));
    dispatch_reopen(&mut app);
    assert!(app.world().entity(leaf).get::<vmux_history::LastActivatedAt>().is_some());
    assert!(app.world().entity(tab).get::<vmux_history::LastActivatedAt>().is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout reopen_focuses_restored_stack_and_ancestors 2>&1 | tail -20`
Expected: FAIL — leaf/tab not stamped `LastActivatedAt`.

- [ ] **Step 3: Stamp focus up the chain**

In `handle_reopen_closed_page`, after obtaining `stack`, walk its ancestors and stamp `LastActivatedAt` on the leaf pane and tab. Add after the `commands.entity(stack).insert(... LastActivatedAt::now());` line:

```rust
    {
        use bevy::ecs::relationship::Relationship;
        let mut cur = stack;
        while let Ok(rel) = child_of.get(cur) {
            let parent = rel.get();
            commands.entity(parent).insert(vmux_history::LastActivatedAt::now());
            if tabs_focus.contains(parent) {
                break;
            }
            cur = parent;
        }
    }
```

Add a `tabs_focus: Query<(), With<Tab>>` parameter to the system (distinct from any existing tab query name). Note: for a freshly spawned `stack`/leaf created this same frame, `child_of` won't yet observe the deferred `ChildOf`; this loop stamps already-live ancestors (the common step-1 case) and the recreate-tab path already stamps its own. Acceptable — focus is also re-derived by `ensure_active_*` systems next frame.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_layout reopen_focuses_restored_stack_and_ancestors 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Full verification**

Run the affected crate suites and fmt/clippy:

```bash
cargo test -p vmux_core -p vmux_layout 2>&1 | tail -30
cargo test -p vmux_desktop pane_id_and_position_round_trip_through_store 2>&1 | tail -15
cargo fmt --check 2>&1 | tail -5
cargo clippy -p vmux_core -p vmux_layout 2>&1 | tail -20
```

Expected: all tests PASS; fmt clean; clippy clean. If `cargo fmt` reordered files under `patches/`, run `git checkout -- patches/` and only keep `crates/` formatting (per repo convention).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/archive.rs
git commit -m "feat(layout): focus reopened stack and ancestors"
```

---

## Task 9: Delete the plan file

- [ ] **Step 1: Remove the plan once implemented**

```bash
git rm docs/plans/2026-06-25-reopen-closed-tab-position.md
git commit -m "docs: remove implemented reopen-position plan"
```

(Per AGENTS.md: delete the plan file once fully implemented. The design spec under `docs/specs/` stays.)

---

## Self-Review

- **Spec coverage:** PaneId + assign (§1) → T1. Position types + capture message (§2) → T2/T3/T4. Restore ladder steps 1–4 (§3) → T6 (1,3,4) + T7 (2). Focus restore (§3) → T8. Persistence allowlist + registration + no-schema-bump (§5) → T2 (register) + T5 (allowlist/round-trip). Out-of-scope items untouched. Testing list (§Testing) → T1,T4,T5,T6,T7,T8 each map to a listed case.
- **Placeholder scan:** none — every code step has full code; `reattach_along_path` is stubbed in T6 then fully implemented in T7 (explicitly noted), which is sequencing, not a placeholder.
- **Type consistency:** `ArchivedPagePosition { leaf_pane_id, stack_index, pane_path }`, `PaneStep { split_id, axis, child_index, flex_weights }`, `SplitAxis::{Row,Column}`, `PaneId(String)`, `spawn_stack_in_leaf`, `pane_in_space`, `clamp_child_index`, `reattach_along_path` names are used identically across T2–T8.
