# vmux_layout Message Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the MCP layout reconciler and snapshot from `vmux_desktop` into `vmux_layout` behind Bevy `Message` types. Drop the `Terminal`-marker coupling via URL-based kind detection. Extract a `split_root_bundle` factory so the reconciler stops drifting from the canonical bundle.

**Architecture:** `vmux_desktop` and `vmux_layout` communicate only through four `Message` types (`LayoutApplyRequest`/`Response`, `LayoutSnapshotRequest`/`Response`). DTOs move into `vmux_layout::protocol` (no more `Dto` suffix); `vmux_service::protocol::layout` becomes a re-export. Request ids cross the boundary as plain `u64`.

**Tech Stack:** Rust, Bevy ECS, vmux internal crates.

**Reference spec:** `docs/specs/2026-05-18-vmux-layout-message-boundary-design.md`

**Project conventions (from AGENTS.md):**
- No comments in code; no `mod.rs` (use filename + directory).
- After each commit, run fmt + clippy + test on the changed crates only.
- Verification template per commit:
  ```bash
  PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
  for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
  for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
  for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
  ```

---

## Phase A — Bundle factory + Cargo dep

### Task 1: Extract `split_root_bundle` factory in `vmux_layout::pane`

**Files:**
- Modify: `crates/vmux_layout/src/pane.rs`

- [ ] **Step 1: Add the factory function**

Insert after `leaf_pane_bundle` (around line 346):

```rust
pub fn split_root_bundle(direction: PaneSplitDirection) -> impl Bundle {
    let flex_direction = match direction {
        PaneSplitDirection::Row => FlexDirection::Row,
        PaneSplitDirection::Column => FlexDirection::Column,
    };
    let gap = pane_split_gaps(direction, crate::event::PANE_GAP_PX);
    (
        Pane,
        PaneSplit { direction },
        PaneSize::default(),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::default(),
        Node {
            flex_grow: 1.0,
            flex_direction,
            column_gap: gap.column_gap,
            row_gap: gap.row_gap,
            align_items: AlignItems::Stretch,
            ..default()
        },
    )
}
```

- [ ] **Step 2: Rewire `split_pane_in_two` to use the factory**

Replace the existing component-list insertion (around lines 367-381) with:

```rust
pub fn split_pane_in_two(
    commands: &mut Commands,
    active: Entity,
    direction: PaneSplitDirection,
    _pane_settings: &crate::settings::PaneSettings,
    existing_tabs: &[Entity],
) -> (Entity, Entity) {
    let pane1 = spawn_leaf_pane(commands, active);
    let pane2 = spawn_leaf_pane(commands, active);

    for tab in existing_tabs {
        commands.entity(*tab).insert(ChildOf(pane1));
    }

    commands.entity(active).insert(split_root_bundle(direction));
    commands.entity(pane2).insert(LastActivatedAt::now());

    (pane1, pane2)
}
```

Note: `split_root_bundle` includes `Pane`, which is idempotent on insert — re-inserting on `active` is a no-op for the marker.

- [ ] **Step 3: Verify and commit**

```bash
cargo fmt -p vmux_layout -- --check
env -u CEF_PATH cargo clippy -p vmux_layout --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_layout
git add crates/vmux_layout/src/pane.rs
git commit -m "refactor(layout): extract split_root_bundle factory"
```

---

### Task 2: Add `vmux_service` dep on `vmux_layout`

**Files:**
- Modify: `crates/vmux_service/Cargo.toml`

- [ ] **Step 1: Add the dep**

In the `[dependencies]` section, add:

```toml
vmux_layout = { path = "../vmux_layout" }
```

Keep alphabetical order with the other `vmux_*` deps.

- [ ] **Step 2: Verify build (no logic change yet — just dep addition)**

```bash
env -u CEF_PATH cargo build -p vmux_service
```

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_service/Cargo.toml
git commit -m "build(service): depend on vmux_layout for forthcoming DTO re-exports"
```

---

## Phase B — Move DTOs to `vmux_layout::protocol`

### Task 3: Create `vmux_layout::protocol` module with renamed types

**Files:**
- Create: `crates/vmux_layout/src/protocol.rs`
- Modify: `crates/vmux_layout/src/lib.rs`

- [ ] **Step 1: Copy the DTOs into `vmux_layout::protocol` with renames**

Open `crates/vmux_service/src/protocol/layout.rs` to see the current type definitions. Create the new file `crates/vmux_layout/src/protocol.rs` with the renamed types (the rkyv/serde derives, free functions `parse_id`/`format_id`, and all enums copied verbatim except the type names):

```rust
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    Serialize, Deserialize,
)]
pub enum NodeKind {
    Space,
    Split,
    Pane,
    Tab,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    Serialize, Deserialize,
)]
pub enum SplitDirection {
    Row,
    Column,
}

#[derive(
    Debug, Clone, Default, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    Serialize, Deserialize,
)]
pub struct Focus {
    pub space: Option<String>,
    pub pane: Option<String>,
    pub tab: Option<String>,
}

#[derive(
    Debug, Clone, Default, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    Serialize, Deserialize,
)]
pub struct Tab {
    pub id: Option<String>,
    pub title: String,
    pub url: String,
    pub is_loading: bool,
    pub favicon_url: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    Serialize, Deserialize,
)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LayoutNode {
    Split {
        #[serde(default)]
        id: Option<String>,
        direction: SplitDirection,
        #[serde(default)]
        flex_weights: Vec<f32>,
        children: Vec<LayoutNode>,
    },
    Pane {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        is_zoomed: bool,
        #[serde(default)]
        tabs: Vec<Tab>,
    },
}

