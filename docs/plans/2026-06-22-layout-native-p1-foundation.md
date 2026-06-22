# Native Layout Renderer — P1 Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **CEF caveat:** building/testing `vmux_layout` compiles the CEF dependency graph — use a warm target dir and run **inline**, do NOT subagent-drive (long agents drop sockets mid-CEF-build).

**Goal:** Build the pure-Rust foundation for the native AppKit layout renderer — the `LayoutView` render model, a keyed reconciler that diffs it into `ViewOp`s, and a `LayoutRenderer` flag — all in `vmux_layout`, fully unit/integration-tested on Linux.

**Architecture:** The ECS already produces a `LayoutSnapshot` (`protocol.rs`). P1 adds a `LayoutView` (a render-oriented projection of that snapshot), a `diff_tabs` reconciler that turns two `LayoutView`s into a minimal list of `ViewOp`s keyed by the snapshot's stable ids, and Bevy resources/systems that record the latest op batch. No objc2, no macOS APIs, no ECS-hierarchy queries — those land in P2 (the macOS applier + the ECS→`LayoutView` producer).

**Tech Stack:** Rust, Bevy ECS (0.19), existing `vmux_layout::protocol` types. Tests use `bevy::MinimalPlugins`.

---

## Scope

P1 is the bottom slice of the spec `docs/specs/2026-06-22-layout-native-appkit-design.md` (§ components 1–3, § migration P1). It delivers:

- The `LayoutRenderer` flag (Resource, default `Cef`).
- The `LayoutView` model + `from_snapshot` projection.
- The `ViewOp` enum + `diff_tabs` reconciler (tab strip only — the recursive pane-tree diff lands in P2 alongside real NSViews).
- Bevy resources + a `diff_into_ops` system + `NativeViewPlugin` wiring, integration-tested.

Explicitly **deferred to P2** (separate plan): the ECS→`LayoutView` producer (`update_current_layout_view` via `build_layout_snapshot`, needs the live entity hierarchy), the recursive pane-tree diff, and the macOS `vmux_desktop::layout_native` applier that turns `ViewOp`s into `NSGlassEffectView`/`NSView`s.

Decision locked here (spec Open Q2): **`LayoutView` lives in `vmux_layout`** (it is snapshot-derived; `vmux_desktop` already depends on `vmux_layout`, so the macOS applier in P2 consumes a public type with no reverse coupling).

## File Structure

Create:
- `crates/vmux_layout/src/native_view.rs` — everything in P1: `LayoutRenderer`, `NodeId`, `LayoutView`/`TabView`, `from_snapshot`, `ViewOp`, `diff_tabs`, the P1 resources, `diff_into_ops`, `NativeViewPlugin`, and the `#[cfg(test)] mod tests`.

Modify:
- `crates/vmux_layout/src/lib.rs` — add `pub mod native_view;` and re-export the plugin/public types.
- `crates/vmux_layout/src/plugin.rs` — add `NativeViewPlugin` to the layout plugin group (so resources/systems are registered in the real app).

One file keeps P1 cohesive (model + reconciler + wiring change together and are small). P2 will split `vmux_desktop/src/layout_native/` into focused files (views, glass, applier) as it grows.

---

## Task 1: `LayoutRenderer` flag

