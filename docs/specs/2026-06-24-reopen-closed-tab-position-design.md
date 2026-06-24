# Reopen Closed Tab — Full Position Restore

Date: 2026-06-24
Status: Approved (design)

## Problem

`cmd+shift+t` (Reopen Closed Page, `StackCommand::Reopen`) does not return a reopened
page to the space / tab / pane / stack position it was closed from. PR #159 added
space + tab-index restore, but only for the whole-tab close case. Closing a stacked
tab (a stack inside a pane that holds other stacks) or a pane inside a split reopens
the page as a brand-new top-level tab instead of restoring it in place.

## Layout model

```
Space  (stable id: SpaceId)
 └─ Tab                       (identified via its root pane's PaneId)
     └─ Pane (PaneSplit)      root split under the tab      (PaneId — new)
         ├─ Pane (PaneSplit)  nested split                  (PaneId — new)
         │   └─ Pane (leaf)   holds Stacks                  (PaneId — new)
         └─ Pane (leaf)
             └─ Stack, Stack…  stacked tabs                 (positional: stack_index)
```

- Every internal pane node carries `PaneSplit`; every leaf pane has no `PaneSplit`
  and contains one or more `Stack`s. A `Stack` holds the page (`PageMetadata`).
- Only `Space` has a stable, persisted identifier (`SpaceId`). `Tab`, `Pane`, and
  `Stack` are identified solely by ephemeral Bevy entity ids. The frontend snapshot
  derives node ids from `entity.to_bits()` (`snapshot.rs`, `protocol::format_id`),
  which do not survive a save/load cycle.

## Current behavior (root cause)

- `archive_on_stack_close` (`vmux_layout/src/archive.rs`) records only `space_id`
  and `tab_index`. Pane location and stack-index-within-pane are discarded.
- `handle_reopen_closed_page` always calls `spawn_tab_scaffold_in_space`, building a
  fresh `Tab → split-root Pane → leaf Pane → Stack`, then re-inserts that tab at
  `tab_index` only when the origin space still matches.
- Therefore: stacked-tab closes and split-pane closes can never be restored in
  place — they always reappear as a new tab.

`StackCommand::Close` (`vmux_layout/src/stack.rs:298`) has four outcomes; the restore
ladder below maps to them:

- **A** — pane has >1 stack: only the stack is despawned; pane + tab survive.
- **B2** — last stack in a split pane: stack + leaf pane despawned, split collapses.
- **B1** — last stack of a tab that has sibling tabs: the whole tab is despawned.
- **B3** — last stack everywhere: replaced with a fresh empty stack (nothing to
  reopen meaningfully; out of scope).

## Goal

Reopen restores the page to its original Space → Tab → Pane → Stack position,
robust across save/load, reconstructing collapsed splits when needed. Degrade
gracefully (never lose the page) when the original structure no longer exists.

## Design

### 1. Stable identifier — `PaneId` only

We add exactly one new component, `PaneId(String)` (opaque UUID), in
`vmux_layout/src/pane.rs`, mirroring `SpaceId`. It derives
`Component, Reflect, Default, Clone, Debug, PartialEq, Eq`, is `#[reflect(Component)]`,
and is `#[require(Save)]` so it round-trips through `space.ron`. Register it in the
pane plugin (`pane.rs:53`).

No `TabId` or `StackId`. Rationale (the rest of the position is integers + reuse):

- **Stack** — the closed stack is despawned and recreated on reopen; it is never
  looked up again. We need only its integer `stack_index` among sibling stacks.
- **Tab** — a tab's single child is its root split pane, so a tab is identified by
  that root pane's `PaneId`. If the tab was despawned (its root pane is gone too),
  we fall back to `tab_index` to recreate it.
- **Pane** — split nodes and leaves have no reusable identity today
  (`leaf_pane_bundle` = `Pane` + `PaneSize` + layout `Node`; no metadata, no
  `CreatedAt`). Reconstruction must find split nodes by a stable handle, so `PaneId`
  is genuinely required.

`PageMetadata { title, url, favicon_url, bg_color }` cannot serve as the anchor: it
is not unique (two terminals share `vmux://terminal/`), it mutates on navigation, it
is not `#[require(Save)]` (url is replayed via `PageOpenRequest` on load), and panes
carry none of it.

**Assignment** is centralized, not per-spawn-site (there are many `Pane` spawn sites;
per-site assignment would rot). A system `assign_pane_ids` runs in `Update`:

```
Query<Entity, (With<Pane>, Without<PaneId>)> → insert PaneId(Uuid::new_v4())
```

Load-safe: only fills missing ids, so ids restored from `space.ron` always win.
A one-frame delay before a freshly spawned pane has an id is harmless — close/archive
happens long after spawn.

### 2. Capture position at close

Extend `ArchivedPage` (component) and `PageArchiveRequest` (message), both in
`vmux_core/src/archive.rs`, with a structural position:

```rust
pub struct PagePosition {
    pub space_id: String,
    pub tab_index: usize,        // ordering hint if the tab must be recreated
    pub path: Vec<PaneStep>,     // tab root split → … → parent split of the leaf
    pub leaf_pane_id: String,    // PaneId of the leaf pane that held the closed stack
    pub stack_index: usize,      // position among sibling stacks in that leaf pane
}

pub struct PaneStep {
    pub split_id: String,        // PaneId of the PaneSplit node at this level
    pub axis: SplitAxis,         // vmux_core mirror of PaneSplitDirection
    pub child_index: usize,      // which child we descend into
    pub flex_weights: Vec<f32>,  // children flex weights, to restore PaneSize
}

pub enum SplitAxis { Row, Column }
```

The tab is identified by the first `PaneStep`'s `split_id` (the root split pane lives
directly under the tab); no separate tab id is stored. `path[0].split_id` is the root
pane, so resolving it yields the tab via its `ChildOf`.

`PagePosition`, `PaneStep`, `SplitAxis` live in `vmux_core` and derive `Reflect` so
the component persists. `vmux_core` must not depend on `vmux_layout` (cycle), so
`SplitAxis` mirrors `PaneSplitDirection`; `vmux_layout` maps between them at the
boundary (consistent with the existing no-new-crates / mirror-type pattern).

`archive_on_stack_close` builds `PagePosition` by walking from the closing stack up
to its tab, recording each `PaneSplit` ancestor's `PaneId`, direction, the child
index descended into, and that split's children flex weights; plus the leaf
`PaneId`, the stack's index among sibling stacks, `tab_index`, and `space_id`.

Older `ArchivedPage` entries deserialized without the new fields default to empty
`path` / ids and fall through to the legacy recreate-tab path (step 3 below).

### 3. Restore ladder (`handle_reopen_closed_page`)

Resolve the target space by `space_id` (fallback: active space, then any space).
Within that space, first match wins:

1. **Leaf pane alive** — a live leaf `Pane` with `PaneId == leaf_pane_id` exists:
   spawn a single `Stack`, parent it to that pane, insert at `min(stack_index, n)`.
   *(Case A — the primary fix.)*
2. **Split survives, leaf gone** — resolve the tab from `path[0].split_id` (root
   pane → `ChildOf` → tab), then walk `path` by `split_id`. Descend through every
   split that still exists. From the deepest surviving split, recreate the missing
   sub-splits and the leaf pane to honor the remaining path (reusing `pane.rs` split
   helpers), apply `flex_weights` via `PaneSize`, then insert the stack.
   *(Case B2 — true split reconstruction.)*
   - The common collapse sub-case (the split kept ≥2 children) is trivial: the
     split with `split_id` still exists, so we only re-add a leaf pane at
     `child_index`.
   - The harder sub-case (split collapsed to one survivor) recreates one split
     level: convert the surviving container back into a `PaneSplit` on the recorded
     `axis`, move its current content into one child, add the reopened leaf as the
     other child at `child_index`.
3. **Tab gone, space alive** — no `PaneId` from `path` resolves, so recreate a tab
   scaffold at `tab_index` and insert the stack. *(Case B1 / PR #159, keyed by
   `tab_index`.)*
4. **Space gone** — append a new tab in the fallback space. *(Existing behavior.)*

On divergence (a stored child index no longer fits, structure rearranged after
close), clamp indices and attach at the deepest valid point — best effort, never
drop the page. This trade-off was accepted when choosing true reconstruction.

After attach, set `LastActivatedAt::now()` on the reopened stack, its leaf pane, and
its tab so the reopened page lands focused. Despawn the consumed `ArchivedPage`.

### 4. Content respawn

Unchanged from today: once the target stack exists, dispatch the existing
`SpawnAgentInStackRequest` / `TerminalSpawnRequest` / `PageOpenRequest` paths based
on the archived `url` + `launch`.

### 5. Persistence & registration

Register `PaneId` in the pane plugin so Save/Load round-trips it and the loader does
not panic on unknown types (cf. the stale-`space.ron` crash class). `PagePosition`
reflection is covered by `ArchivedPage` already being `Save`.

## Out of scope

- The frontend `LayoutSnapshot` keeps its entity-bits node ids; reopen is entirely
  backend ECS logic and needs no frontend change.
- Case B3 (reopening after the very last stack collapsed to an empty stack).
- Exposing `PaneId` over the protocol/MCP surface.

## Testing (native `cargo test -p vmux_layout`, `-p vmux_core`)

- `assign_pane_ids` fills missing `PaneId` on panes and leaves existing ids
  untouched.
- Capture records full `PagePosition`: leaf_pane_id, stack_index, and a `path` with
  correct split ids / axes / child indices / flex weights.
- Reopen step 1: restores a stacked tab into the surviving leaf pane at its index.
- Reopen step 2a: split kept ≥2 children → leaf re-added under the surviving split.
- Reopen step 2b: collapsed split → split level reconstructed, leaf restored.
- Reopen step 3: tab gone → tab recreated at `tab_index`.
- Reopen step 4: space gone → appended in fallback space.
- Divergence: stale indices clamp without dropping the page.
- Existing archive tests continue to pass (legacy entries → recreate-tab path).

## Risks

- Reconstruction touches `pane.rs` split/collapse logic, which is intricate; mitigated
  by reusing existing split helpers and broad unit coverage of each ladder step.
- Adding one persisted component (`PaneId`) grows `space.ron`; acceptable and
  additive.
