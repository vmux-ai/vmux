# Persistent Session Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist all vmux layout, tab, and browsing history state across restarts using moonshine-save, and introduce a Profile entity as the root of the entity tree.

**Architecture:** All persistent state lives in ECS components with `Reflect` + `Save` derives. moonshine-save serializes entities with the `Save` marker to `session.ron`. On startup, entities are restored and observer systems rebuild view components (meshes, materials, Browser handles). `Active` marker is replaced with `LastActivatedAt` timestamp component. Systems query `LastActivatedAt` directly to find the active entity among siblings (max timestamp wins).

**Tech Stack:** Bevy 0.18, moonshine-save 0.6.1, CEF (via bevy_cef), RON serialization

**Spec:** `docs/superpowers/specs/2026-04-19-persistent-session-design.md`

**Part 2:** `docs/superpowers/plans/2026-04-19-persistent-session-part2.md` (Tasks 5-8)

---

## File Map

**New files:**
| File | Purpose |
|------|---------|
| `crates/vmux_desktop/src/persistence.rs` | SavePlugin: AutoSave, debounce, save/load observers, view rebuild |
| `crates/vmux_desktop/src/profile.rs` | ProfilePlugin: Profile component, default profile spawning |

**Modified files:**
| File | Changes |
|------|---------|
| `crates/vmux_desktop/Cargo.toml` | Add moonshine-save dependency |
| `crates/vmux_history/Cargo.toml` | Add moonshine-save dependency (native only) |
| `crates/vmux_header/Cargo.toml` | Add moonshine-save dependency (native only) |
| `crates/vmux_history/src/lib.rs` | Update CreatedAt/LastActivatedAt to i64 + Reflect + Save |
| `crates/vmux_desktop/src/lib.rs` | Register PersistencePlugin, ProfilePlugin, type registrations |
| `crates/vmux_desktop/src/layout/tab.rs` | Remove Active, add `active_among()` helper, refactor all systems |
| `crates/vmux_desktop/src/layout/pane.rs` | Use LastActivatedAt, add PaneSplitDirection |
| `crates/vmux_desktop/src/layout/space.rs` | Use LastActivatedAt directly |
| `crates/vmux_desktop/src/layout/focus_ring.rs` | Use LastActivatedAt directly |
| `crates/vmux_desktop/src/layout/window.rs` | Shell/session split, use LastActivatedAt |
| `crates/vmux_desktop/src/browser.rs` | Use LastActivatedAt, metadata sync, visit spawning, CEF restructure |
| `crates/vmux_header/src/system.rs` | Add Reflect to PageMetadata |
| `crates/vmux_history/src/plugin.rs` | Remove sample data, use real Visits |

---

### Task 1: Dependencies and shared timestamp components

**Files:**
- Modify: `crates/vmux_desktop/Cargo.toml`
- Modify: `crates/vmux_history/Cargo.toml`
- Modify: `crates/vmux_history/src/lib.rs`

- [ ] **Step 1: Add moonshine-save to vmux_desktop**

In `crates/vmux_desktop/Cargo.toml`, add to `[dependencies]`:

```toml
moonshine-save = { workspace = true }
```

- [ ] **Step 2: Add moonshine-save to vmux_history (native only)**

In `crates/vmux_history/Cargo.toml`, add to `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`:

```toml
moonshine-save = { workspace = true }
```

- [ ] **Step 3: Update timestamp components in vmux_history**

Replace the entire `crates/vmux_history/src/lib.rs`:

```rust
pub mod event;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use moonshine_save::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct CreatedAt(pub i64);

#[cfg(not(target_arch = "wasm32"))]
impl CreatedAt {
    pub fn now() -> Self { Self(now_millis()) }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct LastActivatedAt(pub i64);

#[cfg(not(target_arch = "wasm32"))]
impl LastActivatedAt {
    pub fn now() -> Self { Self(now_millis()) }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Copy, Debug, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct Visit;

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
```

- [ ] **Step 4: Build and verify**

