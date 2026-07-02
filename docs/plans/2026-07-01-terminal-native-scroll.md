# Terminal Native Scroll (editor-parity) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Do NOT subagent-drive this plan — CEF builds are huge and long-lived subagents drop the dev socket (`feedback_subagent_cef_fragility`). Execute inline against a warm target dir; the user runtime-tests at the end (`feedback_finish_then_test`).

**Goal:** Make primary-screen terminal scrolling as fast as the `file://` editor by moving to native GPU-compositor scroll over a windowed DOM (no backend round-trip during normal scroll), and fix the editor's edge-stall with viewport-relative buffering.

**Architecture:** Mirror the editor. The terminal frontend becomes an `overflow-auto` scroll container with a full-height spacer and only a windowed slice of document rows in the DOM. Scroll position (`scrollTop`) is derived from a document-row coordinate (row 0 = oldest scrollback line). The out-of-process `vmux_service` serves any document-row window by direct alacritty `Line` indexing (`grid[Line(doc_row - history_size)]`, `display_offset`-free) and streams the bottom window autonomously while the frontend is "following". Alt-screen / mouse-mode / copy-mode keep today's per-notch passthrough. Shared windowing math lives in `vmux_core::scroll`.

**Tech Stack:** Rust, Bevy (host plugin), Dioxus/WASM (page), `alacritty_terminal` 0.26.0 (grid), rkyv (CEF wire), `vmux_core` (shared types + math).

**Reference spec:** `docs/specs/2026-07-01-terminal-native-scroll-design.md`

**Status (2026-07-02):** Tasks 1–6 + workspace checks (T8) DONE on `feat/terminal-native-scroll` — native scroll is complete, workspace green (fmt/clippy/`cargo test --workspace`). **Task 7 (selection doc-coords) is DEFERRED to a follow-up PR**: drag-select calls `EnterCopyMode`, and copy-mode renders screen-relative via `display_offset` (which native scroll keeps at 0), so unifying selection coordinates needs a deliberate copy-mode↔native-scroll design, not a mechanical widen. This PR ships native scroll; selection keeps today's copy-mode behavior (correct when not scrolled). Do NOT delete this plan file until T7 lands.

**Key facts locked by grounding:**
- `grid[Line(n)]` (alacritty 0.26.0): valid `Line` range `Line(-history_size)..=Line(screen_lines-1)`; never reads `display_offset`. Document row → `Line`: `Line(doc_row as i32 - history_size as i32)`. `build_line(term, row_idx, offset)` and `hash_grid_row(term, row_idx, offset)` already compute `Line(row_idx - offset)`, so call them with `row_idx = doc_row`, `offset = history_size as i32`.
- `grid.total_lines()` = history + screen (grows with scrollback); `grid.screen_lines()` = visible rows; `grid.history_size() = total_lines - screen_lines`.
- Terminal Bevy observers registered at `crates/vmux_terminal/src/plugin.rs:377-383`; incoming CEF events at `BinEventEmitterPlugin::<(...)>` `plugin.rs:346`.
- Editor buffering constants today: `SCROLL_OVERSCAN = 48` (`vmux_editor/src/plugin.rs:21`), `SCROLL_EDGE = 16` (`vmux_editor/src/page.rs:23`).

**Build notes:** Warm the build first with a background `cargo build` (CEF is slow to cold-compile), then incremental builds are ~10-15s (`feedback_vmux_build_workflow`). Typecheck the page with `cargo check -p vmux_terminal --target wasm32-unknown-unknown`. Run `git checkout -- patches/` before committing if `cargo fmt` touched vendored crates (`feedback_cargo_fmt_patches`). Work only in this worktree: `.worktrees/terminal-native-scroll` (`feedback_worktree_edit_path`).

---

## File Structure

**Created:**
- `crates/vmux_core/src/scroll.rs` — pure, wasm-safe windowing + overscan math (shared by editor + terminal).

**Modified:**
- `crates/vmux_core/src/lib.rs` — `pub mod scroll;` (ungated).
- `crates/vmux_core/src/event.rs` — `TERM_SCROLL_EVENT`, `TermScrollEvent`, extend `TermViewportPatch` + `TermCursor`/`TermMouseEvent` (doc-row `u32`), `TermSelectionRange` (doc-row `u32`).
- `crates/vmux_editor/src/viewport.rs` — re-export from `vmux_core::scroll` (delete local impls + tests).
- `crates/vmux_editor/src/plugin.rs` — `emit_window` uses `overscan_for`.
- `crates/vmux_editor/src/page.rs` — `onscroll` uses viewport-relative trigger (`needs_refetch`).
- `crates/vmux_service/src/protocol.rs` — `ClientMessage::ScrollWindow`, extend `ServiceMessage::ViewportPatch`.
- `crates/vmux_service/src/process.rs` — doc-row cache, `view_top`/`following`, rewritten `sync_viewport`, `handle_scroll_window`, doc-row cursor/mouse.
- `crates/vmux_service/src/server.rs` — dispatch `ScrollWindow`.
- `crates/vmux_terminal/src/plugin.rs` — thread new patch fields, `on_term_scroll` observer + registration.
- `crates/vmux_terminal/src/page.rs` — native scroll container, spacer, windowed rows anchored to `first_row`, `onscroll`, follow-pin, alt/copy passthrough gate, doc-row mouse.

---

## Task 1: Shared scroll math in `vmux_core::scroll`

**Files:**
- Create: `crates/vmux_core/src/scroll.rs`
- Modify: `crates/vmux_core/src/lib.rs` (add `pub mod scroll;`)

- [ ] **Step 1: Write the module with failing-until-implemented tests**

Create `crates/vmux_core/src/scroll.rs`:

