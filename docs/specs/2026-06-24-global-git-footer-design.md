# Global Git Footer — Design

## Goal

Move the repo-wide commit bar (commit message, Commit, Push, branch, ahead/behind, error) out of the editor page and into a **global footer** that spans the main content area beneath all panes — the same horizontal span as the header. This reinforces that a commit applies to the whole repo/workspace, not just the open file.

## Current State

- `GitFooter` (`crates/vmux_git/src/ui.rs:215`) renders **inside the editor page** (`crates/vmux_editor/src/page.rs:765`) — one pane. Its data (branch, ahead/behind, staged_count) and actions (commit, push) are **already repo-wide**; the file path is only used to locate the repo root (`runner.rs` `status`/`commit`/`push` all run at `repo_root`).
- `GitBar` (`ui.rs:107`) = per-file staging (accept all / deny all / unstage) — file-scoped.
- `DiffView` (`ui.rs:289`) = per-file diff.
- The git backend (`crates/vmux_git/src/plugin.rs`) routes every job result **back to the requesting webview** — so any webview can drive git status by sending `GitStatusRequest`.
- Layout page (`crates/vmux_layout/src/page.rs`) is a full-window transparent overlay (`fixed inset-0 pointer-events-none`) with floating islands: side sheet (left), header (top). It already receives `StacksHostEvent` (active stack `url`) and `LayoutStateEvent` (geometry: `header_left/right`, `window_pad_bottom`).
- Bevy UI tree (`crates/vmux_layout/src/window.rs`): `root`(row) → `[left side sheet, main_column]`; `main_column`(col) → `[Header (84px reserved, Open-gated), Main (panes, flex_grow)]`. Window `pad_bottom` is applied on `root`. `LayoutStateEvent` is emitted from `crates/vmux_browser/src/lib.rs:2134`.

## Decisions (locked)

- **Git scope: follow active pane.** Path = the active stack's `url` when it is a `file:` URL → repo. Terminal / browser / agent panes → no path → footer hidden. (Live terminal-cwd → git is a future feature.)
- **Visibility: only when changes.** Show when `staged_count > 0 || ahead > 0 || error`. Clean repo / no path → hidden.
- **Space: reserve (panes shrink).** Footer gets its own row beneath the panes; nothing is covered. Panes reflow up by `FOOTER_HEIGHT_PX` when it appears.

## Design

### Components (WASM / Dioxus)

- Keep `GitFooter` (`crates/vmux_git/src/ui.rs`) as the presentational bar — already props-driven, stays **layout-agnostic** (no layout-event knowledge), so `vmux_git` keeps zero internal deps.
- Add a **`FooterView`** component in `crates/vmux_layout/src/page.rs` (the layout overlay owns the orchestration, since it owns the layout events) that:
  - derives `path` from the active stack;
  - owns signals: branch, ahead, behind, staged_count, message;
  - adds the `GitStatusEvent` / `GitResultEvent` / `GitErrorEvent` listeners (same pattern `GitBar` uses today — ~3 listeners, duplicated rather than abstracted);
  - `use_effect` re-requests `GitStatusRequest` when `path` changes;
  - computes `should_show` and emits `FooterStateRequest { open }` on flip;
  - renders `GitFooter { ... }` when `should_show`;
  - `GitFooter`'s own Commit/Push buttons send `GitCommitRequest` / `GitPushRequest` with `path` (repo-wide, unchanged).
- **Crate dep:** add `vmux_git = { path = "../vmux_git" }` to `crates/vmux_layout/Cargo.toml`. One-way edge (`vmux_layout → vmux_git`); `vmux_git` has no internal deps → **no cycle**. Same pattern the editor already uses.
- Editor page: **remove** the `GitFooter` usage (`page.rs:765-772`) and the now-unused signal plumbing feeding it. Keep `GitBar` + `DiffView`.

### Geometry / reservation (Bevy)

- Add `FOOTER_HEIGHT_PX` constant in `crates/vmux_layout/src/event.rs`.
- New `Footer` component + node spawned in `main_column` **after** `Main` (`window.rs`): `height: FOOTER_HEIGHT_PX`, `flex_shrink: 0`, gated by the `Open` marker (mirror `Header`). No `Open` initially → not reserved.
- New `crates/vmux_layout/src/footer.rs` (filename-module pattern, no `mod.rs`) with `FooterLayoutPlugin` + `sync_footer_visibility`, mirroring `header.rs` (toggle node `height`/`display` on `Added<Open>` / `RemovedComponents<Open>`).
- WASM→Bevy toggle: new bin event **`FooterStateRequest { open: bool }`** in `event.rs`. The layout page emits it when the "should show footer" boolean flips. A Bevy observer adds/removes `Open` on the `Footer` entity → `Main` reflows.

### Positioning (WASM)

- Footer island in the layout overlay: `fixed bottom-[window_pad_bottom] left-[header_left()] right-[header_right()] h-[FOOTER_HEIGHT_PX]` — identical span to the header, aligning with the reserved Bevy `Footer` strip (`main_column` bottom = window bottom − `pad_bottom`). Reuse the existing `--vmux-*` CSS-var pattern.
- Active path: `stacks.find(is_active)`; if `url.starts_with("file:")` → strip scheme → fs path; else empty.

### Data flow

1. Active stack changes → layout page derives `path`.
2. `path` non-empty → emit `GitStatusRequest` from the layout webview (git plugin routes results back to requester — **no backend change**).
3. `GitStatusEvent` → update branch / ahead / behind / staged_count; compute `should_show`.
4. `should_show` flips → emit `FooterStateRequest { open }` → Bevy reserves/releases the row.
5. Render `GitFooter` island when `should_show`.
6. Commit / Push → `GitCommitRequest` / `GitPushRequest { path, message }` (repo-wide, unchanged). `GitResultEvent` clears the message + re-requests status.

## Non-goals (v1)

- Live terminal-cwd → git (only file-editor panes trigger the footer).
- Multi-repo aggregation within a space.
- Any change to commit/push semantics (already repo-wide).

## Testing

- **Rust unit:**
  - active-stack → path derivation: `file:` URL → path; terminal/browser/agent/empty → empty (pure fn).
  - `should_show` predicate (pure fn).
  - `FooterStateRequest` rkyv round-trip.
  - `sync_footer_visibility`: Bevy app test asserting the `Footer` node height toggles on `Open` add/remove (mirror the header pattern).
- **Existing** `vmux_git` status/commit/push runner tests: unchanged.
- **Manual:** editor pane with staged changes → footer appears beneath panes (panes shrink); Commit works + clears; switch to terminal/browser pane → footer hides + panes restore full height; side sheet open → footer left edge matches the header.