#[derive(
    Debug, Clone, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    Serialize, Deserialize,
)]
pub struct Space {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub is_active: bool,
    pub root: LayoutNode,
}

#[derive(
    Debug, Clone, Default, PartialEq, Eq,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
    Serialize, Deserialize,
)]
pub struct LayoutSnapshot {
    pub spaces: Vec<Space>,
    #[serde(default)]
    pub focused: Focus,
}

pub fn format_id(kind: NodeKind, bits: u64) -> String {
    let prefix = match kind {
        NodeKind::Space => "space",
        NodeKind::Split => "split",
        NodeKind::Pane => "pane",
        NodeKind::Tab => "tab",
    };
    format!("{prefix}:{bits}")
}

pub fn parse_id(id: &str) -> Result<(NodeKind, u64), &'static str> {
    let (prefix, value) = id.split_once(':').ok_or("missing ':' in id")?;
    let kind = match prefix {
        "space" => NodeKind::Space,
        "split" => NodeKind::Split,
        "pane" => NodeKind::Pane,
        "tab" => NodeKind::Tab,
        _ => return Err("unknown id prefix"),
    };
    let value: u64 = value.parse().map_err(|_| "non-numeric id value")?;
    Ok((kind, value))
}
```

> If `vmux_service::protocol::layout` has tests for `parse_id`/`format_id` / serde round-trips, copy them into a `#[cfg(test)] mod tests` block at the end of this file using the new names.

- [ ] **Step 2: Register the module in `vmux_layout/src/lib.rs`**

Add near the other `pub mod` declarations:

```rust
pub mod protocol;
```

- [ ] **Step 3: Verify**

```bash
cargo fmt -p vmux_layout -- --check
env -u CEF_PATH cargo clippy -p vmux_layout --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_layout
```

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/src/protocol.rs crates/vmux_layout/src/lib.rs
git commit -m "feat(layout): add protocol DTOs (Space/Tab/LayoutNode/Focus/SplitDirection)"
```

---

### Task 4: `vmux_service::protocol::layout` re-exports from `vmux_layout::protocol`

**Files:**
- Modify: `crates/vmux_service/src/protocol/layout.rs`

- [ ] **Step 1: Replace the file contents with a re-export**

Open `crates/vmux_service/src/protocol/layout.rs` and replace all type definitions with:

```rust
pub use vmux_layout::protocol::{
    Focus as FocusDto, LayoutNode as LayoutNodeDto, LayoutSnapshot, NodeKind, Space as SpaceDto,
    SplitDirection as SplitDirectionDto, Tab as TabDto, format_id, parse_id,
};
```

> The `as Foo` aliases preserve the existing `Dto`-suffixed names so other callers (`vmux_mcp`, `vmux_service::protocol`, `vmux_desktop`) keep compiling until Task 6 renames them.

- [ ] **Step 2: Verify**

```bash
env -u CEF_PATH cargo build -p vmux_service
env -u CEF_PATH cargo test -p vmux_service
```

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_service/src/protocol/layout.rs
git commit -m "refactor(service): re-export layout DTOs from vmux_layout::protocol"
```

---

### Task 5: Drop the `Dto` aliases — switch callers to new names

**Files:**
- Modify: `crates/vmux_mcp/src/tools.rs`
- Modify: `crates/vmux_service/src/protocol.rs`
- Modify: `crates/vmux_desktop/src/agent.rs`
- Modify: `crates/vmux_desktop/src/agent_layout.rs`
- Modify: `crates/vmux_desktop/src/agent_layout/apply.rs`
- Modify: `crates/vmux_desktop/src/agent_layout/reconcile.rs`
- Modify: `crates/vmux_desktop/src/agent_query.rs`
- Modify: `crates/vmux_service/src/protocol/layout.rs`

- [ ] **Step 1: Rename references at each call site**

In each file above, change imports/uses of the `Dto`-suffixed names to the new names. The mechanical mapping:

```
FocusDto          → Focus
LayoutNodeDto     → LayoutNode
SpaceDto          → Space
SplitDirectionDto → SplitDirection
TabDto            → Tab
```

Example: in `crates/vmux_desktop/src/agent_layout/apply.rs`, replace:

```rust
use vmux_service::protocol::layout::{
    FocusDto, LayoutNodeDto, LayoutSnapshot, NodeKind, SpaceDto, SplitDirectionDto, TabDto,
    format_id, parse_id,
};
```

with:

```rust
use vmux_service::protocol::layout::{
    Focus, LayoutNode, LayoutSnapshot, NodeKind, Space, SplitDirection, Tab, format_id, parse_id,
};
```

Then search-and-replace `FocusDto` → `Focus`, `LayoutNodeDto` → `LayoutNode`, etc. within the file body.

Tip — use ripgrep to find all references in each file before editing:

```bash
rg -F 'FocusDto|LayoutNodeDto|SpaceDto|SplitDirectionDto|TabDto' crates/ --type rust
```

> Note: `vmux_desktop::layout::tab::Tab` (the *component*) will now collide with the imported `Tab` DTO in files that touch both. In those files (`apply.rs`, `reconcile.rs`, `agent_query.rs`, `agent_layout.rs`), keep the component import as `Tab` and import the DTO under its module path: `use vmux_service::protocol::layout as proto;` then refer to `proto::Tab`, `proto::LayoutNode::Split { .. }`, etc.

