# Persist Sibling Order + Keyboard Reorder Commands

## Overview

Persist the order of Tab, Pane, and Space entities across restarts, and
add keyboard commands to swap siblings. Order is stored implicitly in
Bevy's `Children` relationship component, saved via reflection. No new
ordering component.

## Scope

- **Persistence**: save/restore sibling order for Tab, Pane, Space.
- **Commands**: `TabCommand::Swap`, `PaneCommand::Swap`,
  `SpaceCommand::Swap`, each taking a `SwapTarget` (`Prev` | `Next` |
  `Indices(a, b)`).
- **Active-entity semantics**: the active entity is whichever sibling
  has the highest `LastActivatedAt`. Swaps move entities within the
  parent's `Children` but do not mutate `LastActivatedAt`, so the
  active entity follows its own id through the reorder.

## Out of Scope

- Drag-and-drop reordering.
- `PaneCommand::RotateForward` / `RotateBackward` (remain stubbed).
- `TabCommand::MoveToPane` (remains stubbed).
- Tmux `-s` / `-t` cross-parent source/target addressing.

## Data Changes

### Reflection registration

In the persistence plugin, register the two relationship components so
they can be serialized/deserialized via Bevy's reflection system:

```rust
app.register_type::<Children>()
   .register_type::<ChildOf>();
```

`Children` implements `MapEntities`, so moonshine-save remaps the
`Vec<Entity>` on load.

### Save allowlist

Add `Children` to the existing filter in `do_save` (`ChildOf` is
already allowed):

```rust
save.components = SceneFilter::deny_all()
    .allow::<Save>()
    .allow::<ChildOf>()
    .allow::<Children>()   // new
    .allow::<Tab>()
    ...;
```

### Hook-collision caveat

Bevy's `ChildOf` relationship has an `on_insert` hook that appends the
source entity to the target's `Children`. During scene load, moonshine
deserializes components in an order we do not fully control:

- If `Children` deserializes first and `ChildOf` inserts fire after,
  the hook re-appends — producing duplicates or the wrong order.
- If `ChildOf` fires first and `Children` deserializes after,
  deserialization may overwrite the hook output — fine, but relies on
  deserialization order.

**Verification plan** (Phase 1 of implementation):

1. Land the minimal change (registration + allowlist) behind a save
   and manual restart.
2. Inspect a post-load `session.ron` side-by-side with a running
   `Children` dump.
3. If order is correct: done.
4. If broken: capture the deserialized `Children` Vec into a
   short-lived resource during load (before hooks overwrite it), then
   in a `PostStartup` system dedup and reorder each parent's actual
   `Children` to match. Exact mechanism decided during implementation.

Pick the simpler path first; add the fixup only if needed. Document the
outcome as a spec amendment.

## Commands

New shared enum:

```rust
#[derive(Reflect, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwapTarget {
    Prev,
    Next,
    Indices(usize, usize),
}
```

Variant additions:

```rust
pub enum TabCommand {
    // existing variants...
    Swap(SwapTarget),
}

pub enum PaneCommand {
    // existing variants, minus SwapPrev/SwapNext
    Swap(SwapTarget),
    // RotateForward, RotateBackward kept as stubs
}

pub enum SpaceCommand {
    // existing variants...
    Swap(SwapTarget),   // new
}
```

Removed: `PaneCommand::SwapPrev`, `PaneCommand::SwapNext`. The command bar,
keybinding, and any settings referencing those variants migrate to
`PaneCommand::Swap(SwapTarget::Prev | Next)`.

Default keybindings: leave to implementation plan. Suggested starting
point (mirrors tmux / many tiling WMs):

- `Ctrl+Shift+[` / `Ctrl+Shift+]` → `PaneCommand::Swap(Prev | Next)`
- `Ctrl+Shift+,` / `Ctrl+Shift+.` → `TabCommand::Swap(Prev | Next)`
- `Ctrl+Shift+PageUp` / `PageDown` → `SpaceCommand::Swap(Prev | Next)`

## Reorder Logic

### Per-entity sibling resolution

Each entity type has a different parent and a different notion of
"same-type sibling":

| Entity | Parent                  | Same-type filter            |
|--------|-------------------------|-----------------------------|
| Tab    | active Pane             | `With<Tab>`                 |
| Pane   | parent `PaneSplit`      | `With<Pane>`                |
| Space  | active Profile          | `With<Space>`               |

Profile's `Children` mixes Spaces and Visits, so the helper must filter
before indexing. Pane's `Children` mixes Tabs and Browsers is *not*
relevant here because Browser lives under Tab, not under Pane — a leaf
Pane's children are all Tabs. But `PaneSplit` may contain nested splits
alongside leaf panes, both with `Pane`, so filtering by `With<Pane>`
picks up both.

### Shared helper