```rust
//! Pure scroll-windowing math shared by the file editor and the terminal.
//! Rows are `u32`/`u16` counts; no Bevy, no DOM — compiles for wasm.

/// Multiple of the visible row count to buffer beyond the viewport, per side.
pub const EDITOR_OVERSCAN_K: f32 = 1.5;
/// Terminal buffers more: its window refill crosses an extra process hop.
pub const TERMINAL_OVERSCAN_K: f32 = 2.0;
/// Minimum overscan (small panes still get a usable runway).
pub const OVERSCAN_FLOOR: u32 = 48;
/// Maximum overscan (bound DOM node count on very tall panes).
pub const OVERSCAN_CAP: u32 = 512;
/// Refetch trigger margin as a multiple of the visible row count.
pub const EDGE_TRIGGER_K: f32 = 1.0;

/// Clamp a requested top line so the viewport never scrolls past the last page.
pub fn clamp_top_line(top_line: u32, total_lines: u32, rows: u16) -> u32 {
    let max_top = total_lines.saturating_sub(rows as u32);
    top_line.min(max_top)
}

/// `[first, end)` line range for a viewport of `rows` starting at `top_line`.
pub fn window_range(total_lines: u32, top_line: u32, rows: u16) -> (u32, u32) {
    let first = clamp_top_line(top_line, total_lines, rows);
    let end = first.saturating_add(rows as u32).min(total_lines);
    (first, end)
}

/// Number of whole rows that fit in `viewport_height` at `char_height`.
pub fn rows_from_viewport(char_height: f32, viewport_height: f32) -> u16 {
    if char_height <= 0.0 || viewport_height <= 0.0 {
        return 0;
    }
    (viewport_height / char_height).floor() as u16
}

/// `[first, end)` as a `usize` range, for slicing an in-memory line buffer.
pub fn visible_slice(total: u32, top_line: u32, rows: u16) -> std::ops::Range<usize> {
    let (first, end) = window_range(total, top_line, rows);
    (first as usize)..(end as usize)
}

/// Rows to hold beyond the visible region on EACH side, scaled to the viewport
/// and clamped to `[floor, cap]`.
pub fn overscan_for(visible: u16, k: f32, floor: u32, cap: u32) -> u32 {
    let scaled = (visible as f32 * k).ceil() as u32;
    scaled.clamp(floor, cap)
}

/// True when the visible region is within `trigger` rows of the loaded window
/// edge, i.e. a refill should be requested now.
pub fn needs_refetch(
    vis_first: u32,
    vis_rows: u32,
    loaded_first: u32,
    loaded_len: u32,
    trigger: u32,
) -> bool {
    let loaded_end = loaded_first.saturating_add(loaded_len);
    let near_top = vis_first < loaded_first.saturating_add(trigger);
    let near_bot = vis_first + vis_rows + trigger > loaded_end;
    near_top || near_bot
}

/// alacritty grid `Line` for a document row (row 0 = oldest scrollback line):
/// `Line(doc_row - history_size)`. Returned as `i32` (may be negative = history).
pub fn doc_row_to_line(doc_row: u32, history_size: u32) -> i32 {
    doc_row as i32 - history_size as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_clamps_at_end() {
        assert_eq!(window_range(10, 8, 4), (6, 10));
    }

    #[test]
    fn window_from_top() {
        assert_eq!(window_range(10, 0, 4), (0, 4));
    }

    #[test]
    fn window_smaller_than_viewport() {
        assert_eq!(window_range(3, 0, 10), (0, 3));
    }

    #[test]
    fn clamp_caps_at_max_scroll() {
        assert_eq!(clamp_top_line(99, 10, 4), 6);
        assert_eq!(clamp_top_line(2, 10, 4), 2);
        assert_eq!(clamp_top_line(5, 3, 10), 0);
    }

    #[test]
    fn overscan_scales_and_clamps() {
        // 50 rows * 2.0 = 100, within [48, 512].
        assert_eq!(overscan_for(50, 2.0, 48, 512), 100);
        // small pane hits the floor.
        assert_eq!(overscan_for(10, 2.0, 48, 512), 48);
        // huge pane hits the cap.
        assert_eq!(overscan_for(400, 2.0, 48, 512), 512);
    }

    #[test]
    fn refetch_fires_near_edges_only() {
        // Loaded [100, 300), visible 50 rows, trigger 50.
        assert!(needs_refetch(120, 50, 100, 200, 50)); // near top
        assert!(needs_refetch(220, 50, 100, 200, 50)); // near bottom
        assert!(!needs_refetch(170, 50, 100, 200, 50)); // middle: no refetch
    }

    #[test]
    fn doc_row_maps_to_line() {
        // history 100: oldest doc row 0 -> Line(-100); newest visible -> >= 0.
        assert_eq!(doc_row_to_line(0, 100), -100);
        assert_eq!(doc_row_to_line(100, 100), 0);
        assert_eq!(doc_row_to_line(149, 100), 49);
    }
}
```

- [ ] **Step 2: Register the module** — in `crates/vmux_core/src/lib.rs`, add next to the other ungated `pub mod` lines (near `pub mod event;`, around line 6):

```rust
pub mod scroll;
```

(Do NOT put it behind `#[cfg(not(target_arch = "wasm32"))]` — it must compile for wasm; `vmux_core::event` is wasm-compiled too, per `reference_vmux_core_event_wasm`.)

- [ ] **Step 3: Run tests**

Run: `cargo test -p vmux_core scroll::`
Expected: all tests PASS.

- [ ] **Step 4: Commit**

```bash
cd .worktrees/terminal-native-scroll
git add crates/vmux_core/src/scroll.rs crates/vmux_core/src/lib.rs
git commit -m "feat(core): shared scroll windowing + viewport-relative overscan math"
```

---

## Task 2: Editor adopts shared math + viewport-relative buffering (fixes edge-stall)

**Files:**
- Modify: `crates/vmux_editor/src/viewport.rs` (replace impls with re-export)
- Modify: `crates/vmux_editor/src/plugin.rs:19,21,470-486`
- Modify: `crates/vmux_editor/src/page.rs:23,855-873`

- [ ] **Step 1: Re-export shared math from `viewport.rs`**

Replace the ENTIRE contents of `crates/vmux_editor/src/viewport.rs` (currently the four fns + tests) with:

```rust
//! Editor viewport math now lives in `vmux_core::scroll` (shared with the
//! terminal). Re-exported here so existing `crate::viewport::*` call sites are
//! unchanged.
pub use vmux_core::scroll::{clamp_top_line, rows_from_viewport, visible_slice, window_range};
```

(The unit tests moved to `vmux_core::scroll`.)

- [ ] **Step 2: Use viewport-relative overscan in `emit_window`**

In `crates/vmux_editor/src/plugin.rs`, delete the const at line 21:

```rust
const SCROLL_OVERSCAN: u32 = 48;   // DELETE this line
```

Then in `emit_window` (lines 483-485) replace:

```rust
    let (vis_first, vis_end) = window_range(visible, vp.top_row, vp.rows);
    let first_row = vis_first.saturating_sub(SCROLL_OVERSCAN);
    let end_row = (vis_end + SCROLL_OVERSCAN).min(visible);
```

with:

```rust
    let (vis_first, vis_end) = window_range(visible, vp.top_row, vp.rows);
    let overscan = vmux_core::scroll::overscan_for(
        vp.rows,
        vmux_core::scroll::EDITOR_OVERSCAN_K,
        vmux_core::scroll::OVERSCAN_FLOOR,
        vmux_core::scroll::OVERSCAN_CAP,
    );
    let first_row = vis_first.saturating_sub(overscan);
    let end_row = (vis_end + overscan).min(visible);
```

- [ ] **Step 3: Use viewport-relative trigger in the editor `onscroll`**

In `crates/vmux_editor/src/page.rs`, delete the const at line 23 (`const SCROLL_EDGE: u32 = 16;`). Then replace the body of the `onscroll` handler (lines 863-872) with:

```rust
                                        let vis_first = (el.scroll_top() as f64 / ch).floor().max(0.0) as u32;
                                        let vis_rows = (el.client_height() as f64 / ch).ceil() as u32 + 1;
                                        let trigger = (vis_rows as f32 * vmux_core::scroll::EDGE_TRIGGER_K).ceil() as u32;
                                        let rfirst = first_row();
                                        let loaded_len = lines.read().len() as u32;
                                        if vmux_core::scroll::needs_refetch(vis_first, vis_rows, rfirst, loaded_len, trigger)
                                            && last_scroll_req() != vis_first
                                        {
                                            last_scroll_req.set(vis_first);
                                            let _ = try_cef_bin_emit_rkyv(&FileScrollEvent { top_row: vis_first });
                                        }
```

- [ ] **Step 4: Verify it builds and existing editor tests pass**

Run: `cargo test -p vmux_editor`
Expected: PASS (no test references `SCROLL_OVERSCAN`/`SCROLL_EDGE`; `viewport::` re-exports resolve).
Then typecheck the page: `cargo check -p vmux_editor --target wasm32-unknown-unknown`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/viewport.rs crates/vmux_editor/src/plugin.rs crates/vmux_editor/src/page.rs
git commit -m "feat(editor): viewport-relative scroll buffer (fixes edge-stall), share vmux_core::scroll"
```

---

## Task 3: Wire types — `TermScrollEvent` + extended `TermViewportPatch`

**Files:**
- Modify: `crates/vmux_core/src/event.rs`

This changes shared types; `vmux_service` and `vmux_terminal` will not build until Tasks 4-6. `vmux_core` itself must build + its own tests pass.

- [ ] **Step 1: Add the scroll-event constant** — after `pub const TERM_LINK_OPEN_EVENT` (line 11):

```rust
pub const TERM_SCROLL_EVENT: &str = "term_scroll";
```

- [ ] **Step 2: Add the `TermScrollEvent` struct** — near `TermViewportPatch` (after the patch `impl`, ~line 1096). Use the same derive set as `FileScrollEvent`:

```rust
/// Frontend → Bevy scroll intent for the terminal (CEF IPC). `follow = true`
/// means the frontend is pinned to the bottom; the service then streams the
/// bottom window autonomously (no per-tick round-trip).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TermScrollEvent {
    pub top_row: u32,
    pub follow: bool,
}
```

- [ ] **Step 3: Extend `TermViewportPatch`** — replace the struct body (lines 1075-1086) with:

```rust
pub struct TermViewportPatch {
    /// (document_row, line) pairs for rows that changed since last sync.
    /// Document row 0 = oldest retained scrollback line.
    pub changed_lines: Vec<(u32, TermLine)>,
    pub cursor: TermCursor,
    pub cols: u16,
    pub rows: u16,
    pub selection: Option<TermSelectionRange>,
    #[serde(default)]
    pub copy_mode: bool,
    /// When true, changed_lines is a full window rebuild (spawn/resize/scroll jump).
    pub full: bool,
    /// Document row of the first line of the served window.
    #[serde(default)]
    pub first_row: u32,
    /// Total document rows (history + screen) → spacer height.
    #[serde(default)]
    pub total_rows: u32,
    /// Alt-screen active → frontend uses passthrough (non-native) scroll.
    #[serde(default)]
    pub alt: bool,
    /// RESERVED: lines permanently evicted off the top. Always 0 in v1.
    #[serde(default)]
    pub evicted_total: u64,
}
```

- [ ] **Step 4: Make `TermCursor.row` document-scoped (`u32`)** — find `pub struct TermCursor` in `event.rs` and change its `pub row: u16,` field to `pub row: u32,`. (Cursor now lives in document-row space. `col` stays `u16`.)

- [ ] **Step 5: Fix the in-crate test helper** — `event.rs` has `fn patch(changed_rows: Vec<u16>, cols: u16, rows: u16, full: bool) -> TermViewportPatch` (~line 1564). Update it to the new shape:

```rust
    fn patch(changed_rows: Vec<u32>, cols: u16, rows: u16, full: bool) -> TermViewportPatch {
        TermViewportPatch {
            changed_lines: changed_rows
                .into_iter()
                .map(|r| (r, TermLine::default()))
                .collect(),
            cursor: TermCursor::default(),
            cols,
            rows,
            selection: None,
            copy_mode: false,
            full,
            first_row: 0,
            total_rows: rows as u32,
            alt: false,
            evicted_total: 0,
        }
    }
```

Update any test that calls `patch(vec![0u16, ...], ...)` to pass `u32` literals, and any test asserting on `changed_lines` tuples (`(u16, _)` → `(u32, _)`). Run the next step to find them.

- [ ] **Step 6: Verify `vmux_core` builds + tests pass**

Run: `cargo test -p vmux_core`
Expected: PASS. If a test fails to compile on `(u16, _)` vs `(u32, _)` or `TermCursor.row`, fix the literal/type at the reported line and re-run.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_core/src/event.rs
git commit -m "feat(core): TermScrollEvent + windowed TermViewportPatch (doc-row u32, first_row/total_rows/alt)"
```