- [ ] **Step 2: Drop the `as` aliases in the re-export**

In `crates/vmux_service/src/protocol/layout.rs`:

```rust
pub use vmux_layout::protocol::{
    Focus, LayoutNode, LayoutSnapshot, NodeKind, Space, SplitDirection, Tab, format_id, parse_id,
};
```

- [ ] **Step 3: Verify changed crates**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_mcp/src/tools.rs \
        crates/vmux_service/src/protocol.rs \
        crates/vmux_service/src/protocol/layout.rs \
        crates/vmux_desktop/src/agent.rs \
        crates/vmux_desktop/src/agent_layout.rs \
        crates/vmux_desktop/src/agent_layout/apply.rs \
        crates/vmux_desktop/src/agent_layout/reconcile.rs \
        crates/vmux_desktop/src/agent_query.rs
git commit -m "refactor: drop Dto suffix from layout protocol types"
```

---

## Phase C — Move reconcile + snapshot logic to `vmux_layout`

### Task 6: Move `validate` + `plan_diff` to `vmux_layout::reconcile`

**Files:**
- Create: `crates/vmux_layout/src/reconcile.rs`
- Modify: `crates/vmux_layout/src/lib.rs`
- Modify: `crates/vmux_desktop/src/agent_layout.rs` (re-export shim)
- Modify: `crates/vmux_desktop/src/agent_layout/apply.rs`

- [ ] **Step 1: Create `crates/vmux_layout/src/reconcile.rs` with the validate + plan_diff logic**

Move the contents of `crates/vmux_desktop/src/agent_layout/reconcile.rs` into the new file. Update imports:

- Drop `use vmux_service::protocol::layout::*` — replace with `use crate::protocol::{...}`.
- Keep the `#[cfg(test)] mod tests` block; its imports become `use super::*;` plus the protocol imports.

Top of the new file should look like:

```rust
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use crate::protocol::{Focus, LayoutNode, LayoutSnapshot, NodeKind, Tab, parse_id};

// ... rest of the original file (ValidationError enum, validate, plan_diff, etc.)
```

The body of `validate`, `validate_node`, `validate_tab`, `validate_focus`, `plan_diff`, `plan_node`, `NodeAction`, `DiffPlan`, `ValidationError` is unchanged.

- [ ] **Step 2: Register the module in `vmux_layout/src/lib.rs`**

```rust
pub mod reconcile;
```

- [ ] **Step 3: Have `vmux_desktop::agent_layout::reconcile` re-export from the new location**

Replace contents of `crates/vmux_desktop/src/agent_layout/reconcile.rs` with:

```rust
pub use vmux_layout::reconcile::{DiffPlan, NodeAction, ValidationError, plan_diff, validate};
```

This keeps `crates/vmux_desktop/src/agent_layout/apply.rs`'s `use super::reconcile::ValidationError;` working without other code changes.

- [ ] **Step 4: Verify**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

The tests that moved (validate_* / plan_*) run from their new home in `vmux_layout`.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/reconcile.rs \
        crates/vmux_layout/src/lib.rs \
        crates/vmux_desktop/src/agent_layout/reconcile.rs
git commit -m "refactor(layout): move validate + plan_diff into vmux_layout::reconcile"
```

---

### Task 7: Move `build_layout_snapshot` to `vmux_layout::snapshot` with URL-based kind

**Files:**
- Create: `crates/vmux_layout/src/snapshot.rs`
- Modify: `crates/vmux_layout/src/lib.rs`
- Modify: `crates/vmux_desktop/src/agent_layout.rs`
- Modify: `crates/vmux_desktop/src/agent_query.rs`

- [ ] **Step 1: Write a failing test for URL-based kind in the new location**

Create `crates/vmux_layout/src/snapshot.rs` with the test first:

```rust
use bevy::prelude::*;
use vmux_core::PageMetadata;

use crate::{
    pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection, Zoomed},
    protocol::{Focus, LayoutNode, LayoutSnapshot, NodeKind, Space, SplitDirection, Tab, format_id},
    stack::{FocusedStack, Stack},
    tab::Tab as SpaceTab,
};

