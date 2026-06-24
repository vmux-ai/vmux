# Editor Minimap Design

Date: 2026-06-24
Status: Draft — awaiting spec review

## Goal

Add a VS Code-style minimap to the `files://` code editor: a narrow, syntax-colored,
scaled-down overview of the whole file pinned to the right edge of the text view, with a
translucent viewport box showing the visible region. Click to jump, drag the box to scroll.

## Constraints / context

The editor is **server-driven and line-windowed**, not a real scrolling DOM:

- Host (`vmux_editor`, Bevy/CEF, native) owns the full file in `FileBuffer { language, lines: Vec<FileLine> }`
  with syntect-highlighted `StyledSpan`s. It streams only the visible window to the page via
  `FileViewportPatch { first_line, total_lines, lines }` (`emit_window`, `plugin.rs:340`).
- Frontend (`page.rs`, Dioxus/WASM) holds only the windowed slice in the `lines` signal — never the whole file.
- Scroll is **line-index based**: wheel/keyboard compute a `top_line` and emit `FileScrollEvent { top_line }`;
  the host re-windows and pushes a new patch. Signals `first_line` / `total_lines` / `rows` already track position.
- The editor is **view-only** (modes Text/Dir/Image; keydown only navigates — no editing). So the file's
  content is static for the lifetime of a view (until reload), which means the minimap overview can be a
  **one-shot payload per file open**, refreshed only on file-watch reload.

A minimap needs a representation of the *entire* file. That data does not exist on the frontend today, so the
core of this work is a new host→page overview payload plus a canvas painter. The viewport box and all scrolling
reuse existing state and the existing `FileScrollEvent` loop.

Note: unlike `vmux_layout`'s `command_bar/page.rs`, `vmux_editor/page.rs` has **no `include_str!` source-scrape
tests**, so it can be edited freely.

## Approach (chosen: full code-preview)

Canvas-based, matching how VS Code renders its minimap. Per-line colored run-rectangles painted to a `<canvas>`
(not DOM-per-line, which would choke on large files). Fit-to-height layout (whole file always visible; no
secondary minimap scrolling in v1). Always-on (no toggle in v1).

### 1. Wire types — `crates/vmux_core/src/event.rs`

New event constant and rkyv/serde types, alongside the existing `File*` family:

```rust
pub const FILE_OVERVIEW_EVENT: &str = "file_overview";

pub struct MinimapRun { pub fg: [u8; 3], pub start: u16, pub len: u16 }
pub struct MinimapLine { pub runs: Vec<MinimapRun> }
pub struct FileOverviewPatch { pub total_lines: u32, pub lines: Vec<MinimapLine> }
```

All derive the same set as `FileViewportPatch` (`Debug, Clone, PartialEq, Serialize, Deserialize, rkyv::*`).
A run is a maximal span of consecutive **non-whitespace** characters of one color; whitespace becomes gaps,
so indentation structure (code shape) is preserved like VS Code. Columns are character offsets (`start`, `len`).

### 2. Overview builder — shared pure fn in `crates/vmux_editor/src/minimap.rs` (new, not cfg-gated)

`viewport.rs` is the precedent: a dependency-light module compiled for native + wasm + test. The new `minimap.rs`
holds the pure, unit-tested logic used by both host (builder) and frontend (paint math):

- `build_overview(lines: &[FileLine]) -> Vec<MinimapLine>` — collapse each line's `StyledSpan`s into
  non-whitespace runs, tracking the char column. Caps runs/line at `MINIMAP_MAX_RUNS_PER_LINE` to bound
  pathological minified lines.
- `line_to_y(line, total_lines, height_px) -> f32` — fit-to-height position of a file line on the canvas.
- `viewport_box(first_line, rows, total_lines, height_px) -> (y_px, h_px)` — the translucent box rect.
- `y_to_top_line(y_px, height_px, total_lines, rows) -> u32` — map a click/drag y back to a `top_line`
  (clamped via existing `clamp_top_line`).
- `sample_step(total_lines, height_px) -> usize` — for fit-to-height, how many file-lines map to one canvas
  row; the painter steps by this so a 50k-line file doesn't paint 50k sub-pixel rows.

Constants: `MINIMAP_MAX_LINES` (skip overview entirely above this, e.g. 100_000) and `MINIMAP_MAX_RUNS_PER_LINE`.

### 3. Host emit — `crates/vmux_editor/src/plugin.rs`

- In `send_initial_meta` (non-error branch, after `FILE_META_EVENT`): if `buf.lines.len() <= MINIMAP_MAX_LINES`,
  build the overview from `buf.lines` and `commands.trigger(BinHostEmitEvent::from_rkyv(entity, FILE_OVERVIEW_EVENT, &patch))`.
  Re-sends automatically on page-ready because `FileInitialMetaSent` is cleared by `reset_file_sent_markers_on_page_ready`.