---

## Task 4: Service — window serving, `ScrollWindow`, doc-row cache, following

**Files:**
- Modify: `crates/vmux_service/src/protocol.rs`
- Modify: `crates/vmux_service/src/process.rs`
- Modify: `crates/vmux_service/src/server.rs`

- [ ] **Step 1: Add the client message + extend the patch (protocol.rs)**

In `crates/vmux_service/src/protocol.rs`, add a variant to `ClientMessage` (enum at line 314):

```rust
    ScrollWindow {
        process_id: ProcessId,
        top_row: u32,
        follow: bool,
    },
```

Then extend `ServiceMessage::ViewportPatch` (same crate) — change `changed_lines: Vec<(u16, TermLine)>` to `Vec<(u32, TermLine)>` and add fields to match the CEF patch:

```rust
    ViewportPatch {
        process_id: ProcessId,
        changed_lines: Vec<(u32, TermLine)>,
        cursor: TermCursor,
        cols: u16,
        rows: u16,
        selection: Option<TermSelectionRange>,
        copy_mode: bool,
        full: bool,
        first_row: u32,
        total_rows: u32,
        alt: bool,
        evicted_total: u64,
    },
```

- [ ] **Step 2: Add process fields (process.rs)**

Find the `Process` struct and its constructor (the `line_hashes` field is declared there; it is a `Vec<u64>` today). Change the field:

```rust
    // was: line_hashes: Vec<u64>,
    line_hashes: std::collections::HashMap<u32, u64>,   // keyed by document row
    view_top: u32,      // frontend's window top (document row); ignored while following
    following: bool,    // frontend pinned to bottom → stream bottom window
```

In the constructor, initialize:

```rust
    line_hashes: std::collections::HashMap::new(),
    view_top: 0,
    following: true,
```

Remove any remaining `self.line_hashes.resize(...)` / index-based usage (the old `sync_viewport` and `scroll_viewport` used `Vec`); those are replaced in Steps 3-4. Note: `scroll_viewport` (lines 863-872) is still called by copy-mode via `scroll_copy_mode_viewport`? No — copy-mode uses `scroll_copy_mode_viewport`. `scroll_viewport` at 863 calls `self.line_hashes.clear()`; `HashMap::clear()` exists, so leave `scroll_viewport` compiling but it becomes unused for primary (kept only if referenced). If `scroll_viewport` is now unused, delete it and the `else if self.scroll_viewport(delta) != 0` branch in `handle_mouse_wheel` (that branch handled primary-screen wheel, now done natively — see Step 5).

- [ ] **Step 3: Rewrite `sync_viewport` to serve the document-row window (process.rs:1240)**

Replace the whole `sync_viewport` fn with:

```rust
    fn sync_viewport(&mut self) {
        let grid = self.term.grid();
        let screen = grid.screen_lines();
        let num_cols = grid.columns();
        let total_rows = grid.total_lines() as u32;
        let history = total_rows.saturating_sub(screen as u32);
        let visible = screen as u16;

        // Window bounds in document space (row 0 = oldest scrollback line).
        let overscan = vmux_core::scroll::overscan_for(
            visible,
            vmux_core::scroll::TERMINAL_OVERSCAN_K,
            vmux_core::scroll::OVERSCAN_FLOOR,
            vmux_core::scroll::OVERSCAN_CAP,
        );
        let view_top = if self.following {
            history // first visible row at the bottom = history_size
        } else {
            vmux_core::scroll::clamp_top_line(self.view_top, total_rows, visible)
        };
        let first_row = view_top.saturating_sub(overscan);
        let end_row = (view_top + visible as u32 + overscan).min(total_rows);

        // doc_row -> Line(doc_row - history): reuse build_line/hash_grid_row with offset = history.
        let offset = history as i32;

        let mut changed_lines = Vec::new();
        let mut live: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
        for doc_row in first_row..end_row {
            let hash = hash_grid_row(&self.term, doc_row as usize, offset);
            live.insert(doc_row, hash);
            if self.line_hashes.get(&doc_row) != Some(&hash) {
                changed_lines.push((doc_row, build_line(&self.term, doc_row as usize, offset)));
            }
        }
        // `full` window rebuild when the previous cache didn't cover this window
        // (spawn / resize / scroll jump — see handle_scroll_window clearing it).
        let full = self.line_hashes.is_empty()
            || changed_lines.len() as u32 == end_row.saturating_sub(first_row);
        self.line_hashes = live; // prune to the served window

        // Cursor in document-row space; visible only when its row is in the window.
        let cursor_point = grid.cursor.point;
        let cursor_doc_row = history + cursor_point.line.0 as u32;
        let cursor_col = cursor_point.column.0 as u16;
        let cursor_in_window = cursor_doc_row >= first_row && cursor_doc_row < end_row;
        let cursor_char = {
            let cell = &grid[cursor_point.line][cursor_point.column];
            cell.c.to_string()
        };
        let alt = self
            .term
            .mode()
            .contains(alacritty_terminal::term::TermMode::ALT_SCREEN);

        // Drop a selection whose rows were mutated (browser-style), as before.
        if self.copy_mode.is_none()
            && let Some(sel) = self.selection
            && changed_lines.iter().any(|(row, _)| {
                let lo = sel.start_row.min(sel.end_row);
                let hi = sel.start_row.max(sel.end_row);
                *row >= lo && *row <= hi
            })
        {
            self.selection = None;
        }

        let patch = ServiceMessage::ViewportPatch {
            process_id: self.id,
            changed_lines,
            cursor: TermCursor {
                col: cursor_col,
                row: cursor_doc_row,
                shape: CursorShape::Block,
                visible: cursor_in_window,
                ch: cursor_char,
            },
            cols: num_cols as u16,
            rows: visible,
            selection: self.selection,
            copy_mode: self.copy_mode.is_some(),
            full,
            first_row,
            total_rows,
            alt,
            evicted_total: 0,
        };
        let _ = self.patch_tx.send(patch);
    }
```