pub fn build_layout_snapshot(
    spaces_q: &Query<(Entity, &SpaceTab, Option<&Children>)>,
    splits_q: &Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves_q: &Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks_q: &Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
    pane_sizes_q: &Query<&PaneSize>,
    zoomed_q: &Query<&Zoomed>,
    focused: &FocusedStack,
) -> LayoutSnapshot {
    // ... body adapted from vmux_desktop::agent_layout::build_layout_snapshot
    // (see Step 2)
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::stack_bundle;
    use crate::pane::leaf_pane_bundle;
    use crate::tab::tab_bundle;
    use vmux_history::LastActivatedAt;

    #[test]
    fn terminal_url_classifies_tab_as_terminal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(FocusedStack::default());

        let space = app.world_mut().spawn((SpaceTab { name: "S".into() },)).id();
        let leaf = app.world_mut().spawn((leaf_pane_bundle(), ChildOf(space))).id();
        let stack = app
            .world_mut()
            .spawn((
                stack_bundle(),
                LastActivatedAt::now(),
                ChildOf(leaf),
                PageMetadata {
                    url: "vmux://terminal/123".into(),
                    title: String::new(),
                    favicon_url: String::new(),
                    bg_color: None,
                },
            ))
            .id();
        let _ = stack;

        let snap = app.world_mut().run_system_once(
            |spaces_q: Query<(Entity, &SpaceTab, Option<&Children>)>,
             splits_q: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
             leaves_q: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
             stacks_q: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
             pane_sizes_q: Query<&PaneSize>,
             zoomed_q: Query<&Zoomed>,
             focused: Res<FocusedStack>| {
                build_layout_snapshot(
                    &spaces_q, &splits_q, &leaves_q, &stacks_q, &pane_sizes_q, &zoomed_q, &focused,
                )
            },
        ).unwrap();

        let LayoutNode::Pane { tabs, .. } = &snap.spaces[0].root else { panic!("expected pane root"); };
        assert_eq!(tabs[0].url, "vmux://terminal/123");
        // kind field is no longer in DTO (dropped later in plan); URL is the source of truth.
    }
}
```

> The test imports `run_system_once` from `bevy::ecs::system::RunSystemOnce`. Add that import as needed.

- [ ] **Step 2: Write the implementation**

Replace the `todo!()` body with the logic moved from `crates/vmux_desktop/src/agent_layout.rs::build_layout_snapshot` and `build_node`/`build_tab`. Key change: `build_tab` no longer takes a `terminals` query. Instead it classifies tabs by URL prefix:

```rust
fn build_tab(
    stack_entity: Entity,
    page: Option<&PageMetadata>,
) -> Tab {
    Tab {
        id: Some(format_id(NodeKind::Tab, stack_entity.to_bits())),
        title: page.map(|p| p.title.clone()).unwrap_or_default(),
        url: page.map(|p| p.url.clone()).unwrap_or_default(),
        is_loading: false,
        favicon_url: page.map(|p| p.favicon_url.clone()).unwrap_or_default(),
    }
}
```

The wrapping `build_layout_snapshot` + `build_node` logic is the same except for the dropped `terminals` parameter.

- [ ] **Step 3: Register the module in `vmux_layout/src/lib.rs`**

```rust
pub mod snapshot;
```

- [ ] **Step 4: Update `vmux_desktop::agent_layout` to delegate**

Replace contents of `crates/vmux_desktop/src/agent_layout.rs` with:

```rust
pub mod apply;
pub mod reconcile;

pub use vmux_layout::snapshot::build_layout_snapshot;
```

This keeps `crate::agent_layout::build_layout_snapshot` working for `agent_query.rs`.

- [ ] **Step 5: Update `agent_query.rs` to drop the `terminals` parameter**

In `crates/vmux_desktop/src/agent_query.rs`, remove the `terminals` query argument from `handle_agent_queries` and from the call to `build_layout_snapshot`:

```rust
pub(crate) fn handle_agent_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    spaces: Query<(Entity, &Tab, Option<&Children>)>,
    splits: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks: Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
    pane_sizes: Query<&PaneSize>,
    zoomed: Query<&Zoomed>,
    settings: Res<crate::settings::AppSettings>,
    focused: Option<Res<FocusedStack>>,
) {
    // ... drop terminals, drop `&terminals` from the build_layout_snapshot call
}
```

Drop the `use crate::terminal::Terminal;` import.

- [ ] **Step 6: Verify**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_layout/src/snapshot.rs \
        crates/vmux_layout/src/lib.rs \
        crates/vmux_desktop/src/agent_layout.rs \
        crates/vmux_desktop/src/agent_query.rs
git commit -m "refactor(layout): move snapshot to vmux_layout, classify tabs by URL"
```

---

### Task 8: Move `apply` logic to `vmux_layout::reconcile`

**Files:**
- Modify: `crates/vmux_layout/src/reconcile.rs`
- Modify: `crates/vmux_desktop/src/agent_layout/apply.rs`

- [ ] **Step 1: Move the apply functions and tests into `vmux_layout::reconcile`**

Copy the contents of `crates/vmux_desktop/src/agent_layout/apply.rs` (everything below the import block) to the end of `crates/vmux_layout/src/reconcile.rs`. Then apply these adjustments:

- `use crate::layout::{pane::{...}, stack::{Stack, stack_bundle}, tab::{Tab, tab_bundle}};` → `use crate::{pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection, leaf_pane_bundle, pane_split_gaps, split_root_bundle}, stack::{Stack, stack_bundle}, tab::{Tab as SpaceTab, tab_bundle}};`
- `use vmux_service::protocol::layout::{...}` → `use crate::protocol::{Focus, LayoutNode, LayoutSnapshot, NodeKind, Space, SplitDirection, Tab, format_id, parse_id};`
- All component references to the `Tab` *component* must use `SpaceTab` (since the local `Tab` import refers to the DTO).
- `crate::layout::stack::FocusedStack` → `crate::stack::FocusedStack`.
- `vmux_layout::LayoutSpawnRequest` → `crate::LayoutSpawnRequest`.
- The reconciler's local `spawn_split` function — **delete** it. Replace its call site in `create_descendants` with `world.spawn((split_root_bundle(pane_split_dir), LastActivatedAt::now(), ChildOf(parent))).id()`.

The functions to move: `apply`, `apply_with_existing`, `create_descendants`, `spawn_leaf_pane`, `spawn_tab`, `apply_close`, `collect_existing_ids`, `apply_space`, `apply_structure`, `resolve_node_entity`, `apply_node`, `apply_focus`, `node_entity`, `find_root_split_child`, `set_split_direction`.