Run: `cargo build -p vmux_history --lib && cargo build -p vmux_desktop`

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add moonshine-save deps, update timestamp components to i64 + Reflect"
```

---

### Task 2: Replace Active with LastActivatedAt

The largest task. `Active` (unit struct marker) is removed. `LastActivatedAt` (timestamp) determines active state. Systems query `LastActivatedAt` directly — the entity with the max timestamp among siblings is active. A shared helper `active_among()` keeps this DRY.

**Files:** `tab.rs`, `space.rs`, `pane.rs`, `focus_ring.rs`, `window.rs`, `browser.rs`

**Key helper** (defined in `tab.rs`, used by other modules):

```rust
pub(crate) fn active_among<'a>(
    entities: impl Iterator<Item = (Entity, &'a LastActivatedAt)>,
) -> Option<Entity> {
    entities.max_by_key(|(_, ts)| ts.0).map(|(e, _)| e)
}
```

**Pattern for finding active at each level:**

```rust
// Active space:
let active_space = active_among(spaces.iter());

// Active pane (leaf pane with max timestamp under active space):
// Collect leaf panes under space, then active_among()

// Active tab in a pane:
let active_tab = pane_children.get(pane).ok().and_then(|children| {
    active_among(children.iter().filter_map(|e| tabs.get(e).ok()))
});
```

**Pattern for activation (replaces insert/remove Active):**

```rust
// Before: commands.entity(old).remove::<Active>();
//         commands.entity(new).insert(Active);
// After (just update the target — old entity retains its older timestamp):
commands.entity(new).insert(LastActivatedAt::now());
```

- [ ] **Step 1: Rewrite tab.rs**

Replace entire `crates/vmux_desktop/src/layout/tab.rs`. Key changes:
- Remove `Active` struct
- Add `active_among()` helper (pub crate)
- Add `collect_leaf_panes()` helper (pub crate, for finding leaf panes under a space)
- `handle_tab_commands`: find active pane/tab via `active_among()` inline. Replace `insert(Active)` with `insert(LastActivatedAt::now())`. Remove all `.remove::<Active>()`.
- `sync_tab_picking`: for each pane, find tab with max `LastActivatedAt` for ZIndex

```rust
use crate::{
    browser::Browser,
    command::{AppCommand, ReadAppCommands, TabCommand},
    layout::pane::{Pane, PaneSplit},
    layout::space::Space,
    settings::AppSettings,
};
use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_history::LastActivatedAt;

pub(crate) struct TabPlugin;

impl Plugin for TabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_tab_commands.in_set(ReadAppCommands))
            .add_systems(PostUpdate, sync_tab_picking);
    }
}

#[derive(Component)]
pub(crate) struct Tab;

pub(crate) fn active_among<'a>(
    entities: impl Iterator<Item = (Entity, &'a LastActivatedAt)>,
) -> Option<Entity> {
    entities.max_by_key(|(_, ts)| ts.0).map(|(e, _)| e)
}

pub(crate) fn collect_leaf_panes(
    root: Entity,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    result: &mut Vec<Entity>,
) {
    if leaf_panes.contains(root) {
        result.push(root);
    }
    if let Ok(children) = all_children.get(root) {
        for child in children.iter() {
            collect_leaf_panes(child, all_children, leaf_panes, result);
        }
    }
}

/// Find the active pane (max LastActivatedAt) among leaf panes under a space.
pub(crate) fn active_pane_in_space(
    space: Entity,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: &Query<(Entity, &LastActivatedAt), With<Pane>>,
) -> Option<Entity> {
    let mut panes = Vec::new();
    collect_leaf_panes(space, all_children, leaf_panes, &mut panes);
    active_among(panes.iter().filter_map(|&e| pane_ts.get(e).ok()))
}

/// Find the active tab (max LastActivatedAt) in a pane.
pub(crate) fn active_tab_in_pane(
    pane: Entity,
    pane_children: &Query<&Children, With<Pane>>,
    tab_ts: &Query<(Entity, &LastActivatedAt), With<Tab>>,
) -> Option<Entity> {
    pane_children.get(pane).ok().and_then(|children| {
        active_among(children.iter().filter_map(|e| tab_ts.get(e).ok()))
    })
}

