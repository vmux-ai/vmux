# Command Bar "Current Work" Section — Design

Date: 2026-07-01
Status: Approved (brainstorm)
Surfaces: Cmd+K command bar modal **and** `vmux://start` launcher

## Summary

Add a "current work" section to the command bar, rendered **after** the
`vmux://` page entries. It has two groups:

1. **Open-pane dirs** — the working directories of currently-open terminals and
   agents, deduped by path. Selecting one focuses an existing pane in that dir,
   or spawns a terminal there.
2. **Recent files** — files opened in the `file://` editor, sourced from the
   **existing browser navigation history** (frecency-ranked). Selecting one
   reopens the file in the editor.

The section shows on an empty query (immediately when the palette opens) and
while filtering. It renders identically in the Cmd+K modal and `vmux://start`
because both share `CommandPalette` / `filter_results` and both call
`build_command_bar_open_payload`.

## Goals

- Fast keyboard access to the dirs you're actively working in and the files
  you've recently opened, without hunting through tabs or the file tree.
- Recent files are **persisted like any other browser history entry** — no new
  bespoke store. File opens become first-class navigations.

## Non-goals

- Live shell `cwd` tracking (OSC 7). "Current" dir = the dir a pane was
  **launched** in. Documented, not solved here.
- A separate recent-files database. Recent files reuse the `Visit`/`Url` ECS
  history store.
- Reworking the existing history page or history-suggestion search (they gain
  `file://` entries as a natural consequence).

## Current state (verified)

Command bar lives in `crates/vmux_layout/src/command_bar/`.

- Rendered-item enum: `CommandBarResultItem` — `results.rs:6-52` (has
  `Terminal`, `Stack`, `Space`, `Command`, `Page`, `Navigate`, `File`,
  `History`).
- Ordering: `filter_results` — `results.rs:150-289`. vmux:// `page_results`
  pushed at `results.rs:176` (empty query) and `results.rs:229` (query). New
  groups append right after those calls.
- Payload builder: `build_command_bar_open_payload` — `handler.rs:1020-1085`,
  thin constructor `command_bar_open_payload` — `handler.rs:922-944`. Wire event
  `CommandBarOpenEvent` — `crates/vmux_command/src/event.rs:18-33`.
- **Two callers** of the payload builder: the modal handler
  (`handler.rs`) and the launcher (`crates/vmux_layout/src/start/plugin.rs:17`).
- Selection dispatch: `on_command_bar_action` — `handler.rs:1139-1471`
  (`"terminal"` focuses by pid via `focus_pane_entity`; `"open"` expands a path
  and spawns a terminal, or opens a URL).
- Snapshot pattern: `crates/vmux_command/src/snapshot.rs` (resources in the
  `WriteCommandBarSnapshots` set); updater template
  `crates/vmux_space/src/snapshot_updater.rs`.

Dir data:

- Terminals **and** agents are `Terminal` entities carrying
  `TerminalLaunch { cwd, kind }` (`crates/vmux_core/src/terminal.rs:11-19`;
  agents build it at `crates/vmux_agent/src/launch.rs:13-23`). One query covers
  both.
- Focus by pid exists; there is **no** dir→pane map yet (new logic).

History / file-open data:

- History is ECS: `Visit` + `VisitedUrl(→Url)` + `VisitCount` +
  `LastVisitedAt` + `PageMetadata`. Real recorder: `spawn_visits`
  (`crates/vmux_history/src/spawn.rs:83-131`), logic **inline**, driven by
  `WebviewCommittedNavigationEvent`. Skips `vmux://` + empty. Writes **no
  title**.
- `find_or_create_url` (`spawn.rs:7-31`) finds-or-creates a `Url` but does
  **not** bump counts or spawn a `Visit`; currently unused.
- `file://` editor views are **windowed** webviews, so they do **not** hit the
  OSR `spawn_visits` path → file opens are **not** in queryable history today.
- History components are `#[require(Save)]`, in the save allowlist, and reloaded
  on startup (`crates/vmux_desktop/src/persistence.rs:37-66,103`). But
  `mark_dirty_on_change` (`persistence.rs:119-148`) has **no** `Added<Visit>`
  watcher — visits persist only on the 60s periodic save or a coincidental
  dirty flush.
- Matching is exact string equality on `PageMetadata.url` (no normalization).
  The editor already strips the fragment (`clean_url`,
  `crates/vmux_editor/src/plugin.rs:207`), so `foo.rs#goto=10` and `#goto=40`
  dedupe to one `Url` entity.

## Design

### 1. Record `file://` opens into browser history (backend)

Chosen approach **A** (dedicated record path), rejected **B** (synthesize
`WebviewCommittedNavigationEvent` — loses title, leaks to `lechat_bridge` and
the extension manager, which also read that event).

- Extract the inline bump-or-create + spawn-`Visit` logic from `spawn_visits`
  into a shared `vmux_history` helper, e.g.
  `record_visit(commands, urls_query, url, title)`, that:
  - finds an existing `Url` by exact `url` match → bumps `VisitCount` +
    `LastVisitedAt`, else spawns `(Url, PageMetadata { url, title, .. },
    VisitCount(1), LastVisitedAt(now), CreatedAt(now))`;
  - spawns `(Visit, CreatedAt(now), VisitedUrl(url_e), transition)`;
  - **sets `PageMetadata.title`** (unlike the current inline path).
