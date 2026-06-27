# Editor Code Folding — Design

Date: 2026-06-27
Crate: `vmux_editor` (backend plugin + `EditCore`), `vmux_core` (wire types), `page.rs` (Dioxus frontend)

## Goal

Collapse/expand line regions in the file editor like VSCode and Vim. Folds are
computed from LSP `textDocument/foldingRange` with an indentation-based fallback.
Both gutter chevrons (mouse) and keyboard commands (Vim `z*`, VSCode shortcuts)
toggle folds. Fold state persists per file across app restarts.

## Principles

- **Dumb frontend.** The Dioxus/WASM page computes no fold geometry. The backend
  (Bevy plugin + `EditCore`, native Rust) owns all fold state, windowing,
  cursor/motion math, and persistence. The frontend renders fully-resolved rows
  and emits user intents (chevron click, keystroke, scroll). Single source of
  truth; testable with native `cargo test`.
- **Approach 1 (backend owns folds, windows in visual-row space).** Rejected the
  "frontend `display:none`s folded lines" alternative: it breaks virtualization
  (buffer-space windows + hidden lines give the wrong visible count / spacer
  height / hide off-window folds) and still needs rope-side fold-aware motions,
  splitting state across the wire.

## Core concept: buffer line vs visual row

- **Buffer line** — a line index in the rope. Stable; used for the gutter line
  number and all rope operations.