/// Find the globally focused tab: active space -> active pane -> active tab.
pub(crate) fn focused_tab(
    spaces: &Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: &Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: &Query<&Children, With<Pane>>,
    tab_ts: &Query<(Entity, &LastActivatedAt), With<Tab>>,
) -> (Option<Entity>, Option<Entity>, Option<Entity>) {
    let space = active_among(spaces.iter());
    let pane = space.and_then(|s| active_pane_in_space(s, all_children, leaf_panes, pane_ts));
    let tab = pane.and_then(|p| active_tab_in_pane(p, pane_children, tab_ts));
    (space, pane, tab)
}

pub(crate) fn tab_bundle() -> impl Bundle {
    (
        Tab,
        Transform::default(),
        GlobalTransform::default(),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
        ZIndex(0),
    )
}

fn handle_tab_commands(
    mut reader: MessageReader<AppCommand>,
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_children: Query<&Children, With<Pane>>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    tab_q: Query<Entity, With<Tab>>,
    settings: Res<AppSettings>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for cmd in reader.read() {
        let AppCommand::Tab(tab_cmd) = *cmd else { continue };
        let (_, active_pane, active_tab) = focused_tab(
            &spaces, &all_children, &leaf_panes, &pane_ts, &pane_children, &tab_ts,
        );

        match tab_cmd {
            TabCommand::New => {
                let Some(pane) = active_pane else { continue };
                let startup_url = settings.browser.startup_url.as_str();
                let tab = commands
                    .spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(pane)))
                    .id();
                commands.spawn((
                    Browser::new(&mut meshes, &mut webview_mt, startup_url),
                    ChildOf(tab),
                ));
            }
            TabCommand::Close => {
                let Some(pane) = active_pane else { continue };
                let Some(active) = active_tab else { continue };
                let Ok(children) = pane_children.get(pane) else { continue };
                let tabs_in_pane: Vec<Entity> = children.iter().filter(|&e| tab_q.contains(e)).collect();
                if tabs_in_pane.len() <= 1 {
                    let startup_url = settings.browser.startup_url.as_str();
                    commands.entity(active).despawn();
                    let tab = commands.spawn((tab_bundle(), LastActivatedAt::now(), ChildOf(pane))).id();
                    commands.spawn((Browser::new(&mut meshes, &mut webview_mt, startup_url), ChildOf(tab)));
                    continue;
                }
                let next = tabs_in_pane.iter().copied().find(|&e| e != active).unwrap();
                commands.entity(active).despawn();
                commands.entity(next).insert(LastActivatedAt::now());
            }
            TabCommand::Next | TabCommand::Previous => {
                let Some(pane) = active_pane else { continue };
                let Ok(children) = pane_children.get(pane) else { continue };
                let tab_entities: Vec<Entity> = children.iter().filter(|&e| tab_q.contains(e)).collect();
                if tab_entities.len() < 2 { continue }
                let Some(current) = tab_entities.iter().position(|&e| active_tab == Some(e)) else { continue };
                let delta: i32 = if tab_cmd == TabCommand::Next { 1 } else { -1 };
                let n = tab_entities.len() as i32;
                let idx = (current as i32 + delta).rem_euclid(n) as usize;
                commands.entity(tab_entities[idx]).insert(LastActivatedAt::now());
            }
            TabCommand::SelectIndex1 | TabCommand::SelectIndex2 | TabCommand::SelectIndex3
            | TabCommand::SelectIndex4 | TabCommand::SelectIndex5 | TabCommand::SelectIndex6
            | TabCommand::SelectIndex7 | TabCommand::SelectIndex8 | TabCommand::SelectLast => {
                let Some(pane) = active_pane else { continue };
                let Ok(children) = pane_children.get(pane) else { continue };
                let tab_entities: Vec<Entity> = children.iter().filter(|&e| tab_q.contains(e)).collect();
                if tab_entities.is_empty() { continue }
                let target_idx = match tab_cmd {
                    TabCommand::SelectIndex1 => 0, TabCommand::SelectIndex2 => 1,
                    TabCommand::SelectIndex3 => 2, TabCommand::SelectIndex4 => 3,
                    TabCommand::SelectIndex5 => 4, TabCommand::SelectIndex6 => 5,
                    TabCommand::SelectIndex7 => 6, TabCommand::SelectIndex8 => 7,
                    TabCommand::SelectLast => tab_entities.len() - 1, _ => continue,
                };
                if target_idx >= tab_entities.len() { continue }
                commands.entity(tab_entities[target_idx]).insert(LastActivatedAt::now());
            }
            TabCommand::Reopen | TabCommand::Duplicate | TabCommand::Pin
            | TabCommand::Mute | TabCommand::MoveToPane => {}
        }
    }
}