- Refactor `spawn_visits` to call the helper (title empty for browser visits, as
  today).
- `vmux_editor::handle_file_page_open` calls the helper at the
  `PageOpenHandled` point (`plugin.rs:219`), using `clean_url` (fragment
  stripped) + the filename title already computed in `new_file_view_bundle`
  (`plugin.rs:126-129`). Fires exactly once per open.
- `persistence.rs`: add `Added<Visit>` (or `Changed<VisitCount>`) to
  `mark_dirty_on_change` so opens save promptly (also fixes prompt persistence
  for browser visits).

Intended consequence: file opens now appear in the History page and existing
history search — they are navigations.

### 2. Open-pane dirs (backend snapshot)

- New resource `CommandBarWorkSnapshot` in `crates/vmux_command/src/snapshot.rs`,
  registered via `init_resource` in `crates/vmux_command/src/plugin.rs`.
- Dirs updater system in `vmux_terminal`, in the `WriteCommandBarSnapshots` set:
  `Query<(Entity, &TerminalLaunch), With<Terminal>>` → dedupe by `cwd` →
  `Vec<WorkDirSummary { path, kind, entity }>`. Covers terminals and agents.
  On collision keep the most-recently-active entity (for focus).

### 3. Recent files (backend)

- Top-N `file://` history entries by frecency (reuse `vmux_history` scoring,
  `query.rs`) → `Vec<RecentFileSummary { path, title, last_visited_at }>`.
- Feed into `CommandBarWorkSnapshot` (or directly into the payload). Cap N
  (~8). Query is read-only over the history ECS store.

### 4. Wire + render (frontend + both surfaces)

- Add field(s) to `CommandBarOpenEvent` (`event.rs:18-33`) with `#[serde(default)]`
  like `spaces`/`pages`. Add wire summary structs next to `CommandBarTab`.
- Thread `CommandBarWorkSnapshot` through `build_command_bar_open_payload` and
  **both** callers (modal handler + `start/plugin.rs`).
- Add `CommandBarResultItem::WorkDir { path, kind }` and
  `CommandBarResultItem::RecentFile { path, title, last_visited_at }`
  (`results.rs`). Render arms in `palette.rs` (shared by Modal + Start).
- In `filter_results`, `items.extend(...)` the work groups **immediately after**
  each `page_results` push (`results.rs:176` and `results.rs:229`). Order within
  the section: **dirs first, then recent files.**
- Empty query: cap each group (~8). Filtering: fuzzy-match both against query.

### 5. Selection actions

- `WorkDir` → if a terminal with that `cwd` is currently open, `focus_pane_entity`
  it; else spawn a terminal in that `cwd`. Extend the `"terminal"`/`"open"`
  handler in `handler.rs` with dir→pane dedupe (map `cwd`→entity from the
  snapshot, reuse `focus_pane_entity`).
- `RecentFile` → emit the existing `open` action with `file://<path>` → editor
  (`PageOpenRequest`, existing path). Respects the palette's open-target
  modifier.

## Data structures (sketch)

```rust
// crates/vmux_command/src/snapshot.rs
#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarWorkSnapshot {
    pub dirs: Vec<WorkDirSummary>,
    pub recent_files: Vec<RecentFileSummary>,
}

pub struct WorkDirSummary { pub path: String, pub kind: TerminalKind, pub entity: Entity }
pub struct RecentFileSummary { pub path: String, pub title: String, pub last_visited_at: i64 }
```

```rust
// crates/vmux_layout/src/command_bar/results.rs
enum CommandBarResultItem {
    // …existing…
    WorkDir { path: String, kind_label: String },
    RecentFile { path: String, title: String, last_visited_at: i64 },
}
```

## Testing

Native crate tests (per project pattern — `include_str!`/source-scrape tests in
`vmux_layout` only catch via native runs):

- `cargo test -p vmux_history` — `record_visit` helper: create → bump on repeat
  → dedupe by url → title set; `spawn_visits` still records after refactor.
- `cargo test -p vmux_desktop` — `mark_dirty_on_change` sets dirty on
  `Added<Visit>`.
- `cargo test -p vmux_command` — dirs updater dedupes by `cwd`; snapshot covers
  terminals + agents.
- `cargo test -p vmux_layout` — `filter_results` places the work group directly
  after `page_results` (empty + query branches); action mapping for `WorkDir`
  and `RecentFile`.

Manual (one pass at the end): open terminals in a couple of dirs + open a file;
confirm the section appears after vmux:// pages in both Cmd+K and `vmux://start`,
dirs focus/spawn correctly, recent file reopens, and the file shows in history
after restart.

## Risks / consequences

- File opens now appear in browser history + history search (intended). If
  undesired later, the recorder could tag file visits for filtering — out of
  scope now.
- No URL normalization: inconsistent percent-encoding/casing of a file path
  would create duplicate history entries. Editor produces a stable `clean_url`,
  so opens from the editor dedupe correctly.
- "Current" dir = launch dir, not live `cwd`. Acceptable; documented.

## Out of scope

- OSC 7 live cwd tracking.
- Pruning/limits policy for file history beyond existing history behavior.
- Cross-space ranking beyond frecency + simple dedupe.