**Files:**
- Create: `crates/vmux_layout/src/native_view.rs`
- Test: in-file `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing test**

Put this at the bottom of `native_view.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_renderer_defaults_to_cef() {
        assert_eq!(LayoutRenderer::default(), LayoutRenderer::Cef);
    }

    #[test]
    fn parse_renderer_only_native_string_selects_native() {
        assert_eq!(parse_renderer("native"), LayoutRenderer::Native);
        assert_eq!(parse_renderer("cef"), LayoutRenderer::Cef);
        assert_eq!(parse_renderer(""), LayoutRenderer::Cef);
        assert_eq!(parse_renderer("NATIVE"), LayoutRenderer::Cef);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout native_view::tests::layout_renderer_defaults_to_cef`
Expected: FAIL — `native_view` module does not exist / `LayoutRenderer` not found.

- [ ] **Step 3: Write minimal implementation**

At the top of `native_view.rs`:

```rust
use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutRenderer {
    #[default]
    Cef,
    Native,
}

pub fn parse_renderer(s: &str) -> LayoutRenderer {
    match s {
        "native" => LayoutRenderer::Native,
        _ => LayoutRenderer::Cef,
    }
}

impl LayoutRenderer {
    pub fn from_env() -> Self {
        parse_renderer(std::env::var("VMUX_LAYOUT_RENDERER").as_deref().unwrap_or(""))
    }
}
```

Then add the module to the crate. In `crates/vmux_layout/src/lib.rs`:

```rust
pub mod native_view;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_layout native_view::tests`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/native_view.rs crates/vmux_layout/src/lib.rs
git commit -m "feat(layout): add LayoutRenderer flag for native renderer migration"
```

---

## Task 2: `LayoutView` model + `from_snapshot`

**Files:**
- Modify: `crates/vmux_layout/src/native_view.rs`
- Test: in-file `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing tests**

Add to `mod tests`:

```rust
use crate::protocol::{Focus, LayoutNode, LayoutSnapshot, Tab};

fn tab(id: &str, name: &str, is_active: bool) -> Tab {
    Tab {
        id: Some(id.into()),
        name: name.into(),
        is_active,
        root: LayoutNode::Pane { id: Some("pane:1".into()), is_zoomed: false, stacks: vec![] },
    }
}

#[test]
fn from_snapshot_projects_tabs_in_order() {
    let snapshot = LayoutSnapshot {
        tabs: vec![tab("tab:1", "A", true), tab("tab:2", "B", false)],
        focused: Focus::default(),
    };
    let view = LayoutView::from_snapshot(&snapshot);
    assert_eq!(
        view.tabs,
        vec![
            TabView { id: NodeId("tab:1".into()), name: "A".into(), is_active: true },
            TabView { id: NodeId("tab:2".into()), name: "B".into(), is_active: false },
        ]
    );
}

#[test]
fn from_snapshot_skips_tabs_without_id() {
    let mut t = tab("tab:1", "A", true);
    t.id = None;
    let snapshot = LayoutSnapshot { tabs: vec![t], focused: Focus::default() };
    assert!(LayoutView::from_snapshot(&snapshot).tabs.is_empty());
}

#[test]
fn layout_view_default_is_empty() {
    assert!(LayoutView::default().tabs.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_layout native_view::tests::from_snapshot_projects_tabs_in_order`
Expected: FAIL — `LayoutView` / `TabView` / `NodeId` not found.

- [ ] **Step 3: Write minimal implementation**

Add to `native_view.rs` (after the `LayoutRenderer` block):

```rust
use crate::protocol::LayoutSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(pub String);

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutView {
    pub tabs: Vec<TabView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabView {
    pub id: NodeId,
    pub name: String,
    pub is_active: bool,
}

impl LayoutView {
    pub fn from_snapshot(snapshot: &LayoutSnapshot) -> Self {
        let tabs = snapshot
            .tabs
            .iter()
            .filter_map(|t| {
                let id = t.id.clone()?;
                Some(TabView { id: NodeId(id), name: t.name.clone(), is_active: t.is_active })
            })
            .collect();
        LayoutView { tabs }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_layout native_view::tests`
Expected: PASS (5 tests total).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/native_view.rs
git commit -m "feat(layout): add LayoutView render model projected from LayoutSnapshot"
```

---

## Task 3: `ViewOp` + `diff_tabs` reconciler

**Files:**
- Modify: `crates/vmux_layout/src/native_view.rs`
- Test: in-file `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing tests**

Add to `mod tests`:

```rust
fn view(tabs: &[(&str, &str, bool)]) -> LayoutView {
    LayoutView {
        tabs: tabs
            .iter()
            .map(|(id, name, active)| TabView {
                id: NodeId((*id).into()),
                name: (*name).into(),
                is_active: *active,
            })
            .collect(),
    }
}

#[test]
fn diff_no_change_emits_nothing() {
    let v = view(&[("tab:1", "A", true)]);
    assert!(diff_tabs(&v, &v).is_empty());
}

#[test]
fn diff_added_tab_emits_create() {
    let old = view(&[("tab:1", "A", true)]);
    let new = view(&[("tab:1", "A", true), ("tab:2", "B", false)]);
    assert_eq!(
        diff_tabs(&old, &new),
        vec![ViewOp::CreateTab { id: NodeId("tab:2".into()), name: "B".into(), is_active: false }]
    );
}

#[test]
fn diff_removed_tab_emits_remove() {
    let old = view(&[("tab:1", "A", true), ("tab:2", "B", false)]);
    let new = view(&[("tab:1", "A", true)]);
    assert_eq!(diff_tabs(&old, &new), vec![ViewOp::RemoveTab { id: NodeId("tab:2".into()) }]);
}

#[test]
fn diff_renamed_or_activated_tab_emits_update() {
    let old = view(&[("tab:1", "A", false)]);
    let new = view(&[("tab:1", "A2", true)]);
    assert_eq!(
        diff_tabs(&old, &new),
        vec![ViewOp::UpdateTab { id: NodeId("tab:1".into()), name: "A2".into(), is_active: true }]
    );
}

#[test]
fn diff_reorder_emits_set_order_only() {
    let old = view(&[("tab:1", "A", true), ("tab:2", "B", false)]);
    let new = view(&[("tab:2", "B", false), ("tab:1", "A", true)]);
    assert_eq!(
        diff_tabs(&old, &new),
        vec![ViewOp::SetTabOrder {
            ids: vec![NodeId("tab:2".into()), NodeId("tab:1".into())]
        }]
    );
}

#[test]
fn diff_orders_ops_remove_before_create() {
    let old = view(&[("tab:1", "A", true)]);
    let new = view(&[("tab:2", "B", true)]);
    assert_eq!(
        diff_tabs(&old, &new),
        vec![
            ViewOp::RemoveTab { id: NodeId("tab:1".into()) },
            ViewOp::CreateTab { id: NodeId("tab:2".into()), name: "B".into(), is_active: true },
            ViewOp::SetTabOrder { ids: vec![NodeId("tab:2".into())] },
        ]
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_layout native_view::tests::diff_added_tab_emits_create`
Expected: FAIL — `ViewOp` / `diff_tabs` not found.

- [ ] **Step 3: Write minimal implementation**

Add to `native_view.rs`:

```rust
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum ViewOp {
    CreateTab { id: NodeId, name: String, is_active: bool },
    UpdateTab { id: NodeId, name: String, is_active: bool },
    RemoveTab { id: NodeId },
    SetTabOrder { ids: Vec<NodeId> },
}

pub fn diff_tabs(old: &LayoutView, new: &LayoutView) -> Vec<ViewOp> {
    let mut ops = Vec::new();
    let new_ids: HashSet<&NodeId> = new.tabs.iter().map(|t| &t.id).collect();

    for t in &old.tabs {
        if !new_ids.contains(&t.id) {
            ops.push(ViewOp::RemoveTab { id: t.id.clone() });
        }
    }
    for t in &new.tabs {
        match old.tabs.iter().find(|o| o.id == t.id) {
            None => ops.push(ViewOp::CreateTab {
                id: t.id.clone(),
                name: t.name.clone(),
                is_active: t.is_active,
            }),
            Some(o) => {
                if o.name != t.name || o.is_active != t.is_active {
                    ops.push(ViewOp::UpdateTab {
                        id: t.id.clone(),
                        name: t.name.clone(),
                        is_active: t.is_active,
                    });
                }
            }
        }
    }
    let old_order: Vec<&NodeId> = old.tabs.iter().map(|t| &t.id).collect();
    let new_order: Vec<&NodeId> = new.tabs.iter().map(|t| &t.id).collect();
    if old_order != new_order {
        ops.push(ViewOp::SetTabOrder { ids: new.tabs.iter().map(|t| t.id.clone()).collect() });
    }
    ops
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_layout native_view::tests`
Expected: PASS (11 tests total).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/native_view.rs
git commit -m "feat(layout): add keyed tab-strip reconciler (LayoutView diff -> ViewOp)"
```

---

## Task 4: Resources + `diff_into_ops` system + `NativeViewPlugin`

**Files:**
- Modify: `crates/vmux_layout/src/native_view.rs`, `crates/vmux_layout/src/plugin.rs`
- Test: in-file `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing test**

Add to `mod tests`:

```rust
#[test]
fn diff_into_ops_records_create_then_update_across_changes() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(NativeViewPlugin);

    app.world_mut().resource_mut::<CurrentLayoutView>().0 = view(&[("tab:1", "A", true)]);
    app.update();
    assert_eq!(
        app.world().resource::<RecordedViewOps>().0,
        vec![ViewOp::CreateTab { id: NodeId("tab:1".into()), name: "A".into(), is_active: true }]
    );

    app.world_mut().resource_mut::<CurrentLayoutView>().0 = view(&[("tab:1", "B", true)]);
    app.update();
    assert_eq!(
        app.world().resource::<RecordedViewOps>().0,
        vec![ViewOp::UpdateTab { id: NodeId("tab:1".into()), name: "B".into(), is_active: true }]
    );
}

#[test]
fn diff_into_ops_idle_when_unchanged() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(NativeViewPlugin);
    app.world_mut().resource_mut::<CurrentLayoutView>().0 = view(&[("tab:1", "A", true)]);
    app.update();
    app.world_mut().resource_mut::<RecordedViewOps>().0.clear();
    app.update();
    assert!(app.world().resource::<RecordedViewOps>().0.is_empty());
}

#[test]
fn native_view_plugin_registers_default_renderer() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(NativeViewPlugin);
    assert_eq!(*app.world().resource::<LayoutRenderer>(), LayoutRenderer::Cef);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_layout native_view::tests::diff_into_ops_records_create_then_update_across_changes`
Expected: FAIL — `CurrentLayoutView` / `RecordedViewOps` / `NativeViewPlugin` not found.

- [ ] **Step 3: Write minimal implementation**

Add to `native_view.rs`:

```rust
#[derive(Resource, Default)]
pub struct CurrentLayoutView(pub LayoutView);

#[derive(Resource, Default)]
pub struct LastRenderedView(pub Option<LayoutView>);

#[derive(Resource, Default)]
pub struct RecordedViewOps(pub Vec<ViewOp>);

pub fn diff_into_ops(
    current: Res<CurrentLayoutView>,
    mut last: ResMut<LastRenderedView>,
    mut recorded: ResMut<RecordedViewOps>,
) {
    if !current.is_changed() {
        return;
    }
    let empty = LayoutView::default();
    let prev = last.0.as_ref().unwrap_or(&empty);
    let ops = diff_tabs(prev, &current.0);
    if ops.is_empty() {
        return;
    }
    recorded.0 = ops;
    last.0 = Some(current.0.clone());
}

pub struct NativeViewPlugin;

impl Plugin for NativeViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LayoutRenderer>()
            .init_resource::<CurrentLayoutView>()
            .init_resource::<LastRenderedView>()
            .init_resource::<RecordedViewOps>()
            .add_systems(Update, diff_into_ops);
    }
}
```

Note: `Res::is_changed()` is `true` on the first `update()` after `init_resource` only if the value was touched; the tests touch `CurrentLayoutView` via `resource_mut` before each `update`, so the change flag is set as expected. The `ops.is_empty()` guard keeps `diff_into_ops` from overwriting `RecordedViewOps` when a change produced no ops (used by `diff_into_ops_idle_when_unchanged` after the manual clear + an unchanged frame).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_layout native_view::tests`
Expected: PASS (14 tests total).

- [ ] **Step 5: Register the plugin in the real app**

In `crates/vmux_layout/src/plugin.rs`, add `NativeViewPlugin` to the layout plugin group (alongside the other `add_plugins(...)` calls — match the existing chaining style). Example shape (adapt to the actual group/builder in that file):

```rust
use crate::native_view::NativeViewPlugin;
// ... inside the plugin's build(), in the existing add_plugins chain:
app.add_plugins(NativeViewPlugin);
```

- [ ] **Step 6: Verify the crate builds with the plugin wired**

Run: `cargo build -p vmux_layout`
Expected: builds clean (no unused-import / missing-symbol errors).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_layout/src/native_view.rs crates/vmux_layout/src/plugin.rs
git commit -m "feat(layout): wire NativeViewPlugin — diff CurrentLayoutView into recorded ViewOps"
```

---

## Final verification

- [ ] **Run the full crate test suite**

Run: `cargo test -p vmux_layout`
Expected: all pass, including the 14 `native_view::tests`.

- [ ] **fmt + clippy (CI parity)**

Run: `cargo fmt -p vmux_layout && cargo clippy -p vmux_layout --all-targets`
Expected: no diff from fmt; no clippy warnings in `native_view.rs` (note `LayoutRenderer::from_env` is currently unused — that's expected; it's consumed in P2. If clippy flags dead_code, keep it `pub` (public API), which suppresses the lint).

---

## Self-Review

**Spec coverage (P1 portion):**
- LayoutRenderer flag → Task 1. ✓
- LayoutView model + projection (spec §2) → Task 2. ✓
- Reconciler keyed by stable id (spec §3) → Task 3 (tabs; recursive pane tree explicitly deferred to P2). ✓
- Plugin/resource wiring + change-gated update (spec §1) → Task 4. ✓
- Linux-only, no objc2 (spec §7) → entire plan is pure Rust; macOS applier deferred to P2. ✓

**Placeholder scan:** No TBD/TODO; every code step has complete code; the one deferred item (producer + applier) is named with its reason, not left as a placeholder inside a task. ✓

**Type consistency:** `NodeId`, `LayoutView`, `TabView`, `ViewOp` (`CreateTab`/`UpdateTab`/`RemoveTab`/`SetTabOrder`), `LayoutRenderer` (`Cef`/`Native`), `CurrentLayoutView`/`LastRenderedView`/`RecordedViewOps`, `diff_tabs`, `diff_into_ops`, `NativeViewPlugin` — names used identically across Tasks 1–4. ✓

**Known follow-ups for P2 (not P1 gaps):** ECS→`LayoutView` producer (`update_current_layout_view` via `build_layout_snapshot`), recursive pane-tree diff, `vmux_desktop::layout_native` applier (`ViewOp` → `NSGlassEffectView`/`NSView`), `LayoutRenderer` env init at app startup.
