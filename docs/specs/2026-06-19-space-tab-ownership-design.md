# Space-owned tab layout — design

Date: 2026-06-19
Status: approved (pending spec review)

## Problem

Creating a new space (e.g. agent runs `create_space` "mistralai/dashboard" and sets a
startup dir) corrupts the layout: pages despawn, and closing tabs surfaces tabs from
non-active spaces. Observed on Vmux.app v0.0.16 (release == current HEAD; components use
`#[type_path = "vmux_desktop::*"]` overrides, persistence lives in `vmux_desktop`).

### Evidence

- Release log 12:12:45–52: a machine-paced storm of ~20 `Layout(Stack(Close))`, each
  immediately followed by `cef_create_browser uri=vmux://terminal/`. Close → spawn
  replacement → close → repeat. This is the visible "pages dropping."
- `~/.vmux/settings.ron`: per-space override key `"mistralai-dashboard"` (dash) while the
  space id is `mistralai/dashboard` (slash). Lookup misses → the new space ignores its
  `startup_dir` ("why doesn't it start on the repo?").
- `store.ron` aftermath: active space `mistralai/dashboard` ended with **0 tabs**; a single
  orphaned `vmux-ai/vmux` tab survived.

### Root causes

1. **Ownership is a loose label on flat siblings.** Tabs are all children of `MainNode`;
   space membership is only a `SpaceId(String)` component (`vmux_layout/src/space.rs`).
2. **Degrade-to-global.** `in_active_space` / `same_space` (space.rs:100-115) treat a missing
   `SpaceId` as "belongs to every space." New tabs spawn without `SpaceId` and are
   backfilled a frame later by `assign_orphan_tabs_to_active_space` (space.rs:87) — a race
   that can mis-tag or transiently globalize a tab.
3. **Global selection in command paths.** `handle_stack_commands` resolves the focused
   tab/pane/stack via the **global** `focused_stack()` (stack.rs:214 → 134,
   `active_among(tabs.iter())`), not the space-scoped `FocusedStack` resource
   (stack.rs:140-160). `handle_tab_commands` / `on_tabs_command_emit` likewise pick
   `active_tab` as the global max `LastActivatedAt` (tab.rs:90, 348). After a new space is
   created, the previous space's agent tab is the global-most-recent → commands operate on
   the wrong space.
4. **Agent layout is gated by the globally active space.** `serve_snapshot_requests` filters
   the snapshot by the global active space (reconcile.rs:361). An agent anchored in a
   background space sees the wrong/empty layout and its `update_layout` can reach the active
   space.
5. **Override key vs space-id slug mismatch.** `resolve_startup_dir` (vmux_setting
   runtime.rs:92) exact-matches `settings.spaces[space_id]`; a non-canonical key is silently
   ignored.

## Goals

- Space membership is **structural** (hierarchy), not a label.
- Stack/tab commands, agent layout, and reconcile can never select or despawn across spaces.
- An agent anchored in space X always reads/drives space X regardless of which space is
  visible.
- Per-space `startup_dir`/`startup_url` overrides actually apply.
- No backward-compat migration: on an incompatible store, hard-reset.

## Non-goals

- The rkyv `client error` + background service `version=0.0.14` skew seen in the logs (stale
  daemon) is a separate issue, out of scope.
- Multi-window. vmux is single-window today; containers are designed for one window.

## Decisions (from brainstorming)

- **Representation:** the `Space` entity *is* the render container. Tabs are `ChildOf(Space)`.
- **Render model:** one space container shown at a time; background spaces stay live and
  hidden. Two-level visibility: active Space container → active Tab within it.
- **Selection:** unify on a generic `Active` marker at every level.
- **Agent scope:** layout read/drive is scoped to the agent's anchored space.
- **Migration:** none. Hard-reset an incompatible store.
- **Slug fix:** folded in (canonical keying; no rekey migration — reset clears the bad key).

## Architecture