fn sync_tab_picking(
    pane_children: Query<&Children, With<Pane>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>,
    mut tabs: Query<(Entity, &mut ZIndex), With<Tab>>,
) {
    for pane in &leaf_panes {
        let active = active_tab_in_pane(pane, &pane_children, &tab_ts);
        if let Ok(children) = pane_children.get(pane) {
            for child in children.iter() {
                if let Ok((entity, mut z)) = tabs.get_mut(child) {
                    let target = if Some(entity) == active { ZIndex(1) } else { ZIndex(0) };
                    if *z != target { *z = target; }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Rewrite space.rs**

Replace entire `crates/vmux_desktop/src/layout/space.rs`:

```rust
use crate::{
    command::{AppCommand, ReadAppCommands, SpaceCommand},
    layout::tab::active_among,
};
use bevy::prelude::*;
use vmux_history::LastActivatedAt;

pub(crate) struct SpacePlugin;

impl Plugin for SpacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_space_commands.in_set(ReadAppCommands))
            .add_systems(PostUpdate, sync_space_visibility);
    }
}

#[derive(Component)]
pub(crate) struct Space;

pub(crate) fn space_bundle() -> impl Bundle {
    (
        Space,
        Transform::default(),
        GlobalTransform::default(),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
    )
}

fn handle_space_commands(mut reader: MessageReader<AppCommand>) {
    for cmd in reader.read() {
        let AppCommand::Space(space_cmd) = *cmd else { continue };
        match space_cmd {
            SpaceCommand::New | SpaceCommand::Close | SpaceCommand::Next
            | SpaceCommand::Previous | SpaceCommand::Rename => {}
        }
    }
}

pub(crate) fn sync_space_visibility(
    spaces: Query<(Entity, &LastActivatedAt, &mut Node), With<Space>>,
) {
    let active = spaces.iter().max_by_key(|(_, ts, _)| ts.0).map(|(e, _, _)| e);
    // Need a second iteration to mutate - use unsafe or separate query.
    // Actually, query for mutable access in a for loop is fine:
}
```

Actually, Bevy doesn't allow iterating a query immutably then mutably. Use a two-pass approach:

```rust
pub(crate) fn sync_space_visibility(
    mut spaces: Query<(Entity, &LastActivatedAt, &mut Node), With<Space>>,
) {
    // First pass: find active
    let active = spaces.iter().max_by_key(|(_, ts, _)| ts.0).map(|(e, _, _)| e);
    // Second pass: apply visibility
    for (entity, _, mut node) in &mut spaces {
        let target = if Some(entity) == active { Display::Flex } else { Display::None };
        if node.display != target { node.display = target; }
    }
}
```

- [ ] **Step 3: Update pane.rs**

In `crates/vmux_desktop/src/layout/pane.rs`:

Replace imports — remove `Active`, add `LastActivatedAt` and helpers from tab:

```rust
use crate::{
    browser::Browser,
    command::{AppCommand, PaneCommand, ReadAppCommands},
    layout::space::Space,
    layout::tab::{Tab, tab_bundle, active_among, active_pane_in_space, active_tab_in_pane,
                  collect_leaf_panes, focused_tab},
    settings::AppSettings,
};
use bevy::{
    ecs::relationship::Relationship,
    prelude::*,
    ui::{FlexDirection, UiGlobalTransform},
    window::PrimaryWindow,
};
use std::time::Instant;
use bevy_cef::prelude::*;
use vmux_history::LastActivatedAt;
```

In `handle_pane_commands`:
- Replace `active_pane: Query<Entity, (With<Active>, With<Pane>)>` with the full query set for `focused_tab()` and use its return value
- Replace all `insert(Active)` with `insert(LastActivatedAt::now())`
- Remove all `.remove::<Active>()` lines
- Replace `active_tab_in_pane(... &active_tabs)` with `active_tab_in_pane(pane, &pane_children, &tab_ts)`

In `on_pane_select`:
- Replace `active_space`/`active_pane` single-entity queries with `focused_tab()` call
- Replace `insert(Active)` with `insert(LastActivatedAt::now())`
- Remove `.remove::<Active>()`

In `poll_cursor_pane_focus`:
- Replace `active_pane: Query<Entity, (With<Active>, With<Pane>)>` with the queries needed to find active pane via `active_among()`
- Replace `active_pane.single().ok() == Some(target)` with comparing against `active_among()` result
- Replace `insert(Active)` with `insert(LastActivatedAt::now())`
- Remove `commands.entity(current).remove::<Active>()`

- [ ] **Step 4: Update focus_ring.rs**

In `crates/vmux_desktop/src/layout/focus_ring.rs`:

Replace `Active` import:

```rust
use crate::{
    layout::{
        window::{VmuxWindow, WEBVIEW_Z_FOCUS_RING},
        pane::{Pane, PaneSplit},
        tab::{active_among, active_pane_in_space, collect_leaf_panes},
        space::Space,
    },
    settings::{AppSettings, load_settings},
};
use vmux_history::LastActivatedAt;
```

In `sync_focus_ring_to_active_pane`, replace:

```rust
// OLD:
    active_pane: Query<(&ComputedNode, &UiGlobalTransform), (With<Active>, With<Pane>)>,
// NEW:
    spaces: Query<(Entity, &LastActivatedAt), With<Space>>,
    all_children: Query<&Children>,
    leaf_panes_q: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_ts: Query<(Entity, &LastActivatedAt), With<Pane>>,
    pane_layout: Query<(&ComputedNode, &UiGlobalTransform), With<Pane>>,

// OLD body:
    let Ok((pane_computed, pane_ui_gt)) = active_pane.single() else {
// NEW body:
    let active_space = active_among(spaces.iter());
    let active_pane = active_space.and_then(|s| {
        active_pane_in_space(s, &all_children, &leaf_panes_q, &pane_ts)
    });
    let Some(active) = active_pane else {
        *visibility = Visibility::Hidden;
        return;
    };
    let Ok((pane_computed, pane_ui_gt)) = pane_layout.get(active) else {
```

- [ ] **Step 5: Update window.rs**

In `crates/vmux_desktop/src/layout/window.rs`:

```rust
// Remove: use crate::layout::tab::Active;
// Add:
use vmux_history::LastActivatedAt;
```

Replace all 3 `Active` insertions in setup() `children![]` macro with `LastActivatedAt::now()`.

- [ ] **Step 6: Update browser.rs**

In `crates/vmux_desktop/src/browser.rs`:

Replace imports:

```rust
// Remove: tab::{Active, Tab, focused_tab, tab_bundle}
// Add:    tab::{Tab, tab_bundle, focused_tab, active_among, active_tab_in_pane}
// Add:    use vmux_history::LastActivatedAt;
```

For each system, replace Active-based queries with the queries needed for `focused_tab()` or direct `active_among()` calls:

**sync_keyboard_target**: Call `focused_tab()` to get `(_, _, active_tab)`.

**sync_children_to_ui**: For `active_tab_q.contains(parent)`, instead call `active_tab_in_pane()` for the parent's pane, then compare.

Or simpler: add a query `tab_ts: Query<(Entity, &LastActivatedAt), With<Tab>>` and `pane_children: Query<&Children, With<Pane>>`, then check `active_tab_in_pane(pane, &pane_children, &tab_ts) == Some(parent)`.

**sync_osr_webview_focus**: Use `focused_tab()` to get active tab.

**push_tabs_host_emit**: Use `focused_tab()`.

**push_pane_tree_emit**: Use `active_among()` for per-pane active tab check.

**handle_browser_commands**: Use `focused_tab()`.

**on_side_sheet_command_emit**: Replace `insert(Active)` with `insert(LastActivatedAt::now())`, remove `.remove::<Active>()`, use `active_tab_in_pane()` for active tab checks.

Remove the old `focused_tab` function from browser.rs (now in tab.rs). Remove the local `collect_leaf_panes` from browser.rs (now in tab.rs).

- [ ] **Step 7: Build and verify**

Run: `cargo build -p vmux_desktop`

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "refactor: replace Active marker with LastActivatedAt"
```

---

### Task 3: Model components with Reflect + Save

**Files:** `space.rs`, `pane.rs`, `tab.rs`, `lib.rs`

- [ ] **Step 1: Update Space**

In `space.rs`, add `use moonshine_save::prelude::*;` and change:

```rust
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct Space {
    pub name: String,
}
```

- [ ] **Step 2: Update Pane and PaneSplit**

In `pane.rs`, add `use moonshine_save::prelude::*;` and change:

```rust
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct Pane;

#[derive(Component, Reflect)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct PaneSplit {
    pub direction: PaneSplitDirection,
}

#[derive(Reflect, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PaneSplitDirection {
    #[default]
    Row,
    Column,
}
```

Update the SplitV/SplitH handler — replace `commands.entity(active).insert(PaneSplit)` with:

```rust
let split_dir = if pane_cmd == PaneCommand::SplitV {
    PaneSplitDirection::Row
} else {
    PaneSplitDirection::Column
};
commands.entity(active).insert(PaneSplit { direction: split_dir });
```

- [ ] **Step 3: Update Tab**

In `tab.rs`, add `use moonshine_save::prelude::*;` and change:

```rust
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct Tab {
    pub scroll_x: f32,
    pub scroll_y: f32,
}
```

- [ ] **Step 4: Register types in lib.rs**

Add to `VmuxPlugin::build()`:

```rust
        app.register_type::<layout::space::Space>()
            .register_type::<layout::pane::Pane>()
            .register_type::<layout::pane::PaneSplit>()
            .register_type::<layout::pane::PaneSplitDirection>()
            .register_type::<layout::tab::Tab>()
            .register_type::<vmux_history::CreatedAt>()
            .register_type::<vmux_history::LastActivatedAt>()
            .register_type::<vmux_history::Visit>();
```

- [ ] **Step 5: Build and verify**

Run: `cargo build -p vmux_desktop`

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: add Reflect + Save to Space, Pane, PaneSplit, Tab"
```

---

### Task 4: Profile plugin and CEF cache restructure

**Files:** Create `profile.rs`, modify `lib.rs`, modify `browser.rs`

- [ ] **Step 1: Create profile.rs**

Create `crates/vmux_desktop/src/profile.rs`:

```rust
use bevy::prelude::*;
use moonshine_save::prelude::*;

pub(crate) struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Profile>();
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct Profile {
    pub name: String,
    pub color: [f32; 4],
    pub icon: Option<String>,
}

impl Profile {
    pub fn default_profile() -> Self {
        Self {
            name: "default".to_string(),
            color: [0.4, 0.6, 1.0, 1.0],
            icon: None,
        }
    }
}
```

- [ ] **Step 2: Register in lib.rs**

```rust
mod profile;
use profile::ProfilePlugin;
// Add ProfilePlugin to the plugin tuple
```

- [ ] **Step 3: Restructure CEF cache path**

In `browser.rs`, replace `cef_root_cache_path()`:

```rust
fn cef_root_cache_path() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library/Application Support/vmux/profiles/default")
                .to_string_lossy()
                .into_owned()
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir()
            .to_str()
            .map(|p| format!("{p}/vmux_cef/profiles/default"))
    }
}
```

- [ ] **Step 4: Build, verify, commit**

```bash
cargo build -p vmux_desktop
git add -A && git commit -m "feat: add Profile plugin, restructure CEF cache to profiles/default/"
```

---

Continued in Part 2: `docs/superpowers/plans/2026-04-19-persistent-session-part2.md`