Notes for the implementer:
- This removes the old cursor/`last_cursor`/`last_selection`/`last_viewport_copy_mode` change-suppression. Keep the *empty-patch skip* behavior if you want to preserve idle quiet: guard `if changed_lines.is_empty() && !full && cursor unchanged && selection unchanged { return; }` using the existing `self.last_cursor`/`self.last_selection` fields (keep them). Preserve them by reintroducing the same early-return that exists today, adapting the cursor tuple to `(cursor_col, cursor_doc_row)`. Do not drop this — it prevents needless broadcasts on every poll.
- `TermSelectionRange` row fields are still `u16` at this point; the comparison above uses `u16`. Task 7 migrates them to `u32`; after Task 7, the comparison types line up as `u32` automatically.
- If `TermCursor.row` field ordering differs, match the actual field names/shape of the struct (it has `col, row, shape, visible, ch`).

- [ ] **Step 4: Add `handle_scroll_window` (process.rs)** — next to `handle_mouse_wheel`:

```rust
    pub fn handle_scroll_window(&mut self, top_row: u32, follow: bool) {
        self.following = follow;
        self.view_top = top_row;
        // Force a full window rebuild: the frontend's slot→doc-row mapping shifts
        // on a scroll, so every row in the new window must be re-sent.
        self.line_hashes.clear();
        self.sync_viewport();
    }
```

- [ ] **Step 5: Gate primary-screen wheel out of `handle_mouse_wheel` (process.rs:874)**

The primary-screen branch (`else if self.scroll_viewport(delta) != 0 { self.sync_viewport(); }`) is now handled by native scroll + `ScrollWindow`. Keep copy-mode and mouse-mode/alt branches; make the final `else` a no-op (primary-screen wheel arrives only via `ScrollWindow` now):

```rust
    pub fn handle_mouse_wheel(&mut self, up: bool, col: u16, row: u16, modifiers: u8) {
        use alacritty_terminal::term::TermMode;
        let delta = if up { 1 } else { -1 };
        if self.copy_mode.is_some() {
            if self.scroll_copy_mode_viewport(delta) != 0 {
                self.sync_viewport();
            }
            return;
        }
        let mode = self.term.mode();
        if mode.intersects(TermMode::MOUSE_MODE) {
            let bytes = sgr_mouse_wheel_bytes(up, col, row, modifiers);
            Self::write_input_to_writer(&self.pty_writer, &bytes);
        } else if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
            let bytes = alternate_scroll_bytes(up, mode.contains(TermMode::APP_CURSOR));
            Self::write_input_to_writer(&self.pty_writer, bytes);
        }
        // primary-screen scrollback is native (ScrollWindow); no PTY write here.
    }
```

If `scroll_viewport` is now unused, delete it. Keep `copy_mode`/`scroll_copy_mode_viewport` intact.

- [ ] **Step 6: Dispatch `ScrollWindow` (server.rs:343)**

Next to the `ClientMessage::MouseWheel { .. }` arm, add:

```rust
                ClientMessage::ScrollWindow { process_id, top_row, follow } => {
                    with_process_mut(&manager, process_id, |process| {
                        process.handle_scroll_window(top_row, follow)
                    })
                    .await;
                }
```

- [ ] **Step 7: Update / add service tests**

The existing `mouse_wheel_on_normal_screen_scrolls_into_scrollback` test (process.rs:1978) asserts the old primary-wheel path (display_offset changes + a viewport patch). Replace it with a `ScrollWindow`-based test. Add to the `#[cfg(test)]` module:

```rust
    #[test]
    fn scroll_window_serves_document_row_window() {
        let mut process = /* build a Process with an 80x24 pty, as the existing tests do */;
        // Print > 24 lines so content scrolls into history.
        for i in 0..60 {
            process.feed_pty_bytes(format!("line{i}\r\n").as_bytes()); // use the test's existing feed helper
        }
        process.poll(); // drain + sync

        // Follow = bottom window.
        process.handle_scroll_window(0, true);
        let bottom = recv_patch(&mut process); // use the test's existing broadcast receiver
        assert!(bottom.total_rows >= 60);
        assert_eq!(bottom.first_row, bottom.total_rows.saturating_sub(24).saturating_sub(/*overscan*/) );
        // Scroll to the very top.
        process.handle_scroll_window(0, false);
        let top = recv_patch(&mut process);
        assert_eq!(top.first_row, 0);
        assert!(top.changed_lines.iter().any(|(r, _)| *r == 0));
    }
```

Adapt the helpers (`feed_pty_bytes`, `recv_patch`, process construction) to whatever the existing tests in this module already use — mirror `mouse_wheel_on_normal_screen_scrolls_into_scrollback` and `full_text_includes_scrolled_off_history` (process.rs:1720). Delete the obsolete `mouse_wheel_*` assertion about primary-screen scrollback offset (that path no longer writes offset).

- [ ] **Step 8: Verify**

Run: `cargo test -p vmux_service`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/vmux_service/src/protocol.rs crates/vmux_service/src/process.rs crates/vmux_service/src/server.rs
git commit -m "feat(service): serve document-row windows, ScrollWindow intent, following stream"
```

---

## Task 5: Bevy host — thread patch fields + `on_term_scroll` observer

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs`

- [ ] **Step 1: Thread the new fields through the ViewportPatch handler (plugin.rs:1324-1367)**

Update the `ServiceMessage::ViewportPatch { .. }` destructure to include the new fields, and pass them into `TermViewportPatch`:

```rust
            ServiceMessage::ViewportPatch {
                process_id,
                changed_lines,
                cursor,
                cols,
                rows,
                selection,
                copy_mode,
                full,
                first_row,
                total_rows,
                alt,
                evicted_total,
            } => {
                for (entity, pid, _) in &terminals {
                    if *pid == process_id {
                        if !output_seen.contains(entity) {
                            let has_content =
                                changed_lines.iter().any(|(_, l)| line_has_content(l));
                            if shell_prompt_ready(has_content, cursor.col) {
                                commands.entity(entity).insert(ShellOutputSeen);
                            }
                        }
                        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                            continue;
                        }
                        let mut changed_lines = changed_lines;
                        for (_, line) in changed_lines.iter_mut() {
                            crate::link::annotate_links(line, None);
                        }
                        let patch = TermViewportPatch {
                            changed_lines,
                            cursor,
                            cols,
                            rows,
                            selection,
                            copy_mode,
                            full,
                            first_row,
                            total_rows,
                            alt,
                            evicted_total,
                        };
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            TERM_VIEWPORT_EVENT,
                            &patch,
                        ));
                        break;
                    }
                }
            }
```

