# Auto Type-Stack Spiral Placement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **NOTE (vmux-specific):** Do NOT subagent-drive this plan. CEF builds are huge and long-lived agents drop sockets (see memory: subagent CEF build fragility). Execute INLINE in this session with a warm target dir.

**Goal:** When an agent opens a page via MCP without an explicit `direction`, route it into a per-type stack (file/browser/terminal/agent), creating new type-stacks by splitting the most-recently-created non-agent leaf along its longer side (Fibonacci spiral), reusing already-open pages by exact URL across the space, and never splitting the protected agent pane.

**Architecture:** A pure `resolve_placement` core in a new `vmux_layout::placement` module decides Focus (reuse) / AddTab (type-stack hit) / Spiral (new type). The ECS tree stays the single source of truth; the only new persisted state is a `SpawnSeq(u64)` creation-order stamp per pane. The `open_page`/`open_file` MCP path (`AgentCommand::OpenBeside` → `OpenBesideRequest` → `handle_open_beside_requests`) calls the resolver when no `direction` is given; an explicit `direction` keeps today's behavior.

**Tech Stack:** Rust, Bevy 0.19-rc ECS (messages + systems), moonshine_save persistence, existing `vmux_layout` pane/stack/tab/space modules.

---

## Scope & Decomposition

The committed spec (`docs/specs/2026-06-23-auto-type-stack-spiral-placement-design.md`) covers two MCP entry points: `open_page`/`open_file` (the `OpenBeside` path) and `run` (the terminal path in `vmux_agent`). These are independent subsystems.

- **This plan (Plan A)** implements the resolver + the `OpenBeside` path + reuse + spiral + agent-pane protection + direction-optional API. This is independently shippable and testable: file/browser/agent pages auto-group and spiral; terminals opened via `open_page` (`vmux://terminal/`) are classified and reused too.
- **Plan B (follow-up, planned separately after A lands):** migrate the `run`/terminal placement (`handle_agent_self_commands` + `AgentTerminalRegions` in `crates/vmux_agent/src/plugin.rs`) onto the same resolver so `run` terminals join the spiral as a first-class type. Interim: `run` keeps its current behavior (it already groups terminals into one region beside the agent, which is consistent with "one terminal stack").

**Refinement vs spec:** `PageKind` lives ONLY inside `placement.rs` and is derived from `PageMetadata.url`. We do NOT change the emitted protocol DTO `kind` string in `snapshot.rs` (it keeps `terminal`/`files`/`browser`). This keeps the WASM header/reconcile/page-render untouched (smaller blast radius). Reuse and type-grouping read URLs directly, so the DTO string never needs an `agent` value.

---

## File Structure

- **Create:** `crates/vmux_layout/src/placement.rs` — `PageKind`, `page_kind_for_url`, `Placement`, `LeafInfo`, `ReuseHit`, `resolve_placement` (pure core + unit tests). One responsibility: decide where a page goes.
- **Modify:** `crates/vmux_layout/src/pane.rs` — add `SpawnSeq` component, `SpawnCounter` resource, `stamp_spawn_seq` + `reseed_spawn_counter` systems; change `OpenBesideRequest.direction` to `Option<PaneDirection>`; branch `handle_open_beside_requests` to the resolver when direction is `None`.
- **Modify:** `crates/vmux_layout/src/lib.rs` — declare `pub mod placement;` and re-export.
- **Modify:** `crates/vmux_layout/src/plugin.rs` — register `reseed_spawn_counter` in `LayoutStartupSet::Post`.
- **Modify:** `crates/vmux_service/src/protocol.rs` (or wherever `AgentCommand` is defined) — `OpenBeside.direction` → `Option<AgentPaneDirection>`.
- **Modify:** `crates/vmux_mcp/src/tools.rs` — parse `direction` as optional (absent → `None`); pass through.
- **Modify:** `crates/vmux_agent/src/plugin.rs` — map `OpenBeside.direction: Option<_>` into `OpenBesideRequest.direction: Option<_>`.

---

## Task 1: `PageKind` classifier

**Files:**
- Create: `crates/vmux_layout/src/placement.rs`
- Modify: `crates/vmux_layout/src/lib.rs:46` (module list area)

- [ ] **Step 1: Declare the module**

In `crates/vmux_layout/src/lib.rs`, add alongside the other `#[cfg(not(target_arch = "wasm32"))] pub mod` entries (e.g. after `pub mod pane;` at line 46):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod placement;
```

- [ ] **Step 2: Write the failing test** (create `crates/vmux_layout/src/placement.rs` with just the classifier + test)

```rust
use crate::pane::PaneSplitDirection;
use bevy::math::Vec2;
use bevy::prelude::Entity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Agent,
    Terminal,
    File,
    Browser,
}