### Ownership tree

```
Main  (render root, ChildOf window root)
├─ Space "vmux-ai/vmux"  [Active]            Node Display=Flex
│   ├─ Tab  [Active]
│   │   └─ PaneSplit → Pane [Active] → Stack [Active] (page)
│   └─ Tab
└─ Space "mistralai/dashboard"               Node Display=None (live, hidden)
    └─ Tab [Active]
        └─ PaneSplit → Pane → Stack (page)
```

- `Space` gains the view layer (`Node` absolute-fill, `Transform`, `GlobalTransform`,
  `Visibility`) and `ChildOf(Main)`.
- `SpaceId(String)` stays **only on `Space`** = stable identity (settings overrides, spaces
  list, profile dir). **Removed from tabs.**
- Delete `same_space`, `in_active_space`, `assign_orphan_tabs_to_active_space`
  (space.rs:87-115). A tab's space = its parent `Space`.
- New helper `space_of(entity, &child_of_q, &space_q) -> Option<Entity>`: walk `ChildOf` up
  to the nearest entity with `Space`.

### Unified `Active` selection

- New `Active` marker component in `vmux_core`. **Runtime-derived, never persisted.**
  Replaces `ActiveSpaceTag` (the latter is deleted; references updated).
- Invariant: **at most one `Active` child per parent**, at each selectable level:
  - Space under `Main`
  - Tab under a `Space`
  - active branch child under each `PaneSplit`
  - Stack under a leaf `Pane`
- **Focused path = walk `Active` down**: active Space → its Active Tab → follow Active
  split-branch to the leaf Pane → its Active Stack. Replaces `focused_stack`,
  `compute_focused_stack`, and `active_among` (stack.rs:80-160).
- **`ensure_active` safety-net systems** (one per level): if a parent has children of the
  selectable kind but none is `Active`, mark the max-`LastActivatedAt` child. Focus/close
  handlers only need to *set* `Active` on the new target; any gap (e.g. the Active entity was
  despawned) self-heals next frame. This bounds the exactly-one invariant maintenance.
- `LastActivatedAt` retained for: history, tab order, MRU-on-close target, and as the
  `ensure_active` seed/tiebreak (so restore-on-load works without persisting `Active`).
- Switching space: insert `Active` on the target Space, clear it from the previous, and stamp
  the target Space's `LastActivatedAt` so the active space restores via the seed after a
  reload.

### Space switch + visibility

- A system toggles each Space container's `Node.display` (active = `Flex`, others = `None`)
  and `Visibility` from the `Active` Space. Background spaces remain fully live.
- Within the active space, tab visibility shows the Active Tab. `sync_tab_visibility`
  (tab.rs:260) is rewritten: scope = the active Space's tab children; show the Active one.

### Command scoping

- `handle_stack_commands` (stack.rs) resolves the focused tab/pane/stack by walking `Active`
  from the active Space — no global `active_among`.
- `handle_tab_commands` / `on_tabs_command_emit` (tab.rs): the operated tab and its siblings
  are the active Space's tab children (Active for the default target).
- Net: chrome/keyboard Stack/Tab commands can only ever touch the active space.

### Agent / reconcile scoping

- `serve_snapshot_requests` (reconcile.rs:325) and `collect_existing_ids` (reconcile.rs:722):
  scope to a **target Space subtree**. Target = `space_of(self_stack)` when the request
  carries an anchor (agent), else the active Space (chrome). Replaces the `SpaceId` `retain`
  (reconcile.rs:361) and `in_active_space` filter.
- `apply` / `apply_close` diff and despawn only within the target Space's subtree → an
  agent's `update_layout` can never reach another space, and a background agent drives its
  own space correctly.

### Tab/space spawn