The full `#[cfg(test)] mod tests` block from `apply.rs` moves alongside, with imports adjusted the same way.

- [ ] **Step 2: Make `vmux_desktop::agent_layout::apply` a re-export shim**

Replace contents of `crates/vmux_desktop/src/agent_layout/apply.rs` with:

```rust
pub use vmux_layout::reconcile::{apply, apply_with_existing};
```

This keeps `crates/vmux_desktop/src/agent.rs::ServiceAgentCommand::UpdateLayout` working (`crate::agent_layout::apply::apply(world, &layout)`) — for now. Task 11 thins it.

- [ ] **Step 3: Verify**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

All ~27 apply tests should now run from `vmux_layout::reconcile::tests`.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/src/reconcile.rs \
        crates/vmux_desktop/src/agent_layout/apply.rs
git commit -m "refactor(layout): move apply reconciler into vmux_layout::reconcile"
```

---

## Phase D — Add the message boundary

### Task 9: Define the four `Message` types in `vmux_layout::reconcile`

**Files:**
- Modify: `crates/vmux_layout/src/reconcile.rs`

- [ ] **Step 1: Add the message types**

Add at the top of `crates/vmux_layout/src/reconcile.rs` (after the `use` block):

```rust
#[derive(Message, Clone)]
pub struct LayoutApplyRequest {
    pub request_id: u64,
    pub snapshot: LayoutSnapshot,
}

#[derive(Message, Clone)]
pub struct LayoutApplyResponse {
    pub request_id: u64,
    pub result: Result<LayoutSnapshot, String>,
}

#[derive(Message, Clone)]
pub struct LayoutSnapshotRequest {
    pub request_id: u64,
}

#[derive(Message, Clone)]
pub struct LayoutSnapshotResponse {
    pub request_id: u64,
    pub snapshot: LayoutSnapshot,
}
```

> The `bevy::prelude::Message` derive comes from `use bevy::prelude::*;` already present.

- [ ] **Step 2: Verify build (no behavior change yet)**

```bash
env -u CEF_PATH cargo build -p vmux_layout
```

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_layout/src/reconcile.rs
git commit -m "feat(layout): add Layout{Apply,Snapshot}{Request,Response} messages"
```

---

### Task 10: Add `apply_layout_requests` and `serve_snapshot_requests` systems

**Files:**
- Modify: `crates/vmux_layout/src/reconcile.rs`
- Modify: `crates/vmux_layout/src/lib.rs`

- [ ] **Step 1: Write a failing test that asserts the apply system round-trips**

Add to the `tests` module in `reconcile.rs`:

```rust
#[test]
fn apply_layout_requests_emits_response_with_snapshot() {
    use bevy::ecs::message::Messages;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_message::<LayoutApplyRequest>();
    app.add_message::<LayoutApplyResponse>();
    app.add_message::<LayoutSpawnRequest>();
    app.insert_resource(crate::stack::FocusedStack::default());
    app.add_systems(Update, super::apply_layout_requests);

    let space = app.world_mut().spawn((SpaceTab { name: "S".into() },)).id();
    let pane = app.world_mut().spawn((leaf_pane_bundle(), ChildOf(space))).id();

    let snap = LayoutSnapshot {
        spaces: vec![Space {
            id: Some(format_id(NodeKind::Space, space.to_bits())),
            name: "S".into(),
            is_active: true,
            root: LayoutNode::Pane {
                id: Some(format_id(NodeKind::Pane, pane.to_bits())),
                is_zoomed: false,
                tabs: vec![],
            },
        }],
        focused: Focus::default(),
    };

    app.world_mut().resource_mut::<Messages<LayoutApplyRequest>>().send(LayoutApplyRequest {
        request_id: 42,
        snapshot: snap.clone(),
    });
    app.update();

    let responses = app.world().resource::<Messages<LayoutApplyResponse>>();
    let mut cursor = responses.get_cursor();
    let response = cursor.read(responses).next().expect("expected one response");
    assert_eq!(response.request_id, 42);
    assert!(response.result.is_ok(), "apply should succeed");
}
```

Run it — expect FAIL ("function not found" `apply_layout_requests`).

- [ ] **Step 2: Implement `apply_layout_requests`**

Add inside `crates/vmux_layout/src/reconcile.rs`:

```rust
pub fn apply_layout_requests(
    mut reader: MessageReader<LayoutApplyRequest>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let snapshot = request.snapshot.clone();
        let request_id = request.request_id;
        commands.queue(move |world: &mut World| {
            let result = match apply(world, &snapshot) {
                Ok(()) => {
                    let snapshot = run_build_snapshot(world);
                    Ok(snapshot)
                }
                Err(err) => Err(format!("update_layout: {err:?}")),
            };
            world
                .resource_mut::<Messages<LayoutApplyResponse>>()
                .send(LayoutApplyResponse { request_id, result });
        });
    }
}

fn run_build_snapshot(world: &mut World) -> LayoutSnapshot {
    let mut state = SystemState::<(
        Query<(Entity, &SpaceTab, Option<&Children>)>,
        Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
        Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
        Query<(Entity, Option<&Children>, Option<&vmux_core::PageMetadata>), With<Stack>>,
        Query<&PaneSize>,
        Query<&crate::pane::Zoomed>,
        Res<crate::stack::FocusedStack>,
    )>::new(world);
    let (spaces, splits, leaves, stacks, pane_sizes, zoomed, focused) = state.get(world);
    crate::snapshot::build_layout_snapshot(
        &spaces,
        &splits,
        &leaves,
        &stacks,
        &pane_sizes,
        &zoomed,
        &focused,
    )
}
```

