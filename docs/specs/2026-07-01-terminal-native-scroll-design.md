# Terminal Native Scroll (editor-parity) — Design

Date: 2026-07-01
Crates: `vmux_terminal` (Dioxus `page.rs` + Bevy `plugin.rs`), `vmux_service`
(out-of-process alacritty grid), `vmux_core` (wire types + shared windowing math),
`vmux_editor` (adopts the shared buffering strategy)

## Goal

Make terminal scrolling as fast as the `file://` editor. Today scrolling a
terminal feels slow; scrolling the editor is smooth. Close the gap by giving the
terminal the editor's scroll architecture: **native GPU-compositor scroll over a
windowed DOM, with no backend round-trip during normal scroll.**

## Root cause (why terminal is slow, editor is fast)

**Editor (fast).** `vmux_editor/src/page.rs:846` renders an `overflow-auto`
container + a full-height spacer + only a windowed slice of lines (visible ±
overscan) absolutely positioned. The wheel gesture is handled by CEF's native
scroll on the compositor thread — no `onwheel`, nothing calls `prevent_default`.
The backend (`plugin.rs:470` `emit_window`) is pinged only when the visible region
nears the loaded window edge (`FileScrollEvent`, `plugin.rs:613`). Zero re-render,
zero round-trip during ordinary scroll.

**Terminal (slow).** `vmux_terminal/src/page.rs:235` cancels native scroll
(`e.prevent_default()`) and emits **one CEF IPC message per wheel notch** (up to 10
per flick). Each notch is a full cross-process round-trip: WASM → Bevy → socket →
`vmux_service` → scroll the alacritty grid by **exactly ±1 line** → re-serialize →
back. The service `line_hashes.clear()` (`process.rs`, in `scroll_viewport`)
defeats incremental diffing, so the **entire viewport is re-hashed, re-`build_line`d,
rkyv-serialized, link-scanned, and re-diffed into the DOM every notch**. No
throttle, debounce, or coalescing anywhere. A 1-line scroll shifts every row, so
the terminal *re-renders content*; the editor merely *moves a viewport*.

## Principles

- **Dumb frontend.** The Dioxus/WASM page computes no scroll geometry beyond
  reading its own native `scrollTop`. The service (source of truth = the alacritty
  grid) owns windowing, coordinate math, and line content. Frontend renders
  fully-resolved rows and emits intents (scroll position, mode-gated wheel
  passthrough). Consistent with the editor and `[[feedback_dumb_dioxus_frontend]]`.
- **Mirror the editor.** Reuse its proven shape — `overflow-auto` + spacer +
  windowed abs-positioned rows + edge-prefetch. Do not invent a second scroll
  model.
- **No new crate.** Shared windowing math lives in `vmux_core`
  (`[[feedback_no_new_crates]]`), which `vmux_service`, `vmux_editor`, and the WASM
  pages already depend on.
- **Regression-proof the TUIs.** Alt-screen / mouse-mode / copy-mode keep today's
  passthrough behavior exactly. Native scroll applies to the **primary screen
  only**.

## Core concept: document-row coordinate ⇄ alacritty `display_offset`

The terminal is a live stream, not a static file. Define a stable document-row
space and bridge it to alacritty:

```
document row 0   = oldest retained scrollback line
screen_lines     = grid.screen_lines()           (visible rows)
total_rows       = grid.total_lines()            (history + screen)  [process.rs:62]
history_size     = total_rows - screen_lines
first_visible    = history_size - display_offset  (display_offset 0 = bottom)
scrollTop        = first_visible * ch
```

Bijection (any native scroll position ⇄ a `display_offset`):

- Bottom (following): `display_offset = 0` → `first_visible = history_size`.
- Top (oldest): `display_offset = history_size` → `first_visible = 0`.
- To serve a requested `top_row`: `display_offset = clamp(history_size - top_row, 0, history_size)`.

**Coordinate stability.** New output historizes at the *bottom* (grows
`total_rows`); document row 0 (oldest) stays fixed, so top-anchored positions do
not move — no jump while scrolled up. The only shift is **scrollback eviction**
(ring buffer full → oldest line dropped), handled explicitly below.

## Frontend — `vmux_terminal/src/page.rs` (mirror `vmux_editor/page.rs`)