pub fn page_kind_for_url(url: &str) -> PageKind {
    if url.starts_with("vmux://agent/") {
        PageKind::Agent
    } else if url.starts_with("vmux://terminal/") {
        PageKind::Terminal
    } else if url.starts_with("file:") {
        PageKind::File
    } else {
        PageKind::Browser
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_core_four_kinds() {
        assert_eq!(page_kind_for_url("vmux://agent/vibe/abc"), PageKind::Agent);
        assert_eq!(page_kind_for_url("vmux://terminal/123"), PageKind::Terminal);
        assert_eq!(page_kind_for_url("file:///x.rs"), PageKind::File);
        assert_eq!(page_kind_for_url("https://example.com"), PageKind::Browser);
        assert_eq!(page_kind_for_url("vmux://services/"), PageKind::Browser);
        assert_eq!(page_kind_for_url("vmux://spaces/"), PageKind::Browser);
    }
}
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test -p vmux_layout placement::tests::classifies_core_four_kinds`
Expected: PASS (1 passed)

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_layout/src/placement.rs crates/vmux_layout/src/lib.rs
git commit -m "feat(layout): PageKind classifier for type-stack placement"
```

---

## Task 2: `SpawnSeq` creation-order stamp

**Files:**
- Modify: `crates/vmux_layout/src/pane.rs` (component near `PaneSize` ~line 308; systems + plugin registration ~line 51)
- Modify: `crates/vmux_layout/src/plugin.rs` (startup wiring, `LayoutStartupSet::Post`)

- [ ] **Step 1: Add the component + resource** (in `pane.rs`, after `PaneSize`'s `impl Default`, ~line 320)

```rust
#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::pane"]
#[require(Save)]
pub struct SpawnSeq(pub u64);

#[derive(Resource, Default)]
pub struct SpawnCounter(pub u64);
```

- [ ] **Step 2: Write the failing test** (in `pane.rs` `#[cfg(test)] mod tests`, end of file)

```rust
#[test]
fn stamp_spawn_seq_assigns_increasing_values_to_new_panes() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SpawnCounter>()
        .add_systems(Update, stamp_spawn_seq);

    let a = app.world_mut().spawn(Pane).id();
    app.update();
    let b = app.world_mut().spawn(Pane).id();
    app.update();

    let sa = app.world().get::<SpawnSeq>(a).expect("a stamped").0;
    let sb = app.world().get::<SpawnSeq>(b).expect("b stamped").0;
    assert!(sb > sa, "later-created pane must have higher SpawnSeq ({sb} > {sa})");
}

#[test]
fn reseed_spawn_counter_exceeds_max_existing() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<SpawnCounter>()
        .add_systems(Update, reseed_spawn_counter);

    app.world_mut().spawn((Pane, SpawnSeq(7)));
    app.world_mut().spawn((Pane, SpawnSeq(3)));
    app.update();

    assert_eq!(app.world().resource::<SpawnCounter>().0, 8);
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p vmux_layout pane::tests::stamp_spawn_seq_assigns_increasing_values_to_new_panes`
Expected: FAIL ("cannot find function `stamp_spawn_seq`")

- [ ] **Step 4: Implement the systems** (in `pane.rs`, near the other free systems)

```rust
pub fn stamp_spawn_seq(
    mut counter: ResMut<SpawnCounter>,
    new_panes: Query<Entity, (With<Pane>, Without<SpawnSeq>)>,
    mut commands: Commands,
) {
    for pane in &new_panes {
        counter.0 += 1;
        commands.entity(pane).insert(SpawnSeq(counter.0));
    }
}

pub fn reseed_spawn_counter(seqs: Query<&SpawnSeq>, mut counter: ResMut<SpawnCounter>) {
    let max = seqs.iter().map(|s| s.0).max().unwrap_or(0);
    if counter.0 <= max {
        counter.0 = max + 1;
    }
}
```

- [ ] **Step 5: Register in `PanePlugin`** (`pane.rs` ~line 53, chain into the existing builder expression)

Add `.register_type::<SpawnSeq>()`, `.init_resource::<SpawnCounter>()`, and `.add_systems(Update, stamp_spawn_seq)` to the `app...` builder chain in `impl Plugin for PanePlugin`.

- [ ] **Step 6: Register reseed at startup** (`crates/vmux_layout/src/plugin.rs`)

Find where `LayoutStartupSet::Post` systems are registered and add `crate::pane::reseed_spawn_counter` to `Startup` in `LayoutStartupSet::Post` (runs after `Persistence` so restored `SpawnSeq` values are counted before any new pane is stamped):

```rust
.add_systems(Startup, crate::pane::reseed_spawn_counter.in_set(LayoutStartupSet::Post))
```

- [ ] **Step 7: Run both tests + build**

Run: `cargo test -p vmux_layout pane::tests::stamp_spawn_seq_assigns_increasing_values_to_new_panes pane::tests::reseed_spawn_counter_exceeds_max_existing`
Expected: PASS (2 passed)
Run: `cargo build -p vmux_layout`
Expected: builds clean.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_layout/src/pane.rs crates/vmux_layout/src/plugin.rs
git commit -m "feat(layout): SpawnSeq creation-order stamp for spiral anchor"
```

---

## Task 3: `resolve_placement` pure core

**Files:**
- Modify: `crates/vmux_layout/src/placement.rs`

The resolver is pure over plain data so it is fully unit-testable without a World. The calling system (Task 5) builds these inputs from ECS queries and applies the output.

- [ ] **Step 1: Write the failing tests** (append to `placement.rs`, inside an extended `mod tests`)

```rust
#[cfg(test)]
mod resolve_tests {
    use super::*;

    fn e(n: u64) -> Entity {
        Entity::from_bits(n)
    }

    fn leaf(pane: u64, kinds: &[PageKind], seq: u64, size: (f32, f32)) -> LeafInfo {
        LeafInfo {
            pane: e(pane),
            kinds: kinds.to_vec(),
            spawn_seq: seq,
            size: Vec2::new(size.0, size.1),
        }
    }

    #[test]
    fn exact_url_reuse_wins() {
        let hit = ReuseHit { tab: e(1), stack: e(2) };
        let got = resolve_placement(
            "https://x.com",
            Some(hit),
            &[leaf(10, &[PageKind::Browser], 5, (800.0, 600.0))],
            e(10),
        );
        assert_eq!(got, Placement::Focus { tab: e(1), stack: e(2) });
    }

    #[test]
    fn same_type_adds_tab_no_split() {
        let got = resolve_placement(
            "https://b.com",
            None,
            &[leaf(10, &[PageKind::Browser], 5, (800.0, 600.0))],
            e(10),
        );
        assert_eq!(got, Placement::AddTab { pane: e(10) });
    }

    #[test]
    fn first_page_fills_empty_leaf() {
        let got = resolve_placement(
            "https://b.com",
            None,
            &[leaf(10, &[], 1, (800.0, 600.0))],
            e(10),
        );
        assert_eq!(got, Placement::AddTab { pane: e(10) });
    }

    #[test]
    fn new_type_splits_newest_nonagent_leaf_along_longer_side() {
        // agent leaf (protected) + a wide file leaf (newest non-agent).
        let leaves = [
            leaf(1, &[PageKind::Agent], 1, (800.0, 900.0)),
            leaf(2, &[PageKind::File], 9, (900.0, 400.0)),
        ];
        let got = resolve_placement("https://b.com", None, &leaves, e(1));
        // wide (w >= h) => Row split
        assert_eq!(
            got,
            Placement::Spiral { anchor: e(2), axis: PaneSplitDirection::Row }
        );
    }

    #[test]
    fn new_type_splits_tall_leaf_into_column() {
        let leaves = [leaf(2, &[PageKind::File], 9, (400.0, 900.0))];
        let got = resolve_placement("https://b.com", None, &leaves, e(2));
        assert_eq!(
            got,
            Placement::Spiral { anchor: e(2), axis: PaneSplitDirection::Column }
        );
    }

    #[test]
    fn agent_page_never_splits_when_agent_pane_exists() {
        let leaves = [
            leaf(1, &[PageKind::Agent], 1, (800.0, 900.0)),
            leaf(2, &[PageKind::Browser], 9, (900.0, 400.0)),
        ];
        let got = resolve_placement("vmux://agent/vibe/x", None, &leaves, e(2));
        assert_eq!(got, Placement::AddTab { pane: e(1) });
    }

    #[test]
    fn nonagent_page_bootstraps_by_splitting_agent_when_only_leaf() {
        let leaves = [leaf(1, &[PageKind::Agent], 1, (1600.0, 900.0))];
        let got = resolve_placement("https://b.com", None, &leaves, e(1));
        assert_eq!(
            got,
            Placement::Spiral { anchor: e(1), axis: PaneSplitDirection::Row }
        );
    }

    #[test]
    fn agent_page_bootstraps_by_splitting_newest_nonagent_when_no_agent_pane() {
        let leaves = [leaf(2, &[PageKind::Browser], 9, (400.0, 900.0))];
        let got = resolve_placement("vmux://agent/vibe/x", None, &leaves, e(2));
        assert_eq!(
            got,
            Placement::Spiral { anchor: e(2), axis: PaneSplitDirection::Column }
        );
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_layout placement::resolve_tests`
Expected: FAIL ("cannot find type `Placement`" / "cannot find function `resolve_placement`")

- [ ] **Step 3: Implement the types + resolver** (in `placement.rs`, above the test modules)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    Focus { tab: Entity, stack: Entity },
    AddTab { pane: Entity },
    Spiral { anchor: Entity, axis: PaneSplitDirection },
}

#[derive(Debug, Clone)]
pub struct LeafInfo {
    pub pane: Entity,
    pub kinds: Vec<PageKind>,
    pub spawn_seq: u64,
    pub size: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReuseHit {
    pub tab: Entity,
    pub stack: Entity,
}

fn longer_axis(size: Vec2) -> PaneSplitDirection {
    if size.x >= size.y {
        PaneSplitDirection::Row
    } else {
        PaneSplitDirection::Column
    }
}

fn newest_nonagent_leaf(leaves: &[LeafInfo]) -> Option<&LeafInfo> {
    leaves
        .iter()
        .filter(|l| !l.kinds.contains(&PageKind::Agent))
        .max_by_key(|l| l.spawn_seq)
}

/// Decide where a page (`url`) should open.
///
/// `reuse`: an exact-URL hit anywhere in the space, if any (highest priority).
/// `leaves`: leaf panes in the CURRENT tab, each with the kinds of its stacks,
/// its `SpawnSeq`, and its pixel size.
/// `self_pane`: the calling agent's own pane, used as a fallback target.
pub fn resolve_placement(
    url: &str,
    reuse: Option<ReuseHit>,
    leaves: &[LeafInfo],
    self_pane: Entity,
) -> Placement {
    if let Some(hit) = reuse {
        return Placement::Focus { tab: hit.tab, stack: hit.stack };
    }

    let kind = page_kind_for_url(url);

    // Reuse a lone empty leaf (fresh tab / leftover blank pane) for any kind.
    if let Some(empty) = leaves.iter().find(|l| l.kinds.is_empty()) {
        return Placement::AddTab { pane: empty.pane };
    }

    if kind == PageKind::Agent {
        // Agent pages always live in the single agent pane.
        if let Some(agent) = leaves.iter().find(|l| l.kinds.contains(&PageKind::Agent)) {
            return Placement::AddTab { pane: agent.pane };
        }
        // No agent pane yet: bootstrap one by splitting the newest non-agent leaf.
        if let Some(anchor) = newest_nonagent_leaf(leaves) {
            return Placement::Spiral { anchor: anchor.pane, axis: longer_axis(anchor.size) };
        }
        return Placement::AddTab { pane: self_pane };
    }

    // Non-agent kind: reuse an existing same-type stack if present.
    if let Some(same) = leaves.iter().find(|l| l.kinds.contains(&kind)) {
        return Placement::AddTab { pane: same.pane };
    }

    // New non-agent type: spiral on the newest non-agent leaf.
    if let Some(anchor) = newest_nonagent_leaf(leaves) {
        return Placement::Spiral { anchor: anchor.pane, axis: longer_axis(anchor.size) };
    }

    // Only an agent pane exists: bootstrap the non-agent region by splitting it once.
    if let Some(agent) = leaves.iter().find(|l| l.kinds.contains(&PageKind::Agent)) {
        return Placement::Spiral { anchor: agent.pane, axis: longer_axis(agent.size) };
    }

    Placement::AddTab { pane: self_pane }
}
```

- [ ] **Step 4: Run the resolver tests**

Run: `cargo test -p vmux_layout placement::resolve_tests`
Expected: PASS (8 passed)

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/placement.rs
git commit -m "feat(layout): resolve_placement core (reuse/type-stack/spiral/agent)"
```

---

## Task 4: Direction becomes optional (auto by default)

Today `open_page`/`open_file` always send a `direction` (default `right`). Make it optional: absent → resolver (auto), present → today's explicit split.

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs` (the `AgentCommand` enum — grep `OpenBeside {`)
- Modify: `crates/vmux_mcp/src/tools.rs:409` (`parse_direction`) and the `open_page`/`open_file` arms (lines 422-473)
- Modify: `crates/vmux_layout/src/pane.rs:878` (`OpenBesideRequest`)
- Modify: `crates/vmux_agent/src/plugin.rs:701` (`OpenBeside` arm)

- [ ] **Step 1: Make protocol direction optional**

Locate `AgentCommand::OpenBeside` (e.g. `grep -n "OpenBeside {" crates/vmux_service/src/protocol.rs`). Change the field:

```rust
OpenBeside {
    anchor: ProcessId,
    direction: Option<AgentPaneDirection>,
    url: String,
    focus: bool,
},
```

- [ ] **Step 2: Update the MCP dispatch test first** (in `crates/vmux_mcp/src/tools.rs` tests, add)

```rust
#[test]
fn open_page_without_direction_is_auto() {
    let anchor = vmux_service::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "open_page",
        serde_json::json!({"url": "https://x.com"}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::OpenBeside { direction, .. }) => {
            assert_eq!(direction, None, "absent direction => auto placement");
        }
        other => panic!("expected OpenBeside, got {other:?}"),
    }
}

#[test]
fn open_page_with_direction_is_explicit() {
    let anchor = vmux_service::protocol::ProcessId::new();
    let target = dispatch_with_anchor(
        "open_page",
        serde_json::json!({"url": "https://x.com", "direction": "left"}),
        Some(anchor),
    )
    .unwrap();
    match target {
        DispatchTarget::Command(AgentCommand::OpenBeside { direction, .. }) => {
            assert_eq!(direction, Some(vmux_service::protocol::AgentPaneDirection::Left));
        }
        other => panic!("expected OpenBeside, got {other:?}"),
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p vmux_mcp open_page_without_direction_is_auto`
Expected: FAIL (compile error: `direction` is not `Option`, plus the existing `open_page_dispatch_uses_anchor` will need updating).

- [ ] **Step 4: Implement optional parsing** (`tools.rs`)

Replace `parse_direction` (line 409) with an optional variant:

```rust
fn parse_direction(arguments: &Value) -> Result<Option<AgentPaneDirection>, String> {
    match arguments.get("direction").and_then(Value::as_str) {
        None => Ok(None),
        Some("right") => Ok(Some(AgentPaneDirection::Right)),
        Some("left") => Ok(Some(AgentPaneDirection::Left)),
        Some("top") => Ok(Some(AgentPaneDirection::Top)),
        Some("bottom") => Ok(Some(AgentPaneDirection::Bottom)),
        Some(other) => Err(format!("unknown direction: {other}")),
    }
}
```

In the `open_page` arm (line 433) and `open_file` arm (line 462), `let direction = parse_direction(&arguments)?;` now yields `Option<_>` and is passed straight into `AgentCommand::OpenBeside { direction, .. }`. (The `run` arm still needs a concrete direction — keep a local default there: `let direction = parse_direction(&arguments)?.unwrap_or(AgentPaneDirection::Right);`.)

- [ ] **Step 5: Fix the existing dispatch test** (`open_page_dispatch_uses_anchor`, line 802) — it sends `"direction": "right"`, so assert `direction == Some(AgentPaneDirection::Right)` if it inspects direction; otherwise it compiles unchanged. Update only if it binds `direction`.

- [ ] **Step 6: Update `OpenBesideRequest`** (`pane.rs:878`)

```rust
#[derive(Message, Clone)]
pub struct OpenBesideRequest {
    pub pane: Entity,
    pub direction: Option<PaneDirection>,
    pub url: String,
    pub request_id: [u8; 16],
    pub focus: bool,
}
```

- [ ] **Step 7: Map Option through the agent plugin** (`plugin.rs:709`)

In the `ServiceAgentCommand::OpenBeside` arm, map the optional direction:

```rust
open_beside_writer.write(vmux_layout::OpenBesideRequest {
    pane,
    direction: direction.as_ref().map(to_pane_direction),
    url: url.clone(),
    request_id: request.request_id.0,
    focus: *focus,
});
```

(`to_pane_direction` takes `&AgentPaneDirection`; `direction.as_ref().map(...)` yields `Option<PaneDirection>`.)

- [ ] **Step 8: Build the three crates**

Run: `cargo build -p vmux_service -p vmux_mcp -p vmux_agent`
Expected: clean (fix any other `OpenBeside`/`OpenBesideRequest` construction sites the compiler flags — `grep -rn "OpenBesideRequest {" crates` and `grep -rn "OpenBeside {" crates`; in existing pane.rs tests set `direction: Some(PaneDirection::Right)`).

- [ ] **Step 9: Run the MCP tests**

Run: `cargo test -p vmux_mcp`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/vmux_service crates/vmux_mcp crates/vmux_agent crates/vmux_layout/src/pane.rs
git commit -m "feat(mcp): optional open_page/open_file direction (auto placement default)"
```

---

## Task 5: Wire the resolver into `handle_open_beside_requests`

When `direction` is `Some`, keep today's `find_sibling_pane`/`split_or_extend` path. When `None`, build resolver inputs from the live tree and apply the result.

**Files:**
- Modify: `crates/vmux_layout/src/pane.rs:889` (`handle_open_beside_requests`)

- [ ] **Step 1: Write the failing integration tests** (in `pane.rs` tests)

These mirror the existing `open_beside_*` tests (MinimalPlugins + messages). They drive the real system with `direction: None`.

```rust
fn place_pane_with_url(app: &mut App, parent: Entity, seq: u64, size: Vec2, url: &str) -> Entity {
    use bevy::ui::{ComputedNode, UiGlobalTransform};
    let pane = app
        .world_mut()
        .spawn((
            Pane,
            SpawnSeq(seq),
            Node::default(),
            LastActivatedAt::now(),
            ChildOf(parent),
            ComputedNode { size, ..default() },
            UiGlobalTransform::from_translation(size * 0.5),
        ))
        .id();
    let stack = app
        .world_mut()
        .spawn((stack_bundle(), LastActivatedAt::now(), ChildOf(pane)))
        .id();
    app.world_mut().entity_mut(stack).insert(vmux_core::PageMetadata {
        url: url.to_string(),
        ..default()
    });
    pane
}

fn open_beside_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<OpenBesideRequest>()
        .add_message::<PageOpenRequest>()
        .init_resource::<NewStackContext>()
        .init_resource::<SpawnCounter>()
        .add_systems(Update, handle_open_beside_requests);
    app
}

#[test]
fn auto_same_type_adds_tab_without_splitting() {
    let mut app = open_beside_app();
    let tab = app.world_mut().spawn((Tab::default(), vmux_core::Active, LastActivatedAt::now())).id();
    let space = app.world_mut().spawn((crate::space::Space, vmux_core::Active)).id();
    app.world_mut().entity_mut(tab).insert(ChildOf(space));
    let browser_pane =
        place_pane_with_url(&mut app, tab, 5, Vec2::new(800.0, 600.0), "https://a.com");

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: browser_pane,
            direction: None,
            url: "https://b.com".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.update();

    assert!(app.world().get::<PaneSplit>(browser_pane).is_none(), "same type must not split");
    let stacks = app
        .world()
        .get::<Children>(browser_pane)
        .map(|c| c.iter().filter(|&e| app.world().get::<Stack>(e).is_some()).count())
        .unwrap_or(0);
    assert_eq!(stacks, 2, "new browser page tabs into the existing browser pane");
}

#[test]
fn auto_new_type_splits_anchor() {
    let mut app = open_beside_app();
    let tab = app.world_mut().spawn((Tab::default(), vmux_core::Active, LastActivatedAt::now())).id();
    let space = app.world_mut().spawn((crate::space::Space, vmux_core::Active)).id();
    app.world_mut().entity_mut(tab).insert(ChildOf(space));
    let browser_pane =
        place_pane_with_url(&mut app, tab, 5, Vec2::new(1600.0, 900.0), "https://a.com");

    app.world_mut()
        .resource_mut::<Messages<OpenBesideRequest>>()
        .write(OpenBesideRequest {
            pane: browser_pane,
            direction: None,
            url: "file:///x.rs".into(),
            request_id: [0u8; 16],
            focus: false,
        });
    app.update();

    assert!(
        app.world().get::<PaneSplit>(browser_pane).is_some(),
        "a new file type must split the anchor (wide => Row)"
    );
    let split = app.world().get::<PaneSplit>(browser_pane).unwrap();
    assert_eq!(split.direction, PaneSplitDirection::Row);
}
```

(Add any missing `use` items at the top of the test module: `bevy::math::Vec2`, `crate::placement::*`, `crate::tab::Tab`.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p vmux_layout pane::tests::auto_new_type_splits_anchor`
Expected: FAIL (system ignores `None`, or panics building inputs — current code expects a concrete direction).

- [ ] **Step 3: Add resolver SystemParams + branch** (`handle_open_beside_requests`)

Extend the system signature with the queries needed to build resolver inputs and to search the space, then branch on `req.direction`:

```rust
pub fn handle_open_beside_requests(
    mut reader: MessageReader<OpenBesideRequest>,
    pane_children: Query<&Children, With<Pane>>,
    split_dir_q: Query<&PaneSplit>,
    tab_filter: Query<Entity, With<Stack>>,
    child_of_q: Query<&ChildOf>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    // resolver inputs:
    all_children: Query<&Children>,
    seq_q: Query<&SpawnSeq>,
    node_q: Query<&ComputedNode>,
    page_q: Query<&vmux_core::PageMetadata, With<Stack>>,
    spaces: Query<(), With<crate::space::Space>>,
    active_space: Res<crate::space::ActiveSpaceEntity>,
    tab_q: Query<Entity, With<Tab>>,
    mut commands: Commands,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    mut new_stack_ctx: ResMut<NewStackContext>,
) {
    let mut split_this_batch: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    for req in reader.read() {
        // Explicit direction: today's behavior unchanged.
        if let Some(direction) = req.direction {
            let target_pane = match find_sibling_pane(
                req.pane, &direction, &child_of_q, &split_dir_q, &pane_children, &leaf_panes,
            ) {
                Some(sibling) => sibling,
                None => {
                    let existing_tabs: Vec<Entity> = pane_children
                        .get(req.pane)
                        .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                        .unwrap_or_default();
                    let split_dir = direction_to_split(&direction);
                    let already_split =
                        !split_this_batch.insert(req.pane) || split_dir_q.contains(req.pane);
                    split_or_extend(
                        &mut commands, req.pane, split_dir, &existing_tabs, req.focus, already_split,
                    )
                }
            };
            spawn_beside_stack(target_pane, req, &mut commands, &mut new_stack_ctx, &mut page_open_requests);
            continue;
        }

        // Auto: resolver.
        let current_tab = crate::space::space_of(req.pane, &child_of_q, &spaces)
            .or(active_space.0)
            .and_then(|_| tab_of_pane(req.pane, &child_of_q, &tab_q));
        let Some(tab) = current_tab else {
            // No tab context: fall back to a tab on the anchor's own pane.
            spawn_beside_stack(req.pane, req, &mut commands, &mut new_stack_ctx, &mut page_open_requests);
            continue;
        };

        let reuse = find_reuse_in_space(
            &req.url, active_space.0, &spaces, &tab_q, &all_children, &page_q, &child_of_q,
        );
        let leaves = collect_leaf_infos(tab, &all_children, &leaf_panes, &pane_children, &seq_q, &node_q, &page_q);

        match crate::placement::resolve_placement(&req.url, reuse, &leaves, req.pane) {
            crate::placement::Placement::Focus { tab, stack } => {
                commands.entity(tab).insert(LastActivatedAt::now());
                commands.entity(stack).insert(LastActivatedAt::now());
                // do NOT open a new page; it already exists.
            }
            crate::placement::Placement::AddTab { pane } => {
                spawn_beside_stack(pane, req, &mut commands, &mut new_stack_ctx, &mut page_open_requests);
            }
            crate::placement::Placement::Spiral { anchor, axis } => {
                let existing_tabs: Vec<Entity> = pane_children
                    .get(anchor)
                    .map(|c| c.iter().filter(|&e| tab_filter.contains(e)).collect())
                    .unwrap_or_default();
                let already_split =
                    !split_this_batch.insert(anchor) || split_dir_q.contains(anchor);
                let target_pane =
                    split_or_extend(&mut commands, anchor, axis, &existing_tabs, req.focus, already_split);
                spawn_beside_stack(target_pane, req, &mut commands, &mut new_stack_ctx, &mut page_open_requests);
            }
        }
    }
}
```

- [ ] **Step 4: Add the small helpers** (`pane.rs`, near `handle_open_beside_requests`)

```rust
fn spawn_beside_stack(
    target_pane: Entity,
    req: &OpenBesideRequest,
    commands: &mut Commands,
    new_stack_ctx: &mut NewStackContext,
    page_open_requests: &mut MessageWriter<PageOpenRequest>,
) {
    let stack_ts = if req.focus { LastActivatedAt::now() } else { LastActivatedAt(0) };
    let new_stack = commands
        .spawn((stack_bundle(), stack_ts, ChildOf(target_pane)))
        .id();
    open_or_prompt_stack(new_stack, Some(req.url.clone()), new_stack_ctx, page_open_requests);
}

fn tab_of_pane(
    pane: Entity,
    child_of_q: &Query<&ChildOf>,
    tab_q: &Query<Entity, With<Tab>>,
) -> Option<Entity> {
    let mut cur = pane;
    for _ in 0..32 {
        if tab_q.contains(cur) {
            return Some(cur);
        }
        cur = child_of_q.get(cur).ok()?.get();
    }
    None
}

fn collect_leaf_infos(
    tab: Entity,
    all_children: &Query<&Children>,
    leaf_panes: &Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: &Query<&Children, With<Pane>>,
    seq_q: &Query<&SpawnSeq>,
    node_q: &Query<&ComputedNode>,
    page_q: &Query<&vmux_core::PageMetadata, With<Stack>>,
) -> Vec<crate::placement::LeafInfo> {
    let mut panes = Vec::new();
    crate::stack::collect_leaf_panes(tab, all_children, leaf_panes, &mut panes);
    panes
        .into_iter()
        .map(|pane| {
            let kinds = pane_children
                .get(pane)
                .map(|c| {
                    c.iter()
                        .filter_map(|child| page_q.get(child).ok())
                        .map(|p| crate::placement::page_kind_for_url(&p.url))
                        .collect()
                })
                .unwrap_or_default();
            crate::placement::LeafInfo {
                pane,
                kinds,
                spawn_seq: seq_q.get(pane).map(|s| s.0).unwrap_or(0),
                size: node_q.get(pane).map(|n| n.size).unwrap_or(Vec2::ZERO),
            }
        })
        .collect()
}

fn find_reuse_in_space(
    url: &str,
    active_space: Option<Entity>,
    spaces: &Query<(), With<crate::space::Space>>,
    tab_q: &Query<Entity, With<Tab>>,
    all_children: &Query<&Children>,
    page_q: &Query<&vmux_core::PageMetadata, With<Stack>>,
    child_of_q: &Query<&ChildOf>,
) -> Option<crate::placement::ReuseHit> {
    let _ = spaces;
    let space = active_space?;
    let tabs: Vec<Entity> = all_children
        .get(space)
        .map(|c| c.iter().filter(|&e| tab_q.contains(e)).collect())
        .unwrap_or_default();
    for tab in tabs {
        let mut stack_stack = vec![tab];
        while let Some(node) = stack_stack.pop() {
            if let Ok(meta) = page_q.get(node)
                && meta.url == url
            {
                let _ = child_of_q;
                return Some(crate::placement::ReuseHit { tab, stack: node });
            }
            if let Ok(children) = all_children.get(node) {
                stack_stack.extend(children.iter());
            }
        }
    }
    None
}
```

Add `use bevy::ui::ComputedNode;` and `use bevy::math::Vec2;` to the imports if not already present (the file already uses `bevy::prelude::*` and `bevy::ui::UiGlobalTransform`; `ComputedNode`/`Vec2` come via prelude/ui — confirm at build).

- [ ] **Step 5: Run the integration tests**

Run: `cargo test -p vmux_layout pane::tests::auto_same_type_adds_tab_without_splitting pane::tests::auto_new_type_splits_anchor`
Expected: PASS.

- [ ] **Step 6: Run the full layout test suite (catch regressions)**

Run: `cargo test -p vmux_layout`
Expected: PASS (the existing `open_beside_*` tests still pass with `direction: Some(...)`).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_layout/src/pane.rs
git commit -m "feat(layout): auto type-stack spiral placement for open_page/open_file"
```

---

## Task 6: Manual runtime verification

Automated tests verify the broadcast/ECS state; the user runtime-tests UI (see memory: verify observable behavior). Do NOT launch `make dev` unbounded (see memory: no unbounded make dev) — ask the user to drive these, or use a bounded self-killing repro.

- [ ] **Step 1: Build the desktop app**

Run: `cargo build -p vmux_desktop`
Expected: clean build.

- [ ] **Step 2: Verification checklist (user-driven)**

With an agent running, via MCP (no `direction`):
1. `open_file` a file → file pane appears beside the agent (agent keeps priority width).
2. `open_page` a URL → browser pane spirals off the file pane (longer side).
3. `open_file` a second file → tabs into the existing file pane (no new split).
4. `open_page` the SAME URL as step 2 → focuses the existing browser tab (no duplicate); cross-tab hit switches tabs.
5. Confirm the agent pane is never split by steps 1-4 beyond the single bootstrap split.

---

## Self-Review (completed during planning)

- **Spec coverage:** classifier (Task 1), spawn-order anchor (Task 2), resolver reuse/type/spiral/agent (Task 3), auto-default+override API (Task 4), OpenBeside wiring (Task 5). `run`/terminal path is explicitly deferred to Plan B (documented in Scope).
- **Placeholder scan:** none — every code step has complete code.
- **Type consistency:** `Placement`/`LeafInfo`/`ReuseHit`/`page_kind_for_url`/`resolve_placement` names match across Tasks 3 and 5; `SpawnSeq`/`SpawnCounter` match across Tasks 2 and 5; `OpenBesideRequest.direction: Option<PaneDirection>` matches across Tasks 4 and 5.
- **Known build-time follow-ups (flagged in steps):** locate the exact `AgentCommand::OpenBeside` definition file via grep (Task 4 Step 1); fix all `OpenBesideRequest {`/`OpenBeside {` construction sites the compiler flags; confirm `ComputedNode`/`Vec2` imports resolve in `pane.rs`.
