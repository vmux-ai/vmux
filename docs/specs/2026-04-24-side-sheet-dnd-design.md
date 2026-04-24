# Side Sheet Drag-and-Drop for Panes and Tabs

## Overview

Add drag-and-drop editing of the pane/tab layout inside the side sheet.
Users drag tabs to reorder within a pane or move them to another pane,
drag panes to swap or split, and see a mini-map of the current layout
at the top of the sheet. All mutations persist via the existing
`Changed<Children>` auto-save.

## Scope

- **Side sheet representation** becomes a mini-map on top plus all panes
  listed linearly below with their tabs always inline (no click-to-expand).
- **Tab DnD**: reorder within a pane, move across panes, via native
  HTML5 drag events inside the linear list.
- **Pane DnD**: swap (center drop) and split (edge drop), via pointer
  events on the mini-map. Same-parent reorder is a special case of split
  when the drop edge matches the parent's split direction.
- **Auto-collapse**: after any move, a split with a single remaining
  child is replaced by that child in the grandparent.
- **Persistence**: piggy-backs on `Changed<Children>` already wired in
  `mark_dirty_on_change`.

## Out of Scope

- Renaming panes/tabs. Noted as a near-term follow-up; the data model
  below keeps stable `u64` ids so a `label: Option<String>` can be
  added later with no schema churn.
- Dragging Spaces.
- Initiating DnD from the 3D main window.
- Undo/redo of DnD operations (no existing undo infrastructure).

## Representation — Mini-Map + Linear List

The side sheet body is two stacked regions:

```
┌───────────────────────────┐
│ LAYOUT                    │  ← label
│ ┌──────┬─────────────┐    │
│ │      │   P2 · 2    │    │  ← mini-map (~90–120px)
│ │  P1  ├─────────────┤    │     Split → CSS flex container
│ │      │   P3 · 1    │    │     Pane  → leaf rectangle
│ └──────┴─────────────┘    │
│                           │
│ ╭─ Pane 1 ──────────────╮ │
│ │ • GitHub              │ │
│ │ • Linear              │ │  ← all panes listed linearly,
│ │ • Terminal            │ │     each with its tabs inline
│ ╰───────────────────────╯ │
│ ╭─ Pane 2 ──── (active) ╮ │
│ │ • Anthropic (active)  │ │
│ │ • Bevy docs           │ │
│ ╰───────────────────────╯ │
│ ╭─ Pane 3 ──────────────╮ │
│ │ • Slack               │ │
│ ╰───────────────────────╯ │
└───────────────────────────┘
```

- **Mini-map** recursively renders the split tree: `Split` → CSS flex
  container (`flex-direction: row` or `column`) with `flex-grow`
  matching each child's weight; `Pane` → leaf rectangle labelled
  `P{n} · {tab_count}`. Active pane gets the existing
  `ring-2 ring-ring` border style.
- **Linear list** iterates leaf panes in their flattened tree order.
  Each pane section shows a header and its tabs. Pane sections are
  drop targets for tabs. Pane sections are not drag sources — pane
  reorder/swap/split happens on the mini-map where spatial intent is
  unambiguous.

## Data Changes

### `PaneTreeEvent` becomes recursive

Replace the current flat shape:

```rust
// before
pub struct PaneTreeEvent {
    pub panes: Vec<PaneNode>,
}
```

with a tree that carries split structure:

```rust
pub struct PaneTreeEvent {
    pub root: LayoutNode,
}

pub enum LayoutNode {
    Split {
        id: u64,
        direction: SplitDirection,     // Row | Column
        children: Vec<LayoutNode>,
        flex_weights: Vec<f32>,
    },
    Pane {
        id: u64,
        is_active: bool,
        tabs: Vec<TabNode>,
    },
}
```

`TabNode` is unchanged. `SplitDirection` mirrors the existing
`PaneSplitDirection` in `layout/pane.rs`.

### New command event

```rust
pub const SIDE_SHEET_DRAG_EVENT: &str = "side-sheet-drag";

pub enum SideSheetDragCommand {
    MoveTab {
        from_pane: u64,
        from_index: usize,
        to_pane: u64,
        to_index: usize,
    },
    SwapPane {
        pane: u64,
        target: u64,
    },
    SplitPane {
        dragged: u64,
        target: u64,
        edge: Edge,                    // Left | Right | Top | Bottom
    },
}
```