1. **Container.** `overflow-hidden` (`page.rs:194`) → `overflow-auto` `#term-scroll`
   + full-height spacer (`height = total_rows * ch`) + windowed rows abs-positioned
   at `top = doc_row * ch`. Same structure as editor `page.rs:846-891`.
2. **Primary-screen wheel = native.** Remove `e.prevent_default()` and the
   per-notch `emit_mouse` loop (`page.rs:252-263`) when the served patch says the
   screen is primary (`alt == false`). The browser scrolls on the compositor
   thread. **No round-trip.**
3. **Edge prefetch.** Add `onscroll`: derive `vis_first = scrollTop / ch`; if within
   the edge-trigger margin of the loaded window, emit `TermScrollEvent { top_row }`
   (dedup via a `last_scroll_req` signal). Direct port of editor `onscroll`
   (`page.rs:855-873`).
4. **Follow-pin.** Track "at bottom" (`scrollTop + client_height >= scrollHeight - ε`).
   When at bottom, new-output patches keep the view pinned (`scrollTop = scrollHeight`).
   When scrolled up, do not auto-jump on new output.
5. **Passthrough mode.** When the patch reports `alt` (alt-screen) or `copy_mode`,
   restore today's behavior: `overflow-hidden`, `prevent_default`, per-notch wheel
   passthrough (SGR / arrow-key bytes). Frontend switches on the flag; no other
   change to the TUI path.

## Backend service — `vmux_service`

- **New intent:** `ClientMessage::ScrollWindow { process_id, top_row: u32 }`
  (`server.rs`). Handler sets `display_offset = clamp(history_size - top_row)` and
  emits a windowed patch. This replaces the primary-screen `MouseWheel` round-trip;
  `MouseWheel` remains for alt/mouse-mode passthrough.
- **Windowed serialization.** New `fn window_lines(doc_first, count) -> Vec<(u32, TermLine)>`
  reads grid rows in document space (0 = oldest), independent of the live
  `display_offset`, via alacritty grid buffer-line indexing. Serve `visible ±
  overscan` (see buffering). Key `changed_lines` by **absolute document row**, not
  screen row.
- **Kill the per-notch cost.** Do not `line_hashes.clear()` on a window serve; send
  only the window's rows. Incremental live diffing (normal PTY output) is unchanged.
- **Live streaming.** The active-screen window keeps flowing as today, now stamped
  with `first_row` / `total_rows` so the frontend can place rows and size the spacer.

### Wire-type changes — `vmux_core/src/event.rs` (cfg-gated, wasm-safe — `[[reference_vmux_core_event_wasm]]`)

```rust
// New: frontend → Bevy scroll intent (CEF IPC), analogous to FileScrollEvent.
pub struct TermScrollEvent { pub top_row: u32 }

// Extend TermViewportPatch (currently: cols, rows, copy_mode, full, changed_lines, cursor, selection).
pub struct TermViewportPatch {
    // ...existing...
    pub first_row: u32,        // doc row of first line in changed_lines' window
    pub total_rows: u32,       // history + screen → spacer height
    pub alt: bool,             // alt-screen → frontend uses passthrough mode
    pub evicted_total: u64,    // monotonic count of lines permanently dropped off the top
    // changed_lines: now Vec<(u32 doc_row, TermLine)>
}
```

## Buffering strategy (shared `vmux_core`; fixes the editor edge-stall too)

Editor uses **fixed** row counts — `SCROLL_OVERSCAN = 48` (`vmux_editor/plugin.rs:21`),
`SCROLL_EDGE = 16` (`vmux_editor/page.rs:23`) — so a fast flick eats the 48−16 = 32-row
runway before the refill (round-trip + syntect highlight) lands → the stall the user
observes. It also does not scale with pane height.

Replace with **viewport-relative overscan + early trigger**, extracted into pure
`vmux_core::scroll` (wasm-safe), reused by editor and terminal service:

```rust
overscan = clamp(k_over * visible_rows, FLOOR, CAP)   // rows held beyond visible, each side
trigger  = k_edge * visible_rows                       // refetch this far before the loaded edge
runway   = overscan - trigger                          // ≈ ½–1 viewport hidden behind the fetch

fn clamp_top_line(top_row, total, visible) -> u32
fn window_range(total, top_row, visible, overscan) -> (first, end)
fn needs_refetch(vis_first, vis_rows, loaded_first, loaded_len, trigger) -> bool
```

