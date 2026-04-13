# Implement All Missing AppCommand Handlers

## Context

11 command handlers are currently empty stubs. This plan covers implementing all of them, grouped by complexity.

## Unimplemented Commands

| Command | Handler Location | Status |
|---------|-----------------|--------|
| PaneCommand::Toggle | pane.rs:194 | empty `{}` |
| PaneCommand::Zoom | pane.rs | empty `{}` |
| PaneCommand::SelectLeft/Right/Up/Down | pane.rs | empty `{}` |
| PaneCommand::SwapPrev/SwapNext | pane.rs | empty `{}` |
| PaneCommand::RotateForward/RotateBackward | pane.rs | empty `{}` |
| TabCommand::New/Close | tab.rs | empty `{}` |

SpaceCommand::New already has no handler and is out of scope.

---

## Group 1: Pane Toggle (trivial)

**File**: [pane.rs](crates/vmux_desktop/src/layout/pane.rs)

`Toggle` should cycle to the next pane — same as tmux's `prefix+o`. The logic already exists in `on_pane_cycle` which handles `TabCommand::Next/Previous`. 

**Implementation**: Add `AppCommand::Pane(PaneCommand::Toggle) => 1` to the match in `on_pane_cycle` (line 206). Remove the empty match arm from `handle_pane_commands`.

---

## Group 2: Directional Pane Selection (medium)

**File**: [pane.rs](crates/vmux_desktop/src/layout/pane.rs)

SelectLeft/Right/Up/Down need to find the nearest leaf pane in a given direction based on UI layout position.

**Implementation**: New system `handle_pane_select_direction` that:
1. Queries all leaf panes with `(&ComputedNode, &UiGlobalTransform)` to get their center positions via `ui_gt.transform_point2(Vec2::ZERO)`
2. Gets the active pane's center position
3. For each direction, filters candidates that are in the correct half-plane (e.g. SelectLeft → candidates where `candidate.x < active.x`)
4. Among valid candidates, picks the closest one by Euclidean distance
5. Swaps `Active` component to the target

This needs to run in `PostUpdate` after `UiSystems::Layout` so that `ComputedNode` positions are available — or use a 1-frame delay by storing positions in a resource each frame.

**Simpler approach**: Run in `ReadAppCommands` but query `GlobalTransform` on pane nodes (available from prior frame). Since pane layout doesn't change every frame, prior-frame positions are accurate enough for selection.

Actually, looking at how `sync_children_to_ui` works in browser.rs (line 139), it queries `ComputedNode` and `UiGlobalTransform` in `PostUpdate`. But our handlers run in `Update` (ReadAppCommands set). We should use `GlobalTransform` which is available from the previous frame, or move the directional selection to a separate system that runs in PostUpdate.

**Chosen approach**: Extract directional selection into its own system in `PostUpdate` after `UiSystems::Layout`. Use a `Resource` (`PendingPaneSelect`) to communicate the direction from the `ReadAppCommands` handler. The PostUpdate system reads the resource, does the spatial query, and clears it.

Wait — actually simpler: just query `Node` positions. The pane nodes have `Node` with flex layout. After layout, `ComputedNode` has the resolved sizes. But we can just use the `UiGlobalTransform` which is set after layout.

**Final approach**: Add `SelectLeft/Right/Up/Down` match arms that write to an `Events<PaneSelectDirection>` event. Add a separate system in PostUpdate (after UiSystems::Layout) that reads these events and does the spatial query.

Actually, simplest: just use a `Local<Option<PaneSelectDirection>>` or a `Resource`. But Bevy events work well here.

**Final final approach** (keeping it simple): Handle it directly in `handle_pane_commands`. Query `ComputedNode` on leaf panes. In `Update`, `ComputedNode` reflects the *previous* frame's layout — which is fine since pane positions don't change between frames unless a split just happened. This avoids the complexity of cross-schedule communication.

**Steps**:
1. Add `ComputedNode` and `UiGlobalTransform` to the leaf pane query in `handle_pane_commands`
2. For SelectLeft/Right/Up/Down: get active pane center, filter candidates by direction, pick closest
3. Swap `Active` component

---

## Group 3: Pane Swap (medium)

**File**: [pane.rs](crates/vmux_desktop/src/layout/pane.rs)

SwapPrev/SwapNext swap the active pane's position with its neighbor in the flattened leaf order.

**Implementation**:
1. Collect all leaf panes sorted by entity bits (same as `on_pane_cycle`)
2. Find current active's position in the list
3. Determine the target (prev or next, wrapping)
4. Swap the *children* (browsers) of the two panes — reparent browsers from active to target and vice versa using `commands.entity(browser).insert(ChildOf(new_parent))`
5. Keep `Active` on the original entity (the pane that now has the swapped content)

This effectively swaps what's displayed in each pane position.

---

## Group 4: Pane Rotate (medium)

**File**: [pane.rs](crates/vmux_desktop/src/layout/pane.rs)

RotateForward/RotateBackward rotate all pane contents by one position.