`SideSheetCommandEvent` (`activate_tab`, `close_tab`) stays untouched —
drag commands travel on their own event channel so the handlers stay
independent.

## Drag Gestures

### Tab DnD (linear list, native HTML5)

- `draggable="true"` on each `TabRow`.
- `ondragover` on tab gaps and pane section bodies renders a drop
  indicator (2px horizontal line between rows, border highlight on the
  pane section).
- `ondrop` emits `SideSheetDragCommand::MoveTab` with source
  `(pane_id, index)` and target `(pane_id, index)`.
- Drop onto a pane header (not a gap) appends to the end of that pane.

### Pane DnD (mini-map, pointer events)

- `onpointerdown` on a pane rectangle captures the pointer and begins a
  drag; the source pane dims to 40% opacity and a floating ghost
  follows the cursor.
- While dragging over another pane rectangle, the target renders five
  overlay zones: 20%-wide strips on each of the four edges, a 40%
  centred square in the middle. The zone under the cursor highlights
  blue. Corner ambiguity is resolved by whichever axis distance is
  greater.
- `onpointerup` over a zone emits:
  - Center zone → `SwapPane { pane, target }`
  - Edge zone → `SplitPane { dragged, target, edge }`
- Releasing outside any pane or on the source itself is a no-op.

### Prevented gestures (cursor `no-drop`, no command emitted)

- Dragging a pane onto itself.
- Dragging the only pane in a Space (would leave the Space empty).
- Dragging a tab onto its current position.

## Tree Mutations

Four systems live in a new `crates/vmux_desktop/src/layout/drag.rs`,
one per command shape plus auto-collapse. All run after reading
`SideSheetDragCommand` and rely on the existing auto-save trigger.

### `MoveTab`