- **Visual row** — a line's position in the fold-collapsed view. Hidden lines
  (inside a collapsed region's body) occupy zero rows.

A `FoldRegion { start, end }` has header line `start` and collapsible body
`start+1..=end`. When collapsed, the body is hidden; only the header shows, with
a fold marker. Nested regions allowed: a collapsed outer region hides inner
regions wholesale (inner collapse state is preserved but inert).

## Backend fold engine (beside `EditCore`)

New component `FoldState`:

```rust
struct FoldRegion { start: u32, end: u32 }      // header = start; body start+1..=end

struct FoldState {
    regions: Vec<FoldRegion>,                    // sorted; may nest; from provider
    collapsed: HashSet<u32>,                     // keyed by header line
}
```

Derived **fold map** (recomputed when regions/collapsed change):

- `is_hidden(line)` — true iff `line` lies in the body of some collapsed region
  (`R.start < line <= R.end` for any collapsed `R`). Header lines never hidden.
- `visible_lines` — `total_lines - hidden_count`.
- `buffer_to_row(line)` — `line - hidden_before(line)`.
- `row_to_buffer(row)` — inverse.
- Implemented over a sorted list of collapsed spans with cumulative-hidden
  prefix sums; binary search for mapping. (v1 may use a straightforward scan;
  optimize if large-file profiling demands.)

### Fold providers (seam)

```rust
trait FoldProvider { fn regions(&self, rope: &Rope) -> Vec<FoldRegion>; }
```

- **`IndentFoldProvider`** (M1, the fallback). A line starts a region when the
  next non-blank line is more indented; the region extends through the last
  consecutive line with indent greater than the header's, excluding trailing
  blanks. Produces nested regions. Self-contained over the rope; no LSP.
- **LSP `foldingRange`** (M2). Request on open + on change (debounced); map
  `FoldingRange{startLine,endLine,kind}` → `FoldRegion`. When a non-empty LSP
  result exists for the buffer, it replaces the indent-derived regions; otherwise
  indent regions stand. The LSP client already issues hover/definition/diagnostic
  requests — this is one more method. `vmux_mock_lsp` gains a `foldingRange`
  response for tests.

### Recompute + collapsed remap

Regions are re-derived on open and after every text change. The collapsed set is
remapped so folds survive editing:

1. On a text edit that inserts/deletes `N` lines at line `P`, shift every
   collapsed header `>= P` by `±N`.
2. After regions are recomputed, intersect `collapsed` with the new region start
   lines; drop entries that no longer begin a region (stale).

This keeps folds stable through ordinary typing; large structural edits may pop a
fold open, which is acceptable for v1.

## Windowing & motions (rope-side, backend)

- `FileViewport { top_row, rows }` — semantics shift from buffer line to visual
  row. `rows_from_viewport` (pixels/`ch`) unchanged.
- `window_bounds` / `emit_window` clamp in visual-row space against
  `visible_lines`, map the row window to visible buffer lines (with overscan in
  row space), and emit each visible line tagged with its `fold` marker.
- **Vertical motions** (`Up`, `Down`, `PageUp`, `PageDown`) step over *visible*
  lines, skipping hidden ones (cursor down from a collapsed header lands on the
  line after the fold). `EditCore::apply` takes a `&FoldMap` view to resolve
  these. Horizontal, word, and line-internal motions are unchanged.
- **Jump-into-hidden** — `GotoLine`, goto-definition, and search targets that
  land on a hidden line auto-reveal the enclosing collapsed region(s) so the
  caret is visible (VSCode behavior).
- `autoscroll` operates in visual rows.

## Fold commands

New `EditCommand` variants, handled at the **plugin level** (no-ops inside
`EditCore::apply`, like `Save` / `GotoDefinition` today):

```
FoldToggle, FoldOpen, FoldClose, FoldToggleRecursive, FoldAll, UnfoldAll
```

The plugin handler finds the innermost region enclosing the caret line, mutates
`FoldState.collapsed`, recomputes the map, re-emits window + cursor, and persists.

- `FoldAll` collapses every region; `UnfoldAll` clears `collapsed`.
- `FoldToggleRecursive` toggles the enclosing region and all its descendants.

Gutter clicks send `FileFoldToggle { line }` (frontend → backend), routed to the
same handler keyed by line.

### Keymaps

- **Vim** (`vim.rs`): add a `z_pending` flag mirroring the existing `g_pending`.
  - `za` → `FoldToggle`, `zo` → `FoldOpen`, `zc` → `FoldClose`,
    `zA` → `FoldToggleRecursive`, `zR` → `UnfoldAll`, `zM` → `FoldAll`.
- **VSCode** (`vscode.rs`):
  - `Ctrl/Cmd+Shift+[` → `FoldClose`, `Ctrl/Cmd+Shift+]` → `FoldOpen`.
  - Fold-all / unfold-all use **non-chord** binds for v1 (the VSCode keymap has no
    Cmd+K chord support): `Cmd+Shift+0` → `FoldAll`, `Cmd+Shift+J` → `UnfoldAll`.
    Real VSCode chords (`Cmd+K Cmd+0` / `Cmd+K Cmd+J`) deferred.

## Persistence

A per-file store, `folds.ron`, in the profile directory:

```
HashMap<PathBuf, Vec<u32>>   // path -> collapsed header lines
```

- **Load** on file open: after the first region computation, collapse the stored
  header lines that match a current region start.
- **Save** (debounced) on any fold mutation. Remove the entry when a file has zero
  collapsed regions — never write empty entries (no config auto-seed).
- **Drift** — if the file changed outside the editor, only header lines that still
  begin a region re-collapse; the rest are dropped. Acceptable for v1.

## Wire protocol (`vmux_core::event`)

All new/changed types carry the same `serde` + `rkyv` derives as their siblings.

- `enum FoldGutter { None, Open, Collapsed }`.
  - `Open` — line is an expanded fold header (▾, hover-revealed; click collapses).
  - `Collapsed` — folded header (▸ always shown + `⋯` placeholder; body hidden).
  - `None` — not a header.
- `FileLine` gains `fold: FoldGutter`.
- `FileViewportPatch` becomes `{ first_row: u32, total_rows: u32, total_lines: u32,
  lines: Vec<FileLine> }`. `total_rows` sizes the spacer; `total_lines` sizes the
  gutter; each `FileLine.line_no` is the buffer line for its number.
- `CursorPos` and `SelSpan` gain `row: u32` (backend-computed visual row) so the
  frontend positions caret and selection with no fold math.
- `FileScrollEvent` field semantics: `top_line` → `top_row` (visual row).
- New `FileFoldToggle { line: u32 }` (frontend → backend), gutter chevron click.

## Frontend (`page.rs`) — render only

- Lay visible line `i` at `(first_row + i) * ch`; spacer height `total_rows * ch`.
- Gutter (the existing `sticky left-0` line-number span): when `fold == Open`,
  show a ▾ chevron on row hover; when `Collapsed`, always show ▸ and render a `⋯`
  placeholder after the line text. Chevron click → `FileFoldToggle { line_no }`.
- Caret and selection use the `row` from the cursor event; no local fold lookup.
- Scroll handler emits `FileScrollEvent { top_row }` (renamed semantics).
- Keystrokes flow to the backend keymap exactly as today.

## Testing

Native `cargo test -p vmux_editor` (plus `vmux_core` for wire types):

- Fold map: `buffer_to_row` / `row_to_buffer`, nested collapse, `visible_lines`.
- `IndentFoldProvider`: regions for indented bodies, brace bodies, trailing blanks.
- Motions: `Down` skips a folded body; `PageDown` counts visible rows;
  jump-into-fold reveals the enclosing region.
- Edit remap: insert/delete shifts collapsed headers; stale headers dropped.
- Persistence: round-trip; drift drops non-matching headers; zero-fold files write
  no entry.
- LSP `foldingRange` parse → regions (via `vmux_mock_lsp`).
- Plugin message-integration (per AGENTS.md): send `FileFoldToggle` / fold
  `EditCommand`s, run schedules, assert the resulting `FileViewportPatch` hides the
  body lines and reports the right `total_rows`.
- When editing `page.rs` gutter markup, re-run the `include_str!` source-scrape
  tests (`style.rs`, `tests/page_source.rs`).

## Milestones

- **M1** — fold engine + visual-row windowing + `IndentFoldProvider` + gutter
  chevrons + keyboard commands + persistence.
- **M2** — LSP `foldingRange` provider feeding the same region set.

## Out of scope (v1)

- Column / horizontal virtualization (separate concern).
- Fold-on-open defaults (e.g. auto-fold imports/comments); files open expanded.
- Real VSCode Cmd+K chords.
- Folding inside the diff/preview views (editor view only).