- In `reload_changed_files` (non-error branch, after `FILE_META_EVENT` + `emit_window`): rebuild and re-emit the
  overview, since the file content changed.
- **No new inbound event.** The minimap scrolls by emitting the existing `FileScrollEvent`, already registered in
  `BinEventEmitterPlugin` and handled by `on_file_scroll`.

### 4. Frontend — `crates/vmux_editor/src/page.rs`

- New signal `overview: Signal<Option<FileOverviewPatch>>`, fed by
  `use_bin_event_listener::<FileOverviewPatch>(FILE_OVERVIEW_EVENT, ...)`. Cleared to `None` on `FILE_META_EVENT`
  (new file) until its overview arrives, so the minimap hides during load and for over-cap files.
- **Layout:** wrap the `Mode::Text` body (when `!show_diff()`) in a relative flex row: existing scrolling text
  region (`flex-1`) + a fixed-width minimap column (`~120px`, `shrink-0`). Hidden when `overview()` is `None`.
- **Canvas painter:** a `<canvas>` filled via `web_sys` `CanvasRenderingContext2d`. A `use_effect` reacting to
  `overview()` and the canvas pixel size paints once per overview change: for each sampled line, draw its runs as
  filled rects (`x = start * scale_x`, `w = len * scale_x`, `y = line_to_y(...)`), colored from `fg` with low alpha
  for the soft look. Canvas backing-store sized to `clientWidth/Height * devicePixelRatio` for crisp rendering;
  resize handled by a `ResizeObserver` (same pattern as `setup_measurement`).
- **Viewport box:** a translucent absolutely-positioned `div` over the canvas, positioned from
  `viewport_box(first_line(), rows, total_lines(), height)`. It repositions reactively on scroll **without
  repainting the canvas** (cheap). `rows` derived from `cell_dims` + container height (same math as host's
  `rows_from_viewport`).
- **Interaction:** `onmousedown` on the minimap → `y_to_top_line` → emit `FileScrollEvent`; set a `dragging`
  signal; `onmousemove` while dragging repeats; `onmouseup`/leave clears it. Click = jump (single mousedown).
  Reuses the existing scroll loop end-to-end; the host echoes back a patch that moves `first_line`, confirming.

## Data flow

```
file open ─▶ host build_overview(buf.lines) ─▶ FileOverviewPatch ─▶ page `overview` signal ─▶ canvas paint
scroll/drag ─▶ FileScrollEvent { top_line } ─▶ host on_file_scroll ─▶ FileViewportPatch ─▶ first_line ─▶ box moves
file reload ─▶ host rebuild overview ─▶ FileOverviewPatch ─▶ repaint
```

## Error / edge handling

- Over-cap files (`> MINIMAP_MAX_LINES`): host skips the overview event; frontend leaves `overview = None`;
  minimap simply doesn't render. No error surfaced.
- Empty file / zero rows: `viewport_box` and `y_to_top_line` clamp via existing `clamp_top_line`; box collapses
  or fills; no panic.
- `__error__:` buffers and Dir/Image modes: no overview emitted (guarded by the existing non-error / Text path).
- Very tall files within cap: `sample_step` keeps paint cost ~O(canvas height), not O(total_lines).

## Testing

Pure unit tests in `minimap.rs` (native, fast — no CEF, no WASM):

- `build_overview`: whitespace becomes gaps; adjacent same-color non-whitespace merges into one run; per-line run cap.
- `viewport_box`: top of file, middle, clamped at end, file shorter than viewport.
- `y_to_top_line`: round-trips with `viewport_box`; clamps at 0 and max scroll.
- `sample_step`: ≥1; collapses many lines/pixel for tall files.
- rkyv round-trip for `FileOverviewPatch` in `vmux_core` (mirrors the existing `FileViewportPatch` test).

Frontend behavior is verified by the observable output — dragging the minimap emits `FileScrollEvent { top_line }`
— consistent with how scroll is already tested (assert on the emitted event, not internal DOM state). Final visual
confirmation is a manual runtime test by the user.

## Out of scope (v1)

- VS Code-style minimap that *scrolls* when the file is taller than the minimap (fit-to-height instead).
- A show/hide toggle or width/side configuration.
- Live updates while editing (editor is view-only).
- Search-match / diff / error decorations on the minimap.

## Files touched

- `crates/vmux_core/src/event.rs` — new const + 3 types + rkyv test.
- `crates/vmux_editor/src/minimap.rs` — new shared module (builder + paint math + tests).
- `crates/vmux_editor/src/lib.rs` — `pub mod minimap;`.
- `crates/vmux_editor/src/plugin.rs` — emit overview in `send_initial_meta` + `reload_changed_files`.
- `crates/vmux_editor/src/page.rs` — signal, listener, minimap column, canvas painter, box, drag handlers.