> `SystemState` lets us run the snapshot queries from within an exclusive World closure. Import: `use bevy::ecs::system::SystemState;`.

- [ ] **Step 3: Run the test — expect PASS**

```bash
env -u CEF_PATH cargo test -p vmux_layout reconcile::tests::apply_layout_requests
```

- [ ] **Step 4: Write a failing test for `serve_snapshot_requests`**

```rust
#[test]
fn serve_snapshot_requests_emits_response() {
    use bevy::ecs::message::Messages;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_message::<LayoutSnapshotRequest>();
    app.add_message::<LayoutSnapshotResponse>();
    app.insert_resource(crate::stack::FocusedStack::default());
    app.add_systems(Update, super::serve_snapshot_requests);

    let space = app.world_mut().spawn((SpaceTab { name: "S".into() },)).id();
    let _ = app.world_mut().spawn((leaf_pane_bundle(), ChildOf(space))).id();

    app.world_mut().resource_mut::<Messages<LayoutSnapshotRequest>>().send(LayoutSnapshotRequest {
        request_id: 7,
    });
    app.update();

    let responses = app.world().resource::<Messages<LayoutSnapshotResponse>>();
    let mut cursor = responses.get_cursor();
    let response = cursor.read(responses).next().expect("expected one response");
    assert_eq!(response.request_id, 7);
    assert_eq!(response.snapshot.spaces.len(), 1);
}
```

- [ ] **Step 5: Implement `serve_snapshot_requests`**

```rust
pub fn serve_snapshot_requests(
    mut reader: MessageReader<LayoutSnapshotRequest>,
    spaces_q: Query<(Entity, &SpaceTab, Option<&Children>)>,
    splits_q: Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves_q: Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks_q: Query<(Entity, Option<&Children>, Option<&vmux_core::PageMetadata>), With<Stack>>,
    pane_sizes_q: Query<&PaneSize>,
    zoomed_q: Query<&crate::pane::Zoomed>,
    focused: Res<crate::stack::FocusedStack>,
    mut writer: MessageWriter<LayoutSnapshotResponse>,
) {
    for request in reader.read() {
        let snapshot = crate::snapshot::build_layout_snapshot(
            &spaces_q, &splits_q, &leaves_q, &stacks_q, &pane_sizes_q, &zoomed_q, &focused,
        );
        writer.send(LayoutSnapshotResponse {
            request_id: request.request_id,
            snapshot,
        });
    }
}
```

- [ ] **Step 6: Run the test — expect PASS**

```bash
env -u CEF_PATH cargo test -p vmux_layout reconcile::tests::serve_snapshot_requests
```

- [ ] **Step 7: Verify the whole crate**

```bash
cargo fmt -p vmux_layout -- --check
env -u CEF_PATH cargo clippy -p vmux_layout --all-targets -- -D warnings
env -u CEF_PATH cargo test -p vmux_layout
```

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_layout/src/reconcile.rs
git commit -m "feat(layout): add apply_layout_requests + serve_snapshot_requests systems"
```

---

### Task 11: Register messages + systems in `LayoutPlugin`

**Files:**
- Modify: `crates/vmux_layout/src/lib.rs`

- [ ] **Step 1: Wire the four messages and two systems into `LayoutPlugin`**

In `impl Plugin for LayoutPlugin::build`, extend the message and system registration:

```rust
fn build(&self, app: &mut App) {
    app.register_type::<Open>();
    app.init_resource::<NewStackContext>()
        .init_resource::<settings::ConfirmCloseSettings>()
        .add_message::<LayoutSpawnRequest>()
        .add_message::<reconcile::LayoutApplyRequest>()
        .add_message::<reconcile::LayoutApplyResponse>()
        .add_message::<reconcile::LayoutSnapshotRequest>()
        .add_message::<reconcile::LayoutSnapshotResponse>()
        .configure_sets(
            Startup,
            (
                LayoutStartupSet::Window,
                LayoutStartupSet::Persistence,
                LayoutStartupSet::DefaultSpace,
                LayoutStartupSet::Post,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                reconcile::apply_layout_requests,
                reconcile::serve_snapshot_requests,
            ),
        );
    // ... rest unchanged
}
```

- [ ] **Step 2: Verify**

```bash
env -u CEF_PATH cargo test -p vmux_layout
env -u CEF_PATH cargo test -p vmux_desktop
```

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_layout/src/lib.rs
git commit -m "feat(layout): register layout reconcile messages + systems in LayoutPlugin"
```

---

## Phase E — Wire `vmux_desktop` to the message boundary

### Task 12: Thin `ServiceAgentCommand::UpdateLayout` dispatch in `agent.rs`

**Files:**
- Modify: `crates/vmux_desktop/src/agent.rs`

- [ ] **Step 1: Change the handler to write `LayoutApplyRequest`**

Locate the `ServiceAgentCommand::UpdateLayout { layout }` arm in `handle_agent_commands` (around line 1118). Replace the `commands.queue(...)` block with a `MessageWriter<LayoutApplyRequest>` write:

```rust
ServiceAgentCommand::UpdateLayout { layout } => {
    layout_apply_writer.write(vmux_layout::reconcile::LayoutApplyRequest {
        request_id: request.request_id.0,
        snapshot: layout.clone(),
    });
    continue;
}
```

> The `MessageWriter` parameter (`mut layout_apply_writer: MessageWriter<vmux_layout::reconcile::LayoutApplyRequest>`) is added to the `handle_agent_commands` system signature. `AgentRequestId(u64)` unwraps to `u64` via `.0`.

- [ ] **Step 2: Verify**

```bash
env -u CEF_PATH cargo build -p vmux_desktop
```

(The response side is wired next task; tests for the full round-trip come at the end.)

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/agent.rs
git commit -m "refactor(desktop): dispatch UpdateLayout via LayoutApplyRequest message"
```

---

### Task 13: Thin `AgentQuery::ReadLayout` dispatch in `agent_query.rs`

**Files:**
- Modify: `crates/vmux_desktop/src/agent_query.rs`

- [ ] **Step 1: Split the handler — keep settings, route layout to messages**

Replace the body of `handle_agent_queries` with:

```rust
pub(crate) fn handle_agent_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    settings: Res<crate::settings::AppSettings>,
    mut layout_snapshot_writer: MessageWriter<vmux_layout::reconcile::LayoutSnapshotRequest>,
) {
    let Some(service) = service else { return };

    for request in reader.read() {
        match request.query {
            AgentQuery::ReadLayout => {
                layout_snapshot_writer.write(vmux_layout::reconcile::LayoutSnapshotRequest {
                    request_id: request.request_id.0,
                });
            }
            AgentQuery::GetSettings => {
                let result = AgentQueryResult::Settings(
                    crate::settings::serialize_settings_to_json(&settings),
                );
                service.0.send(ClientMessage::AgentQueryResponse {
                    request_id: request.request_id,
                    result,
                });
            }
        }
    }
}
```

Drop the now-unused query parameters (`spaces`, `splits`, `leaves`, `stacks`, `terminals`, `pane_sizes`, `zoomed`, `focused`) and their `use` statements.

- [ ] **Step 2: Verify**

```bash
env -u CEF_PATH cargo build -p vmux_desktop
```

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_desktop/src/agent_query.rs
git commit -m "refactor(desktop): dispatch ReadLayout via LayoutSnapshotRequest message"
```

---

### Task 14: Forward layout responses back to the service client

**Files:**
- Create: `crates/vmux_desktop/src/layout_response.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`

- [ ] **Step 1: Create the forwarding system**

Create `crates/vmux_desktop/src/layout_response.rs`:

```rust
use bevy::prelude::*;
use vmux_layout::reconcile::{LayoutApplyResponse, LayoutSnapshotResponse};
use vmux_service::protocol::{
    AgentCommandResult, AgentQueryResult, AgentRequestId, ClientMessage,
};

use crate::terminal::ServiceClient;

pub(crate) struct LayoutResponseForwarderPlugin;

impl Plugin for LayoutResponseForwarderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (forward_layout_apply_responses, forward_layout_snapshot_responses),
        );
    }
}

fn forward_layout_apply_responses(
    mut reader: MessageReader<LayoutApplyResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        let result = match response.result.clone() {
            Ok(snapshot) => AgentCommandResult::Layout(snapshot),
            Err(message) => AgentCommandResult::Error(message),
        };
        service.0.send(ClientMessage::AgentCommandResponse {
            request_id: AgentRequestId(response.request_id),
            result,
        });
    }
}

fn forward_layout_snapshot_responses(
    mut reader: MessageReader<LayoutSnapshotResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: AgentQueryResult::Layout(response.snapshot.clone()),
        });
    }
}
```

- [ ] **Step 2: Register the plugin in `vmux_desktop/src/lib.rs`**

Add the `mod layout_response;` declaration and include `LayoutResponseForwarderPlugin` in the plugin list assembled in `VmuxPlugin::build`:

```rust
mod layout_response;
// ... in the .add_plugins((...)) block:
layout_response::LayoutResponseForwarderPlugin,
```

