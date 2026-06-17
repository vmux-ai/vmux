# Spaces as Live Directories, One `store.ron`

## Problem

Spaces persistence is split across many files and only ever holds **one** space in
the ECS world at a time:

- `spaces.ron` — a registry of `{id, name, profile}` records.
- `…/Application Support/Vmux/<profile>/profiles/<p>/spaces/<id>/space.ron` — one
  moonshine scene **per space** (its layout).
- Switching a space **saves** the active scene to its file, **despawns** the active
  tab tree, and **loads** the target file (`on_space_command`, three duplicated
  save→despawn→load blocks).

Consequences:

- The active-space selection wasn't persisted (always reset to the first record).
- The display name is duplicated (registry `name` + a stale `Name` component baked
  into each `space.ron`).
- Inactive spaces don't exist in the world, so their terminals/agents/browsers are
  torn down on switch — unlike tmux, where detached sessions keep running.

## Goal

Model spaces like **tmux sessions**: every space stays **live** (its terminals,
agents, and browser pages keep running), switching only changes what is **rendered
and focused**, and all non-settings state lives in **one** `store.ron`.

- **`store.ron`** (single moonshine scene) holds **all** saved state except
  settings: every space and its full layout (tabs/panes/stacks), plus browsing
  history. Active space and ordering are **components in the scene**, not a side file.
- A space's **working directory** is a real folder under `~/.vmux/spaces/<id>/`.
- Switching is **in-memory** — no file IO, nothing despawned.

## Non-goals

- No migration. Fresh start: existing `spaces.ron` / per-space `space.ron` are
  ignored (left on disk, harmless).
- Profiles: collapse to a single profile. CEF/chromium browser data stays shared in
  Application Support (unchanged).
- The Spaces web page UI and MCP/agent space tools keep their current surface
  (`new`/`attach`/`delete`/`rename` events); MCP/automation continues to see the
  **active** space only.

## Storage layout

```
~/Library/Application Support/Vmux/<profile>/
  settings.ron        # settings — separate, unchanged
  store.ron           # ALL other saved state (scene: spaces + layouts + history)
  <CEF chromium data…> # unchanged, shared across spaces

~/.vmux/spaces/
  default/            # cwd for the "default" space (terminals/agents start here)
  work/               # cwd for the "work" space
```

- `store.ron` path = `vmux_core::profile::shared_data_dir().join("store.ron")` (the
  same dir as `settings.ron`).
- Space cwd = `space_dir(id)` → `~/.vmux/spaces/<id>` (today it is `~/.vmux/<id>`; add
  the `spaces/` segment).

## `store.ron` contents (the scene model)

One moonshine scene. New/affected components on the **Space entity**:

- `Space` (existing marker), `Save` (existing).
- `SpaceId(String)` — **new**. Stable slug; names the cwd folder. Survives rename.
- `Name` (Bevy) — display name (pretty; may contain spaces/caps). Rename edits this,
  not `SpaceId`.
- `Order` (existing component, reused) — space ordering in the strip/list.
- `ActiveSpaceTag` — **new** zero-field marker on exactly one Space entity = the
  persisted active space.
- `Profile` — dropped from the model (single profile).

Each Space entity **owns its tab subtree** (see hierarchy). History entities
(`Url`/`Visit`) remain top-level in the same scene. The save allowlist
(`save_space_to_path`) gains `SpaceId` and `ActiveSpaceTag`; drops `Profile`.

`Main` (the window node) is **not** saved, so top-level `ChildOf(Main)` references
can't persist — same as today. On load we re-parent loaded **Space** entities to the
live `Main`; tab→Space references are intra-scene and persist as-is.

## ECS hierarchy change

Today: `Main → Tab → Pane(split) → Pane(leaf) → Stack → Browser`, and `Space` is a
standalone marker unrelated to the tab tree.

New: insert Space into the hierarchy.

```
Main → Space → Tab → Pane(split) → Pane(leaf) → Stack → Browser
```

- Tabs spawn `ChildOf(active Space)` instead of `ChildOf(Main)`
  (`spawn_requested_tab_layouts`, window.rs).
- Inactive Space nodes get `Visibility::Hidden`; Bevy cascades it, hiding the whole
  subtree in **one** place. The page/process keeps running; it just doesn't paint.

## Active-space scoping

Introduce `ActiveSpaceEntity(Entity)` (resource) pointing at the active Space; it is
recomputed whenever `ActiveSpaceTag` moves. The existing `ActiveSpace { record }`
resource stays as a derived id/name cache for cwd resolution and snapshots.