Defaults (embedded constants; overridable via settings, read-with-fallback, never
auto-seeded — `[[feedback_no_config_autoseed]]`, `[[reference_settings_section_merge]]`):

| surface  | `k_over` | `k_edge` | rationale                              |
|----------|----------|----------|----------------------------------------|
| editor   | 1.5      | 1.0      | in-process refill                      |
| terminal | 2.0      | 1.0      | extra out-of-process hop → more cushion|

`FLOOR` ≈ 48 (small panes), `CAP` ≈ a few hundred rows (bound DOM node count on
huge panes). Fast flicks may still briefly show blank rows past the cap until fill —
acceptable, and the same behavior the editor already tolerates. If a hard flick
still stutters, the follow-up lever is **velocity-aware prefetch** (fetch distance
∝ scroll speed) — deferred (YAGNI).

## Selection

Move selection anchors to **document-row coordinates** (row 0 = oldest). Native
scroll moves the DOM; selection overlays live in the same absolute space, so no
per-tick re-projection is needed (today's viewport-coord model projects on every
scroll — `[[reference_terminal_selection_model]]`). Copy resolves document rows
back to grid content service-side.

## Eviction drift

While scrolled up, a full scrollback ring evicts the oldest line → every document
row shifts down by the evicted count. Patch carries monotonic `evicted_total`;
frontend tracks the last value and, when **scrolled up**, compensates on delta `D`:
`first_row -= D`, `scrollTop -= D * ch` → viewed content stays visually put. When
pinned to bottom, no action (already anchored to max). Growth at the bottom needs
no compensation (top-anchored positions are stable).

## Data flow

**Before (per notch):** WASM `onwheel` → CEF IPC (×N notches) → Bevy `on_term_mouse`
→ socket → service scroll ±1 → `line_hashes.clear()` → re-hash + `build_line` whole
viewport → rkyv → socket → Bevy link-scan whole viewport → CEF IPC → WASM diff all
rows. Every notch.

**After (normal scroll):** WASM native compositor scroll over windowed DOM — **stops
here.** Only when nearing the window edge: `onscroll` → `TermScrollEvent` → Bevy →
`ClientMessage::ScrollWindow` → service serves one window → patch → placed in spacer.
Rare, amortized by overscan.

## Testing

- `vmux_core::scroll` — unit tests for `clamp_top_line`, `window_range`,
  `needs_refetch`, overscan clamping (pure, native `cargo test`).
- Coordinate bijection — `first_visible = history_size - display_offset` round-trips
  across bottom / mid / top / clamp edges.
- Service — `ScrollWindow { top_row }` serves the correct document-row window
  (visible ± overscan); `window_lines` reads history correctly; `evicted_total`
  increments on ring overflow. Follow existing `vmux_service` process tests.
- Mode gating — alt-screen / copy-mode patches set `alt` / `copy_mode`; primary
  screen does not (assert on emitted patch, per `[[feedback_verify_observable_behavior]]`).
- Runtime scroll-feel — one manual pass at the end (`[[feedback_finish_then_test]]`):
  flick primary screen (native, smooth), scroll up + live output (stays put), TUI
  wheel still works (vim/less/htop), selection survives scroll, editor edge-stall
  gone.

## Scope / non-goals

- **In scope:** primary-screen native scroll (the 99% win), viewport-relative
  buffering for editor + terminal, alt/mouse/copy passthrough, selection doc-coords,
  eviction compensation.
- **Non-goals (v1):** velocity-aware prefetch; reflow-on-resize changes beyond what
  the windowed model needs; touching the unrelated agent-prompt bg-overlay scroll
  bug (`[[reference_terminal_bg_overlay_drift]]`) except where doc-coord rendering
  naturally subsumes it.

## Risks

- **alacritty history indexing.** `window_lines` must read arbitrary history rows in
  document space. Confirm the grid indexing API (buffer-line access independent of
  `display_offset`) early; today's `build_line(term, row_idx, offset)` already reads
  history via `display_offset`, so the capability exists — the task is a clean
  doc-space accessor.
- **Refill latency across the process hop.** Mitigated by the larger terminal
  overscan (`k_over = 2.0`); velocity-aware prefetch is the fallback if needed.
- **Follow-pin correctness under bursty output.** Must not fight the compositor
  (avoid `scrollTop` thrash). Pin only when already at bottom; never force-scroll
  while the user is scrolled up.