Reparent the tab entity and insert at the target index. If the source
pane ends up empty, leave it in place — users close panes explicitly;
a pane vanishing mid-drag is surprising and loses metadata (e.g. the
pane's `flex_grow`).

### `SwapPane`

Remove both panes from their parents' `Children`, re-insert each into
the other's slot. Positions and tab contents travel with each pane.
Works across split boundaries.

### `SplitPane`

`SplitPane` is the gesture-level command; its effect depends on the
target's parent split and the drop edge.

| Target's parent  | Drop edge      | Action                                               |
|------------------|----------------|------------------------------------------------------|
| Row              | Left           | Insert dragged before target in same Row             |
| Row              | Right          | Insert dragged after target in same Row              |
| Row              | Top            | Replace target with new Column split `[dragged, target]` |
| Row              | Bottom         | Replace target with new Column split `[target, dragged]` |
| Column           | Top            | Insert dragged before target in same Column          |
| Column           | Bottom         | Insert dragged after target in same Column           |
| Column           | Left           | Replace target with new Row split `[dragged, target]`    |
| Column           | Right          | Replace target with new Row split `[target, dragged]`    |
| None (root pane) | any            | Wrap target in a matching-direction split            |

Same-parent sibling reorder is the top four Row/Column rows where the
edge direction aligns with the parent split.

### Auto-collapse

After every mutation, walk from the affected parent(s) up to the root.
For each `PaneSplit` with exactly one remaining child, replace the
split with its child in the grandparent's `Children` and despawn the
split entity. Prevents dangling single-child splits.

### Flex weights

- Insertion into an existing split: new sibling gets the mean of the
  existing siblings' `flex_grow`. Keeps visible sizes approximately
  stable.
- New split via wrap: both children get `flex_grow = 1.0` (50/50).

### Helpers (extend `layout/swap.rs`)

```rust
fn move_to_index(child: Entity, parent: Entity, index: usize, ...);
fn wrap_in_split(
    target: Entity,
    split_direction: PaneSplitDirection,
    dragged_on_side: Side,  // Before | After
    ...
) -> Entity;                // returns the new split entity
fn collapse_if_single_child(split: Entity, ...);
```

## Rendering the Mini-Map

The webview builds the mini-map by recursing over `LayoutNode`:

```rust
// Dioxus pseudocode
fn render(node: &LayoutNode) -> Element {
    match node {
        Split { direction, children, flex_weights, .. } => rsx! {
            div {
                class: match direction {
                    SplitDirection::Row    => "flex flex-row gap-1",
                    SplitDirection::Column => "flex flex-col gap-1",
                },
                for (child, w) in children.iter().zip(flex_weights) {
                    div { style: "flex-grow: {w};", {render(child)} }
                }
            }
        },
        Pane { id, is_active, tabs, .. } => rsx! {
            PaneRect { id: *id, is_active: *is_active, tab_count: tabs.len() }
        }
    }
}
```

The same recursion powers the linear list: a flat pre-order traversal
over `LayoutNode::Pane` leaves produces `Pane 1`, `Pane 2`, … in the
order they appear in the mini-map left-to-right / top-to-bottom.

## Files Touched

**New:**
- `crates/vmux_desktop/src/layout/drag.rs` — command handler systems
  (`handle_move_tab`, `handle_swap_pane`, `handle_split_pane`) and the
  auto-collapse system.

**Modified:**
- `crates/vmux_side_sheet/src/event.rs` — replace `PaneTreeEvent`
  payload with `LayoutNode` tree; add `SideSheetDragCommand` and
  `SIDE_SHEET_DRAG_EVENT` constant.
- `crates/vmux_side_sheet/src/app.rs` — replace `PaneSection` loop
  with recursive `LayoutNodeView` / `SplitView` / `PaneRect` for the
  mini-map; add drag handlers and drop-zone overlays; keep the linear
  pane-and-tab list below.
- `crates/vmux_desktop/src/layout/pane.rs` — the tree-traversal that
  builds `PaneTreeEvent` emits the recursive structure (walks
  `Children` under the active Space, splits on `PaneSplit` vs leaf
  `Pane`).
- `crates/vmux_desktop/src/layout/side_sheet.rs` (or wherever the
  existing `SideSheetCommandEvent` bridge lives) — register the new
  `SideSheetDragCommand` event reader and forward to the drag handler
  systems.
- `crates/vmux_desktop/src/layout/swap.rs` — add `move_to_index`,
  `wrap_in_split`, `collapse_if_single_child`.

## Testing

**Unit tests in `drag.rs`** against a synthetic Bevy world:

1. `MoveTab` within same pane — tab order updates.
2. `MoveTab` across panes — tab reparents; source pane keeps its
   (now empty) tab list.
3. `SwapPane` same parent — equivalent to existing `swap_siblings`.
4. `SwapPane` cross-split — parents exchange one child each.
5. `SplitPane` all nine rows of the decision table, asserting the
   resulting tree shape and `flex_grow` values.
6. Auto-collapse: move the only sibling out of a Row → Row despawned,
   surviving child becomes direct grandchild.
7. Nested auto-collapse: cascade through two levels.

**Manual scenarios** (each followed by a full restart to verify
persistence):

1. Three tabs A/B/C in one pane; drag C between A and B → order
   A/C/B; restart, same.
2. Two panes side-by-side; drag right pane's tab into left pane's
   list → tab moves across; restart, same.
3. Three panes (Row with P1 left, Column P2/P3 right); drag P1 onto
   P3's center → P1 and P3 swap; restart, same.
4. Drag P1 onto P2's top edge → Column split replaces P2, `[P1, P2]`;
   restart, same.
5. Drag P1 out of a two-pane Row → Row collapses, P2 becomes direct
   child of the Space; restart, same.

## Deliverables

1. Recursive `PaneTreeEvent` payload and producer in
   `layout/pane.rs`.
2. `SideSheetDragCommand` event type and bridge registration.
3. Mini-map component in the Dioxus side sheet with drop-zone
   overlays and pointer-event drag handling.
4. Linear pane list with native HTML5 tab-drag handlers.
5. `move_to_index`, `wrap_in_split`, `collapse_if_single_child`
   helpers in `layout/swap.rs`.
6. `drag.rs` command-handler systems with unit tests for every row
   of the `SplitPane` decision table and for auto-collapse.
7. Manual-test pass through the scenarios above.