```rust
fn swap_same_type_siblings<F>(
    parent: Entity,
    a: usize,
    b: usize,
    children_q: &mut Query<&mut Children>,
    is_kind: F,
) -> Option<()>
where F: Fn(Entity) -> bool,
{
    let mut children = children_q.get_mut(parent).ok()?;
    let positions: Vec<usize> = children.iter()
        .enumerate()
        .filter(|(_, e)| is_kind(*e))
        .map(|(i, _)| i)
        .collect();
    let pos_a = *positions.get(a)?;
    let pos_b = *positions.get(b)?;
    if pos_a == pos_b { return Some(()); }
    children.swap(pos_a, pos_b);
    Some(())
}

fn resolve_prev_next(
    parent: Entity,
    active: Entity,
    children_q: &Query<&Children>,
    is_kind: impl Fn(Entity) -> bool,
) -> (usize, Vec<usize>) {
    let children = children_q.get(parent).unwrap();
    let typed: Vec<usize> = children.iter().enumerate()
        .filter(|(_, e)| is_kind(*e))
        .map(|(i, _)| i)
        .collect();
    let idx = children.iter().position(|e| e == active).unwrap();
    let typed_idx = typed.iter().position(|p| *p == idx).unwrap();
    (typed_idx, typed)
}
```

Each handler (`handle_tab_commands`, `handle_pane_commands`,
`handle_space_commands`) resolves the parent + active entity, converts
`SwapTarget` → `(a, b)` typed indices, and calls the shared helper.

### SwapTarget resolution

```rust
fn indices_for(target: SwapTarget, typed_len: usize, active_idx: usize)
    -> Option<(usize, usize)>
{
    match target {
        SwapTarget::Prev => active_idx.checked_sub(1).map(|p| (active_idx, p)),
        SwapTarget::Next => (active_idx + 1 < typed_len).then(|| (active_idx, active_idx + 1)),
        SwapTarget::Indices(a, b) => (a < typed_len && b < typed_len).then_some((a, b)),
    }
}
```

Out-of-range indices or at-end Prev/Next are silent no-ops.

## Active-Entity Behavior

`LastActivatedAt` remains the mechanism for "active". Swaps do not touch
`LastActivatedAt`. Consequence:

- Before swap: active tab is T, at position 1.
- `TabCommand::Swap(Next)`: positions 1 ↔ 2.
- After swap: T still has the highest `LastActivatedAt` among tabs, so
  it is still active — now at position 2.

This matches tmux default (without `-d`): focus follows the moved pane.

Tmux's `-d` flag (stay on position, not entity) is out of scope. If
needed later, it becomes `Swap { target, keep_position: bool }` and a
post-swap `LastActivatedAt::now()` on the displaced entity.

## Persistence Triggers

Add `Changed<Children>` to `mark_dirty_on_change`:

```rust
fn mark_dirty_on_change(
    mut auto_save: ResMut<AutoSave>,
    added_tabs: Query<(), Added<Tab>>,
    // ...
    changed_children: Query<(), Changed<Children>>,   // new
) {
    if /* ... existing triggers ... */ || !changed_children.is_empty() {
        auto_save.dirty = true;
        auto_save.debounce.reset();
    }
}
```

Swap-only reorders do not add/remove entities, so without this trigger
the save pipeline will not fire until the next structural change or the
60s periodic tick.

## Testing

Manual scenarios (each followed by a full app restart):

1. Three tabs A/B/C under one pane. Swap(Next) on A → order B/A/C.
   Restart → order still B/A/C.
2. Two panes side-by-side (split Row). Swap(Prev) on right pane →
   panes exchange positions. Restart → same.
3. Three spaces. Swap(Indices(0, 2)) → first and third swap. Restart →
   same.
4. Two tabs, active is left. Swap(Next). Confirm active is still the
   originally-active tab, now on the right.
5. Swap(Next) on the last tab → no-op, no save churn.

Automated: unit test the `indices_for` resolver and the
`swap_same_type_siblings` helper with a synthetic `Children` Vec.

## Deliverables

1. Register `Children` + `ChildOf` for reflection in the persistence
   plugin.
2. Add `Children` to the `do_save` allowlist.
3. Add `Changed<Children>` to `mark_dirty_on_change`.
4. Define `SwapTarget` enum.
5. Add `Swap(SwapTarget)` variants to `TabCommand`, `PaneCommand`,
   `SpaceCommand`. Remove `PaneCommand::SwapPrev` / `SwapNext`.
6. Implement shared swap helper + SwapTarget resolver.
7. Wire swap handling into `handle_tab_commands`, `handle_pane_commands`,
   and `handle_space_commands` (the latter currently has empty match
   arms for its variants).
8. Update command bar / keybinding config to reference the new variants.
9. Verify load-time `Children` integrity (see hook-collision caveat).
   If broken, add the post-load fixup and amend this spec.