The following currently-global systems must scope to **children of the active
Space** (today they assume one space's worth of tabs in the world):

- `sync_tab_visibility` (tab.rs) — candidates = active space's tabs; inactive spaces
  already hidden via their Space node.
- `compute_focused_stack` / `focused_stack` (stack.rs) — widest blast radius; scope
  the `active_among` set to the active space.
- `push_tabs_host_emit` + `active_tab_siblings` (vmux_browser/lib.rs, tab.rs) — the
  tab strip lists the active space's tabs only.
- `handle_tab_commands`, `sync_tab_order` (tab.rs) — Next/Prev/Close/SelectIndex and
  ordering scoped to the active space.
- `build_layout_snapshot` + reconcile apply (snapshot.rs, reconcile.rs) — MCP/
  automation reads/writes the active space only.
- `request_default_layout` (window.rs) — "any tabs?" check scoped to active space.
- `rebuild_space_views`, `mark_dirty_on_change` (vmux_desktop/persistence.rs) —
  rebuild views for all loaded spaces; re-parent Space→Main; dirty-tracking spans all
  spaces (one combined file).
- `compute_boot_status` stack count (boot_status.rs); Spaces-page `tab_count`
  (`broadcast_spaces_to_views`, `space_rows`, vmux_space/plugin.rs) — count the
  active space's stacks.

## Lifecycle: fully live (tmux semantics)

- Inactive spaces keep **all** processes alive: terminals, agent CLIs, and CEF
  webviews are **not** despawned on switch.
- Rendering: only the active space paints. Hidden Space subtree ⇒ `Visibility::Hidden`
  ⇒ no UI/mesh draw. Hidden CEF webviews must stop painting and **not** wake the event
  loop (respect the `no_continuous_update_mode` rule; route real wakeups through
  `EventLoopProxy`, never `UpdateMode::Continuous`).

## Switching & CRUD (in-memory, no file IO)

- **attach(id)**: move `ActiveSpaceTag` to the target Space; set
  `ActiveSpaceEntity`; flip Visibility (hide previous, show target); recompute
  `FocusedStack` from the target's tabs. Debounced `store.ron` save.
- **new(name)**: slug → `SpaceId`; spawn `Space` (`ChildOf(Main)`) + an initial Tab
  subtree (startup url / prompt); `mkdir ~/.vmux/spaces/<id>`; make active.
- **delete(id)**: despawn that Space's subtree; refuse if it is the last space;
  reassign active to another Space. **Keep** the cwd folder on disk (it holds the
  user's files).
- **rename(id, name)**: set `Name` on the Space entity. cwd folder unchanged.
- **save**: one debounced `SaveWorld` → `store.ron` (all Space subtrees + history).
- **load (startup)**: `LoadWorld` `store.ron` → spawn all spaces; rebuild views for
  all; set active from `ActiveSpaceTag` (fallback: first by `Order`, else bootstrap).

## Bootstrap (fresh start)

On startup, if `store.ron` is absent or has no Space entities: spawn a single default
Space (`SpaceId("default")`, `Name "default"`, `ActiveSpaceTag`), `mkdir
~/.vmux/spaces/default`, and an initial tab. Old `spaces.ron` and per-space
`space.ron` files are ignored.

## Removed

- `SpaceRegistry` / `spaces.ron`; the `read/write_space_registry_from/to` path.
- The Application Support per-space layout tree (`profiles/<p>/spaces/<id>/space.ron`)
  and `space_layout_path_for`.
- The per-space `profile` field; `Profile` from the scene.
- The three save→despawn→load switch blocks (`on_space_command` attach/new,
  `apply_pending_space_switch`, `handle_open_in_new_space`) → replaced by in-memory
  activation.

## Phasing

Each phase compiles, passes tests, and is independently reviewable.

**Phase A — Structural (single space, behavior unchanged).** Add `SpaceId`,
`ActiveSpaceTag`, `ActiveSpaceEntity`. Window setup spawns a Space node under `Main`;
tabs spawn `ChildOf(active Space)`. Re-scope every system in *Active-space scoping*.
Keep current persistence working. Verify the app behaves identically with one space.
This is the bulk of the risk (≈15 systems).

**Phase B — Persistence + multi-space.** `store.ron` at `shared_data_dir()`;
`SaveWorld`/`LoadWorld` of all spaces + history; allowlist updates. In-memory
switching; `new`/`delete`/`rename` operate on Space entities; cwd folders at
`~/.vmux/spaces/<id>`; fully-live inactive spaces; fresh-start bootstrap.

**Phase C — Cleanup.** Delete the registry/index/per-space-file/profile code and the
old switch blocks. Verify hidden live CEF respects the wake/idle-CPU rule.

## Risks

- **Idle CPU**: N live hidden CEF webviews must be throttled; guard against
  `Continuous` regressions (existing `no_continuous_update_mode` test).
- **Scoping regressions**: ~15 systems convert from global to active-space; Phase A
  behavior-preservation tests are the safety net.
- **Load re-parenting**: Space→`Main` must re-parent on load (Main isn't saved);
  tab→Space must round-trip.

## Testing

- Unit: `SpaceId` slugging; active-selection fallback (tag → first-by-order →
  bootstrap).
- System (Bevy app): two Space subtrees in one world → only the active space's tab is
  visible; focus is scoped; tab strip lists only active space tabs; MCP snapshot
  returns active space only; tab Next/Prev stays within the active space.
- Persistence: `store.ron` round-trip with multiple spaces preserves layouts, order,
  and active; reload re-parents Space→Main.
- Lifecycle: switching does **not** despawn an inactive space's terminal entity
  (process stays alive).