- `spawn_requested_tab_layouts` (window.rs:469): parent the new tab to the **target Space
  container** (active Space, or the request's space), not `Main`; drop the `SpaceId` insert
  (window.rs:491-498). Mark the new tab `Active` (and bump `LastActivatedAt`).
- `on_space_command` "new" (vmux_space plugin.rs:484): spawn `Space` with the container view
  layer + `ChildOf(Main)` + `Active`; clear `Active` from the previous space; the
  `TabLayoutSpawnRequest` targets the new Space container.
- `tab_count` for the spaces list (`space_rows_from_world`, plugin.rs:168) counts a Space's
  tab children instead of matching `SpaceId`.

### Persistence (no migration)

- Save allowlist (persistence.rs:152): `SpaceId` now appears on `Space` only; **drop
  `ActiveSpaceTag`**. `ChildOf` already round-trips both `Space→Main` and `Tab→Space`. Ensure
  `LastActivatedAt` is saved for `Space` (already in the allowlist) so the active space
  restores.
- `rebuild_space_views` (persistence.rs:279): add the container view layer to each `Space`
  and fix the `Space→Main` link at runtime (Main is not persisted — same pattern used for
  tabs→Main today). No legacy reparent-by-`SpaceId` code.
- **Schema-version guard.** Write a sidecar `store.version` file (single integer) next to
  `store.ron` on every save. In `load_space_on_startup` (persistence.rs:183), read it before
  `trigger_load`; if it is missing or less than the current schema version, delete `store.ron`
  (and the version file) and start fresh, then write the current version on the next save.
  Implemented alongside the existing `remove_stale_space_if_needed` check (persistence.rs:201).
  Reading a plain sidecar avoids parsing the scene and prevents both the broken flat-tab load
  and the known stale-store load panic.

### Startup-dir slug fix

- Canonical id = the `SpaceId` produced by the existing slug rules (keeps `/`, lowercases,
  spaces→`-`; see `unique_space_id_among` and the rename test plugin.rs:733).
- Per-space override writes always key by the live `SpaceId`. `resolve_startup_dir/url` stay
  exact-match (correct once keys are canonical).
- Optional: prune `settings.spaces` keys matching no live space (mirrors
  `prune_orphan_space_dirs`, plugin.rs:330).
- No rekey migration: the existing `mistralai-dashboard` entry is cleared by the hard reset.

## Testing

ECS / unit tests (run as `vmux_layout` / `vmux_space` package tests):

- Create space → the new tab is `ChildOf` the new Space; the new Space is `Active`; the
  previous Space is not.
- Switch space → only the active Space container is displayed; background space keeps its
  tab/pane/stack subtree alive.
- `Stack(Close)` / `Tab(Close)` while space B is active never despawns space A's subtree.
- Reconcile `apply` with an anchor in space A leaves space B's subtree intact; anchorless
  apply targets the active space.
- `ensure_active` reseeds the Active child from `LastActivatedAt` when the marked child is
  despawned (and on load).
- Closing the last stack/tab in a space spawns the replacement **inside that space** (no
  cross-space churn).
- Slug: writing a per-space `startup_dir` keys by the canonical id and `resolve_startup_dir`
  returns it.
- Schema guard: a store written at an older schema version is reset rather than loaded.

## Risks

- Broad but mechanical: touches `vmux_layout` (space, tab, stack, pane, window, reconcile,
  snapshot), `vmux_space` (plugin), `vmux_core` (new `Active`), `vmux_desktop` (persistence),
  `vmux_setting` (override keying).
- CEF builds are heavy. Implement directly in this worktree off `origin/main` with a warm
  target dir (do not share `CARGO_TARGET_DIR` across worktrees — CEF cmake pins absolute
  paths). Prefer targeted package tests over a full workspace build during the edit loop.
- Pane recursion is the trickiest part of the `Active` invariant; the `ensure_active`
  safety-net keeps it robust even if a mutation forgets to move `Active`.
- The user always runtime-tests; verify the observable behavior (the layout the frontend
  renders and the snapshot the agent receives), not just internal ECS state.