- [ ] **Step 3: Verify**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_desktop/src/layout_response.rs crates/vmux_desktop/src/lib.rs
git commit -m "feat(desktop): forward Layout*Response messages to service client"
```

---

## Phase F — Cleanup

### Task 15: Delete `vmux_desktop::agent_layout/` directory and `agent_layout.rs`

**Files:**
- Delete: `crates/vmux_desktop/src/agent_layout/`
- Delete: `crates/vmux_desktop/src/agent_layout.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`
- Modify: `crates/vmux_desktop/src/agent.rs` (drop the now-dead `crate::agent_layout::apply::apply` import)

- [ ] **Step 1: Remove the module declaration from `lib.rs`**

In `crates/vmux_desktop/src/lib.rs`, delete the line:

```rust
mod agent_layout;
```

- [ ] **Step 2: Delete the files**

```bash
git rm crates/vmux_desktop/src/agent_layout/apply.rs
git rm crates/vmux_desktop/src/agent_layout/reconcile.rs
git rm crates/vmux_desktop/src/agent_layout.rs
rmdir crates/vmux_desktop/src/agent_layout/ 2>/dev/null || true
```

- [ ] **Step 3: Drop any now-unused imports in `agent.rs`**

In `crates/vmux_desktop/src/agent.rs`, search for `agent_layout` and remove any remaining `use crate::agent_layout::...` lines.

- [ ] **Step 4: Verify**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

- [ ] **Step 5: Commit**

```bash
git add -u crates/vmux_desktop/src/lib.rs crates/vmux_desktop/src/agent.rs
git commit -m "refactor(desktop): delete vestigial agent_layout module"
```

---

### Task 16: Drop the `vmux_desktop::layout` re-export shim

**Files:**
- Delete: `crates/vmux_desktop/src/layout.rs`
- Modify: `crates/vmux_desktop/src/lib.rs`
- Modify: every file under `crates/vmux_desktop/src/` that imports `use crate::layout::*;` or `use crate::layout::{...}`

- [ ] **Step 1: Find every caller**

```bash
rg -l 'crate::layout' crates/vmux_desktop/src/
```

- [ ] **Step 2: Rewrite imports**

For each file in the list, change:

```rust
use crate::layout::{pane::Pane, stack::FocusedStack, tab::Tab, ...};
```

to:

```rust
use vmux_layout::{pane::Pane, stack::FocusedStack, tab::Tab, ...};
```

> The re-exports in `vmux_desktop/src/layout.rs` already pointed at `vmux_layout::{...}`, so the items themselves are unchanged — only the path the importer uses.

- [ ] **Step 3: Delete the shim**

```bash
git rm crates/vmux_desktop/src/layout.rs
```

- [ ] **Step 4: Remove the module declaration from `lib.rs`**

In `crates/vmux_desktop/src/lib.rs`, delete `mod layout;` and any `use {layout::LayoutPlugin, ...}` reference (it now comes via `vmux_layout::LayoutPlugin`).

- [ ] **Step 5: Verify**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

- [ ] **Step 6: Commit**

Stage every file that was edited in steps 2 and 4 (the list from `rg -l 'crate::layout' crates/vmux_desktop/src/` plus `crates/vmux_desktop/src/lib.rs`):

```bash
git add -u crates/vmux_desktop/src/
git rm crates/vmux_desktop/src/layout.rs 2>/dev/null || true
git commit -m "refactor(desktop): drop layout re-export shim, import vmux_layout directly"
```

> `git add -u` stages tracked-file modifications/deletions only — won't pick up untracked sensitive files.

---

### Task 17: Drop the `kind` field from the MCP `update_layout` schema

**Files:**
- Modify: `crates/vmux_mcp/src/tools.rs`

- [ ] **Step 1: Remove the `kind` property from the Tab schema**

In `update_layout_definition()`'s `input_schema`, find the `Tab` `$def`:

```rust
"Tab": {
    "type": "object",
    "properties": {
        "id": {"type": "string", "description": "tab:<id>; omit to create"},
        "title": {"type": "string"},
        "url": {"type": "string", "description": "Required when id is omitted"},
        "kind": {"type": "string", "enum": ["browser", "terminal"], "description": "Required when id is omitted"},
        "is_loading": {"type": "boolean"},
        "favicon_url": {"type": "string"}
    }
}
```

Remove the `"kind"` line.

- [ ] **Step 2: Update the description text**

In the tool description string, find:

```
- Identifiers use kind:value format (space:N, pane:N, split:N, tab:N). Omit id to create a new node; a new tab needs url+kind, a new pane needs at least one tab, a new space needs name.
```

Change to:

```
- Identifiers use kind:value format (space:N, pane:N, split:N, tab:N). Omit id to create a new node; a new tab needs url (use vmux://terminal/ for a terminal, anything else loads as a browser), a new pane needs at least one tab, a new space needs name.
```

- [ ] **Step 3: Verify**

```bash
env -u CEF_PATH cargo test -p vmux_mcp
```

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_mcp/src/tools.rs
git commit -m "feat(mcp): drop redundant 'kind' field from update_layout schema"
```

---

### Task 18: Final end-to-end verification + plan cleanup

**Files:**
- Delete: `docs/plans/2026-05-18-vmux-layout-message-boundary.md`

- [ ] **Step 1: Run lint/test on every changed crate**

```bash
PKGS=$(BASE=origin/main ./scripts/changed-crates.sh)
for pkg in $PKGS; do cargo fmt -p "$pkg" -- --check; done
for pkg in $PKGS; do env -u CEF_PATH cargo clippy -p "$pkg" --all-targets -- -D warnings; done
for pkg in $PKGS; do env -u CEF_PATH cargo test -p "$pkg"; done
```

- [ ] **Step 2: Manual UI check**

Launch the desktop binary and trigger an MCP `update_layout` from the Vibe agent ("open google on right"). Verify:
- Google renders in the right pane after a single `update_layout` call.
- Vibe prompt still accepts input (focus chain intact).
- No B0004 hierarchy warnings in the log.

- [ ] **Step 3: Delete the plan file**

Per AGENTS.md: "Delete the plan file once the plan is fully implemented."

```bash
git rm docs/plans/2026-05-18-vmux-layout-message-boundary.md
git commit -m "chore: remove implemented plan"
```

---

## Done

The repo now has:
- `vmux_layout` owns the reconciler, snapshot, DTOs, and Bevy messages.
- `vmux_desktop` no longer contains layout logic; `agent.rs` is a thin dispatcher; `agent_layout/` and `layout.rs` shim are gone.
- One `split_root_bundle` factory used by both `split_pane_in_two` and the reconciler — no more drift between hand-rolled component sets.
- URL prefix is the source of truth for tab kind; the `Terminal`-marker coupling is gone.