- [ ] **Step 2: Add the `on_term_scroll` observer** — next to `on_term_mouse` (after it, ~line 2920):

```rust
fn on_term_scroll(
    trigger: On<BinReceive<TermScrollEvent>>,
    q: Query<&ProcessId, With<Terminal>>,
    service: Option<Res<ServiceClient>>,
) {
    let entity = trigger.event_target();
    let event = &trigger.payload;
    let Some(service) = service else { return };
    let Ok(pid) = q.get(entity) else { return };
    service.0.send(ClientMessage::ScrollWindow {
        process_id: *pid,
        top_row: event.top_row,
        follow: event.follow,
    });
}
```

Ensure `TermScrollEvent` is imported (it comes from `vmux_core::event`; add to the existing `use vmux_core::event::{...}` group alongside `TermMouseEvent`).

- [ ] **Step 3: Register the incoming route + observer**

In `plugin.rs`, add `TermScrollEvent` to the `BinEventEmitterPlugin::<(...)>` tuple at line 346:

```rust
            .add_plugins(BinEventEmitterPlugin::<(
                TermResizeEvent,
                TermMouseEvent,
                TermScrollEvent,
                TermKeyEvent,
                TermLinkOpenRequest,
            )>::for_hosts(&["terminal"]))
```

And add the observer next to `on_term_mouse` at line 379:

```rust
            .add_observer(on_term_mouse)
            .add_observer(on_term_scroll)
```

- [ ] **Step 4: Verify (native host build)**

Run: `cargo build -p vmux_terminal`
Expected: PASS. (This is the heavy CEF-adjacent crate; ensure the target dir is warm.)

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_terminal/src/plugin.rs
git commit -m "feat(terminal): forward TermScrollEvent → ScrollWindow, thread windowed patch fields"
```

---

## Task 6: Frontend — native scroll container, spacer, windowed rows, follow-pin

**Files:**
- Modify: `crates/vmux_terminal/src/page.rs`

This is the core UX change. Mirror `vmux_editor/src/page.rs`. Reuse `TerminalRow` and `render_span` unchanged.

- [ ] **Step 1: Add scroll consts + page-state signals**

Near the top consts (lines 22-25), add:

```rust
const SCROLL_ID: &str = "term-scroll";
```

In `Page()` after the existing signals (after line 39), add:

```rust
    let mut first_row = use_signal(|| 0u32);
    let mut total_rows = use_signal(|| 0u32);
    let mut alt = use_signal(|| false);
    let mut last_scroll_req = use_signal(|| u32::MAX);
```

(`rows`/`cursor_rows` remain `Vec<Signal<..>>` but now represent the *windowed* slice anchored at `first_row`.)

- [ ] **Step 2: Rewrite the `TERM_VIEWPORT_EVENT` listener (page.rs:46-105)** to anchor rows at `first_row`, rebuild on window shift, and follow-pin:

```rust
    let _listener =
        use_bin_event_listener::<TermViewportPatch, _>(TERM_VIEWPORT_EVENT, move |patch| {
            let window_moved = *first_row.peek() != patch.first_row;
            let want_len = patch
                .changed_lines
                .iter()
                .map(|(r, _)| r.saturating_sub(patch.first_row) as usize + 1)
                .max()
                .unwrap_or(0)
                .max(rows.peek().len());

            if patch.requires_row_rebuild(*cols.peek(), rows.peek().len() as u16)
                || window_moved
            {
                resize_row_signals(&mut rows, want_len);
                resize_cursor_row_signals(&mut cursor_rows, want_len);
                first_row.set(patch.first_row);
            }
            total_rows.set(patch.total_rows);
            if *alt.peek() != patch.alt {
                alt.set(patch.alt);
            }

            let base = *first_row.peek();
            let targets = rows.with_peek(|row_signals| {
                patch
                    .changed_lines
                    .iter()
                    .filter_map(|(doc_row, line)| {
                        let i = doc_row.checked_sub(base)? as usize;
                        row_signals.get(i).copied().map(|row| (row, line.clone()))
                    })
                    .collect::<Vec<_>>()
            });
            for (mut row, line) in targets {
                if *row.peek() != line {
                    row.set(line);
                }
            }

            // Cursor: place at (cursor.row - first_row) within the window.
            if cursor.peek().as_ref() != Some(&patch.cursor) {
                let next_cursor = patch.cursor.clone();
                let cur_i = next_cursor.row.checked_sub(base).map(|i| i as u32);
                let prev_i = cursor
                    .peek()
                    .as_ref()
                    .and_then(|c| c.row.checked_sub(base))
                    .map(|i| i as u32);
                let targets = cursor_rows.with_peek(|row_signals| CursorRowSignalUpdate {
                    clear: prev_i.and_then(|row| row_signals.get(row as usize).copied()),
                    set: cur_i.and_then(|row| row_signals.get(row as usize).copied()),
                });
                if let Some(mut clear) = targets.clear
                    && clear.peek().is_some()
                {
                    clear.set(None);
                }
                if let Some(mut set) = targets.set
                    && *set.peek() != Some(next_cursor.clone())
                {
                    set.set(Some(next_cursor.clone()));
                }
                cursor.set(Some(next_cursor));
            }
            if *cols.peek() != patch.cols {
                cols.set(patch.cols);
            }
            if *selection.peek() != patch.selection {
                selection.set(patch.selection);
            }
            if *copy_mode.peek() != patch.copy_mode {
                copy_mode.set(patch.copy_mode);
            }

            // Follow-pin: when following (bottom), keep the viewport at the end.
            if last_scroll_req() == u32::MAX || is_following() {
                pin_scroll_to_bottom();
            }
        });
```

Add two small helpers (near `scroll_el` you will add in Step 4):

```rust
fn is_following() -> bool {
    let Some(el) = scroll_el() else { return true };
    let dist = el.scroll_height() as f64 - el.scroll_top() as f64 - el.client_height() as f64;
    dist <= 2.0
}