**Implementation**:
1. Collect all leaf panes sorted by entity bits
2. Collect browser children of each pane: `Vec<(Entity, Vec<Entity>)>` — pane entity + its browser entities
3. For RotateForward: shift browser assignments by +1 (last pane's browsers go to first pane)
4. For RotateBackward: shift by -1
5. Reparent all browsers to their new pane via `ChildOf`
6. Move `Active` to follow the content (rotate active index same direction)

---

## Group 5: Pane Zoom (medium-hard)

**File**: [pane.rs](crates/vmux_desktop/src/layout/pane.rs)

Zoom toggles the active pane between maximized (fills entire pane area) and normal.

**Implementation**:
1. New component `Zoomed` on the zoomed pane entity
2. New resource `ZoomState { hidden_panes: Vec<Entity> }` to track which panes were hidden
3. **Zoom in**: 
   - Query all leaf panes that are NOT the active pane
   - Set `Display::None` on each non-active leaf pane AND all PaneSplit ancestors (except the root)
   - Store hidden entities in `ZoomState`
   - Insert `Zoomed` on active pane
4. **Zoom out** (toggle when already zoomed):
   - Restore `Display::Flex` (default) on all entities in `ZoomState.hidden_panes`
   - Remove `Zoomed` from active pane
   - Clear `ZoomState`

**Edge case**: If the zoomed pane is closed or split while zoomed, need to auto-unzoom. Add an observer or check in the Close/Split handlers.

**Simpler approach**: Instead of tracking hidden panes, just hide all siblings at every ancestor level up to the root pane. To unzoom, walk the tree and restore all `Display::None` nodes back to default. This avoids needing a resource.

**Chosen approach**:
1. `Zoomed` component on the active pane
2. On zoom: walk from active pane up to root. At each PaneSplit parent, set `Display::None` on all children except the one in the active path. Store nothing — the Zoomed component is the state.
3. On unzoom: walk all pane descendants of root, remove any `Display::None` that was set. Remove `Zoomed`.
4. In Close/Split handlers: if `Zoomed` exists on active, unzoom first.

---

## Group 6: Tab New/Close (hard — structural)

**Files**: [tab.rs](crates/vmux_desktop/src/layout/tab.rs), [display.rs](crates/vmux_desktop/src/layout/display.rs), [pane.rs](crates/vmux_desktop/src/layout/pane.rs)

Currently there's no Tab entity. The root pane container is spawned directly in `display.rs` setup as a child of DisplayGlass. To support multiple tabs, we need to introduce a Tab layer.

**Implementation**:
1. **Tab component** (already exists in tab.rs but unused) marks root pane containers
2. Add `Tab` component to the root pane entity spawned in display.rs:169 
3. **TabCommand::New**:
   - Spawn a new root pane entity with `(Tab, Pane, PaneSplit, HostWindow, ...)` same as display.rs:169-191
   - Set `Display::None` on the current active tab's root pane
   - Set the new tab as visible
   - Insert `Active` on the new tab's first leaf pane
4. **TabCommand::Close**:
   - Find the root pane with `Tab` that contains the active leaf pane (walk up via ChildOf)
   - Despawn the entire tab subtree
   - Activate the next/previous tab
   - If last tab, spawn a fresh default tab
5. **TabCommand::Next/Previous** — currently handled in `on_pane_cycle` which cycles *panes*. This is confusing but changing it now would break the existing Ctrl+Tab behavior. Leave `on_pane_cycle` as-is (it cycles panes within the visible tab). Tab switching will use a different mechanism once tab UI exists.

**Key insight**: The root pane in display.rs (line 169) already has `Pane + PaneSplit`. Adding `Tab` to it marks it as a tab root. New tabs are additional `Pane + PaneSplit + Tab` entities as children of DisplayGlass.

---

## Implementation Order

1. **Toggle** — trivial, 2 lines changed
2. **SelectLeft/Right/Up/Down** — self-contained spatial query
3. **SwapPrev/SwapNext** — reparenting logic
4. **RotateForward/RotateBackward** — similar to swap but all panes
5. **Zoom** — new component + tree walking
6. **Tab New/Close** — structural addition

## Files Modified

| File | Changes |
|------|---------|
| [pane.rs](crates/vmux_desktop/src/layout/pane.rs) | Toggle, Select*, Swap*, Rotate*, Zoom handlers. New `Zoomed` component. Extended queries for `ComputedNode`/`UiGlobalTransform`. |
| [tab.rs](crates/vmux_desktop/src/layout/tab.rs) | Tab New/Close handlers. Remove `#[allow(dead_code)]` from `Tab`. |
| [display.rs](crates/vmux_desktop/src/layout/display.rs) | Add `Tab` component to root pane entity (line 170). Extract root pane bundle into reusable function. |

## Verification

- `cargo check` after each group
- Manual test: run the app, split some panes, verify each command works via keybindings
- Test edge cases: zoom then close, swap with single pane, rotate with single pane, directional select with no neighbor in that direction
