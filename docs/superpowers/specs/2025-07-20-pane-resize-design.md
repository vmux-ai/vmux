# Pane Resize Design

Pane splits can be resized by dragging the gap between panes or via tmux-style keyboard commands.

## Data Model

### PaneSize Component

```rust
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub struct PaneSize {
    pub flex_grow: f32, // default 1.0
}
```

Added to leaf panes only (entities with `Pane` but without `PaneSplit`). Split containers always use `flex_grow: 1.0`. Persisted via moonshine-save. On session rebuild, `rebuild_session_views` reads `PaneSize.flex_grow` and applies it to the pane's `Node.flex_grow`.

New panes default to `PaneSize { flex_grow: 1.0 }`.

### Constants

| Name | Value | Purpose |
|------|-------|---------|
| `MIN_PANE_PX` | 60.0 | Hard minimum pane size in pixels. Resize stops at this limit. |
| `RESIZE_STEP` | 0.05 | Keyboard resize step as fraction of total sibling flex_grow. |

### PaneDrag Component

```rust
#[derive(Component)]
struct PaneDrag {
    prev_child: Entity,
    next_child: Entity,
    start_pos: f32,
    start_prev_grow: f32,
    start_next_grow: f32,
}
```

Inserted on the `PaneSplit` entity when a drag begins. Removed on mouse release. The split's `PaneSplitDirection` provides the axis, so no need to duplicate it.

## Drag Resize

System: `pane_gap_drag_resize` in `Update`.

### Hover

- Query all `PaneSplit` entities (without `PaneDrag`) for their children's `ComputedNode` positions.
- For each adjacent pair of children, compute the gap rect between them.
- If cursor is within the 4px gap, set `CursorIcon` to `ColResize` (Row split) or `RowResize` (Column split).

### Drag

- On left mouse press while cursor is in a gap: insert `PaneDrag` on the `PaneSplit` entity with adjacent children, cursor start position, and snapshot of both children's `flex_grow`.
- Each frame, query `Query<(&PaneDrag, &PaneSplit, &Children)>`:
  - Compute pixel delta along the split axis.
  - Convert to flex_grow delta: `delta_ratio = pixel_delta / parent_size_along_axis * total_sibling_flex_grow`.
  - Add delta to `prev_child` flex_grow, subtract from `next_child`.
  - Clamp both to enforce `MIN_PANE_PX`: minimum ratio = `MIN_PANE_PX / parent_size_along_axis * total_sibling_flex_grow`.
  - Write to both `Node.flex_grow` and `PaneSize.flex_grow`.
- On mouse release: remove `PaneDrag` from the split entity, mark auto-save dirty.

### Focus suppression

While any `PaneDrag` component exists (`!pane_drag_q.is_empty()`), `poll_cursor_pane_focus` returns early to prevent focus switching during resize.

## Keyboard Resize

Fill existing `PaneCommand::Resize{Left,Right,Up,Down}` stubs. Bindings: `Ctrl+B, Alt+Arrow`.

### Algorithm

1. Find the focused leaf pane.
2. Walk up to its parent PaneSplit.
3. Check axis alignment:
   - `ResizeLeft`/`ResizeRight` require a `Row` split.
   - `ResizeUp`/`ResizeDown` require a `Column` split.
   - If the parent split's axis doesn't match, walk up to the grandparent split and retry. If no matching ancestor, no-op.
4. Find the pane's index among the matching split's children.
5. Determine the resize pair:
   - `ResizeLeft`/`ResizeUp`: shrink this pane, grow the previous sibling. No-op if first child.
   - `ResizeRight`/`ResizeDown`: shrink this pane, grow the next sibling. No-op if last child.
6. Apply `RESIZE_STEP`: transfer `step = RESIZE_STEP * total_sibling_flex_grow` from one pane to the other.
7. Clamp both to `MIN_PANE_PX` minimum.
8. Write to both `Node.flex_grow` and `PaneSize.flex_grow`.
9. Mark auto-save dirty.

## EqualizeSize

`PaneCommand::EqualizeSize` (`Ctrl+B, =`): resets all children of the focused pane's parent PaneSplit to `flex_grow: 1.0` (siblings only, not recursive). Writes to both `Node.flex_grow` and `PaneSize.flex_grow`. Marks auto-save dirty.

## Persistence

- Add `PaneSize` to `do_save()` allowlist.
- Register `PaneSize` type: `app.register_type::<PaneSize>()`.
- In `rebuild_session_views`, when inserting `Node` on leaf panes, use `PaneSize.flex_grow` instead of hardcoded `1.0`.
- In `spawn_default_session` and `leaf_pane_bundle()`, include `PaneSize::default()`.

## Files Changed

| File | Change |
|------|--------|
| `pane.rs` | Add `PaneSize` component, `PaneDrag` component, `pane_gap_drag_resize` system, fill `Resize*` and `EqualizeSize` handlers. |
| `persistence.rs` | Add `PaneSize` to save allowlist, read it in `rebuild_session_views`. |
| `lib.rs` | Register `PaneSize` type. |
