# Space Switcher in Command Bar — Design

Date: 2026-07-07
Status: Approved

## Problem

Switching spaces is tedious. `<leader> s` (`Ctrl+g, s` = `SpaceCommand::Open`) already
opens the command bar, but it seeds the query with the `vmux://spaces/` URL and the
default-selected first result is the *"open full spaces page"* nav item. A blind Enter
therefore navigates to the full-page `vmux://spaces/` view instead of switching.

Goal: `<leader> s` opens a clean, tmux-style space switcher inside the command bar —
select with Ctrl+N/P (or ↑/↓), Enter to switch — as fast as tmux `choose-tree`.

## Behavior

On `<leader> s`, the command bar opens in **space-switch mode**:

- Empty input, placeholder `Switch space…`.
- Results = spaces only, in **consistent `Order`** (stable index order like tmux, never
  MRU-reshuffled), followed by a trailing `Manage spaces…` row.
- The **current (active) space is pre-highlighted**, orienting the list like tmux's
  `choose-tree`. Ctrl+N/P / ↑/↓ move the selection; Enter switches.
- Typing filters the space list by name (stays spaces-only + `Manage spaces…`). No URL,
  history, tab, or command results appear in this mode.
- Enter on a space → existing `attach` path → instant switch (toggles `Display` on the
  persistent per-space container node; no reload).
- Enter on `Manage spaces…` → opens the full `vmux://spaces/` page, where create / rename
  / delete already live.

Keybinding is unchanged: reuse `<leader> s` (`Ctrl+g, s`). No new chord.

## Architecture

Reuse the command-bar palette with an explicit mode flag, rather than the current
URL-seeding hack. A flag keeps the visible query empty and avoids fragile
`vmux://spaces/` string matching in the frontend, and matches the intent of "just do it
in the command bar."

Downstream switch machinery is untouched — the `"space"` action already maps to
`SpaceCommandEvent { command: "attach" }`, handled by `on_space_command` in
`vmux_space`.

### Changes

1. **Wire flag** — add `space_switch: bool` to `CommandBarOpenRequest` and to the
   `CommandBarOpenEvent` rkyv payload (`vmux_command`). Append the field to preserve
   rkyv layout stability.
2. **Open mapping** — `command_bar_open_request` (`vmux_layout/.../handler.rs`):
   `SpaceCommand::Open` sets `space_switch = true` and an empty url (drop the
   `spaces_page_url` seed).
3. **Results** — `filter_results` (`vmux_layout/.../results.rs`): add a `space_switch`
   param. When set, return `space_list_items(spaces, filter)` in payload order plus a
   single trailing `Manage spaces…` item, and nothing else. Remove the
   `query_targets_spaces_page` seeding branch (superseded by the flag).
4. **Palette** — `palette.rs`: thread `space_switch` from the payload; force empty query
   and the `Switch space…` placeholder; on open in this mode, set `selected` to the
   active space's index; render `Manage spaces…` → `emit_action("open", spaces_page_url)`.

### Data already available

- The command-bar spaces snapshot (`update_spaces_snapshot`) already sorts by `Order`
  and marks the active space via `is_active`, so the frontend renders in order and
  pre-selects the active index with no new backend query.

## Out of scope / optional

- tmux-style index badges (`0 1 2…`) on rows and digit-to-jump. Deferred; can be added
  later without changing this design.
- Creating / renaming / deleting spaces from within the switcher (remains on the full
  `vmux://spaces/` page, reachable via `Manage spaces…`).

## Testing

Native tests (no runtime app needed):

- `vmux_layout` (`results.rs`): space-switch mode returns spaces in `Order` + a trailing
  `Manage spaces…`; name filter narrows the space list; no tab/command/url items leak in.
- `vmux_layout` (`handler.rs`): `SpaceCommand::Open` produces `space_switch = true`
  (update the existing test around `handler.rs:2305`).
- Pre-selection: active-space index helper returns the `is_active` row's position.

Manual verification (single pass at the end): `<leader> s` → Ctrl+N/P → Enter switches;
`Manage spaces…` opens the full page.