fn pin_scroll_to_bottom() {
    if let Some(el) = scroll_el() {
        el.set_scroll_top(el.scroll_height());
    }
}
```

`cursor_row_update` is no longer used by this listener; keep the `CursorRowSignalUpdate` struct (still referenced) and delete `cursor_row_update` if it becomes unused (the compiler will flag it).

- [ ] **Step 3: Rewrite the container + row rendering (page.rs:190-396)** — native scroll + spacer + absolutely-positioned windowed rows, with alt/copy passthrough gate.

Replace the container `div` opening (line 191-195) `class`/`id` and the wheel handler so that native scroll is the default and passthrough is gated. Concretely:

- Change the container class from `... overflow-hidden ...` to conditionally `overflow-auto` when NOT in passthrough mode, `overflow-hidden` when in passthrough (alt or copy). Add `id: SCROLL_ID`. Compute a `passthrough` bool at the top of `rsx!`:

```rust
        let passthrough = alt() || copy_mode();
        let overflow = if passthrough { "overflow-hidden" } else { "overflow-auto" };
        let spacer_h = total_rows() as f64 * ch;
```

- Container:

```rust
        div {
            id: SCROLL_ID,
            tabindex: "0",
            class: "relative h-full w-full {overflow} bg-term-bg text-term-fg font-mono text-sm leading-tight select-none",
            style: "{theme_style}{cell_style}outline:none;",

            onwheel: move |e: Event<WheelData>| {
                // Native scroll on the primary screen; passthrough only for TUIs.
                if !passthrough {
                    return; // let the browser scroll natively
                }
                e.prevent_default();
                // ... existing notch-accumulation + emit_mouse loop (unchanged) ...
            },

            onscroll: move |_| {
                if passthrough { return; }
                let (_, ch) = cell_dims();
                if ch <= 0.0 { return; }
                let Some(el) = scroll_el() else { return; };
                let vis_first = (el.scroll_top() as f64 / ch).floor().max(0.0) as u32;
                let vis_rows = (el.client_height() as f64 / ch).ceil() as u32 + 1;
                let trigger = (vis_rows as f32 * vmux_core::scroll::EDGE_TRIGGER_K).ceil() as u32;
                let follow = is_following();
                let rfirst = first_row();
                let loaded_len = rows.read().len() as u32;
                let refetch = follow
                    || vmux_core::scroll::needs_refetch(vis_first, vis_rows, rfirst, loaded_len, trigger);
                if refetch && last_scroll_req() != vis_first {
                    last_scroll_req.set(vis_first);
                    let _ = try_cef_bin_emit_rkyv(&TermScrollEvent { top_row: vis_first, follow });
                }
            },

            // ... keep onmousedown / onmouseup / onmousemove / onkeydown / oncontextmenu ...
```

- Replace the inner padding wrapper + row loop (lines 370-394) with a spacer + absolutely-positioned rows (mirror editor page.rs:874-891):

```rust
            div {
                style: "padding:{padding}px;",
                div {
                    class: "relative",
                    style: "height:{spacer_h}px;",
                    {
                        let row_signals = rows();
                        let cursor_signals = cursor_rows();
                        let base = first_row();
                        rsx! {
                            for (i, line) in row_signals.iter().copied().enumerate() {
                                if let Some(row_cursor) = cursor_signals.get(i).copied() {
                                    {
                                        let top = (base + i as u32) as f64 * ch;
                                        rsx! {
                                            div {
                                                key: "{base + i as u32}",
                                                style: "position:absolute;left:0;right:0;top:{top}px;",
                                                TerminalRow {
                                                    row_idx: (base + i as u32) as usize,
                                                    line,
                                                    cursor: row_cursor,
                                                    selection,
                                                    cols,
                                                    theme,
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
```

Notes:
- `TerminalRow`'s `row_idx` is now the **document row** (used by `row_selection_cols` after Task 7). Keep passing it.
- Keep the mouse handlers, but their `(col, row)` must become document rows — done in Task 7 (`client_to_cell` gains `scrollTop`). For now they still compile (they pass `u16`); Task 7 migrates them.

- [ ] **Step 4: Add `scroll_el` helper** (mirror editor page.rs:1419):

```rust
fn scroll_el() -> Option<web_sys::Element> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(SCROLL_ID))
}
```

If `CONTAINER_ID` is still referenced by `client_to_cell`/`do_measure`, keep `CONTAINER_ID` equal to `SCROLL_ID` OR set the same `id` — simplest: set `CONTAINER_ID = "term-scroll"` too, or give the container both behaviors by reusing one id. Since `do_measure`/`client_to_cell` look up `CONTAINER_ID`, set `const CONTAINER_ID: &str = "term-scroll";` and drop the separate `SCROLL_ID` (use `CONTAINER_ID` in `scroll_el`). Pick one id for the scroll container and use it everywhere.

- [ ] **Step 5: Typecheck the page (wasm)**

Run: `cargo check -p vmux_terminal --target wasm32-unknown-unknown`
Expected: PASS. Fix any signal-capture / type errors the compiler reports (Dioxus closures move signals; `first_row`/`total_rows`/`alt`/`last_scroll_req` must be `mut` where `.set()` is called).

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_terminal/src/page.rs
git commit -m "feat(terminal): native scroll page — spacer + windowed rows, follow-pin, alt passthrough"
```

---

## Task 7: Selection & mouse in document-row coordinates

**Files:**
- Modify: `crates/vmux_core/src/event.rs` (`TermSelectionRange`, `TermMouseEvent` rows → `u32`)
- Modify: `crates/vmux_terminal/src/page.rs` (`client_to_cell` + mouse handlers + `row_selection_cols`)
- Modify: `crates/vmux_terminal/src/plugin.rs` (mouse-action → service, screen-row conversion for SGR)
- Modify: `crates/vmux_service/src/process.rs` (selection stored/emitted in doc rows; SGR uses screen row)

Without this, selection highlights the wrong rows (the frontend now passes document `row_idx` to `TerminalRow`).

- [ ] **Step 1: Widen the wire row types (event.rs)**

- `TermSelectionRange`: change `start_row: u16` and `end_row: u16` to `u32` (keep `start_col`/`end_col` `u16`, `is_block` bool).
- `TermMouseEvent`: change `row: u16` to `row: u32` (now a document row). `col` stays `u16`.

- [ ] **Step 2: Compute document rows from scroll position (page.rs `client_to_cell`)**

Update `client_to_cell` (page.rs:654) to add the scroll offset so `row` is a document row:

```rust
fn client_to_cell(
    client_x: f64,
    client_y: f64,
    padding: f64,
    (cw, ch): (f64, f64),
) -> Option<(u16, u32)> {
    if cw <= 0.0 || ch <= 0.0 {
        return None;
    }
    let container = web_sys::window()?.document()?.get_element_by_id(CONTAINER_ID)?;
    let rect = container.get_bounding_client_rect();
    let scroll_top = container.scroll_top() as f64;
    let x = client_x - rect.left() - padding;
    let y = client_y - rect.top() - padding + scroll_top;
    let col = (x / cw).floor().max(0.0) as u16;
    let row = (y / ch).floor().max(0.0) as u32;
    Some((col, row))
}
```

Update `mouse_to_cell` return type to `Option<(u16, u32)>` and every caller/`emit_mouse` call to pass `u32` row. `emit_mouse` signature: `fn emit_mouse(button: u8, col: u16, row: u32, ...)` and `TermMouseEvent { ..., row, ... }`.

- [ ] **Step 3: Compare document rows in `row_selection_cols` (page.rs:896)**

`row_selection_cols` currently takes a `row_idx` and compares to `selection.start_row`/`end_row` (both now `u32`) — update its `row_idx` parameter type to `u32` and ensure the caller at line 456 passes the document `row_idx` (`TerminalRow.row_idx` is already the document row from Task 6). Fix the arithmetic to `u32`.

- [ ] **Step 4: Service — selection in doc rows, SGR in screen rows (process.rs + plugin.rs)**

- In `plugin.rs`, `on_term_mouse` / `mouse_terminal_actions` build selection commands from `event.row` (now `u32` document row) — pass it through unchanged to the service selection command (widen any `u16` locals to `u32`).
- For the SGR mouse-report path (`sgr_mouse_wheel_bytes` and any button report), the PTY app expects a **screen** row. Convert in the service where the report is built: `screen_row = doc_row.saturating_sub(history) ` clamped to `0..screen_lines` (in alt-screen `history == 0`, so `screen_row == doc_row`). Locate where the service turns a mouse event into SGR bytes and apply this conversion. If the conversion point is in `plugin.rs` (host) instead, do it there using the last-known `rows`/`total_rows`; prefer doing it service-side where `history` is authoritative.
- Selection storage in `process.rs` (`store_selection`/`project_selection`, per `reference_terminal_selection_model`) now uses document rows directly and no longer needs display_offset projection on scroll (rows are absolute). Simplify: store the `TermSelectionRange` as received (doc rows); drop the display_offset re-projection.

- [ ] **Step 5: Verify**

Run: `cargo test -p vmux_core && cargo test -p vmux_service && cargo build -p vmux_terminal && cargo check -p vmux_terminal --target wasm32-unknown-unknown`
Expected: PASS. Fix type mismatches (`u16`↔`u32`) at reported sites.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_core/src/event.rs crates/vmux_terminal/src/page.rs crates/vmux_terminal/src/plugin.rs crates/vmux_service/src/process.rs
git commit -m "feat(terminal): selection + mouse in document-row coordinates"
```

---

## Task 8: Workspace checks + runtime verification

- [ ] **Step 1: Format, restoring vendored patches**

```bash
cargo fmt
git checkout -- patches/   # cargo fmt reformats vendored crates; keep only crates/ changes
```

- [ ] **Step 2: Clippy + full test**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Run: `cargo test --workspace`
Expected: PASS. (If `vmux_desktop::...incompatible_store_resets_layout_on_startup` is the only red, it is a known flake — `gh run rerun --failed`, not a real failure, per `reference_flaky_persistence_test`.)

- [ ] **Step 3: Commit any fmt/clippy fixups**

```bash
git add -A
git commit -m "chore: fmt + clippy for terminal native scroll"
```

- [ ] **Step 4: Runtime verification (user drives — `feedback_verify_observable_behavior`)**

Build and run the dev app, then confirm each:
- Primary screen: trackpad/wheel flick scrolls **smoothly** (no per-notch lag), matching the editor. This is the acceptance criterion.
- Scroll up while a command streams output → view **stays put**; scrolling back to the bottom **resumes following** (auto-pins to new output).
- Scroll near the top of a long scrollback → no stall at the loaded-window edge (the fix for the editor complaint applies here too — verify the editor as well).
- TUIs unaffected: `vim`, `less`, `htop` — wheel still drives their own scroll (passthrough); no native scrollbar appears.
- Selection: click-drag selects the correct cells while at the bottom AND after scrolling up; copy yields the selected text.
- Cursor renders at the correct cell when following; hidden when scrolled far up.
- Idle CPU stays low (no `Continuous` update mode regression).

- [ ] **Step 5: Open PR** (after runtime pass)

```bash
git push -u origin feat/terminal-native-scroll
gh pr create --title "feat(terminal): native (editor-parity) scroll" --body "<summary + spec link>"
```

Delete this plan file once merged.

---

## Self-Review Notes (author)

- **Spec coverage:** coordinate model (T3/T4), frontend native scroll + spacer + follow-pin + passthrough (T6), service window serving + ScrollWindow + following (T4), buffering shared + editor fix (T1/T2), selection doc-coords (T7), eviction deferred with reserved field (T3 field, no task — intentional per spec). ✓
- **Type consistency:** `changed_lines: Vec<(u32, TermLine)>`, `TermCursor.row: u32`, `TermSelectionRange.{start,end}_row: u32`, `TermMouseEvent.row: u32`, `client_to_cell -> (u16, u32)` — consistent across T3/T4/T6/T7. `TermScrollEvent { top_row: u32, follow: bool }` matches `ClientMessage::ScrollWindow`. ✓
- **Known soft spots for the implementer:** (a) preserve the idle-broadcast early-return in `sync_viewport` (Step 4.3 note); (b) unify the scroll-container element id used by `do_measure`/`client_to_cell`/`scroll_el` (Step 6.4); (c) SGR screen-row conversion point (Step 7.4) — verify against the actual mouse-report code path.
