# file:// directory browser — yazi-style miller columns + previews

Date: 2026-06-23
Branch: `feat/file-dir-preview`
Status: design

## Motivation

The `file://` viewer (PR #118, `crates/vmux_editor`) renders single text files with
syntax highlighting and directories as a flat icon grid with no preview. The grid
cannot preview anything — notably images. The driving use case is browsing the
screenshots the in-progress screenshot feature writes to `~/.vmux/screenshots/`, but
the result is general: any directory becomes navigable with live previews of images,
code/text, sub-directories, and a metadata fallback.

This work is independent of the screenshot feature (separate worktree
`.worktrees/screenshot-mcp`). It branches off `origin/main` and ships on its own; it
only consumes ordinary image files, with no code dependency on the screenshot work.

## Goals

- Replace the flat icon grid with a **yazi-style miller-column** directory view:
  parent column | current column | preview column.
- **Modern "soft glass" look** (not the old boxy grid): translucent rounded panes,
  generous padding, a rounded accent **selection pill**, subtle hover, file-type glyphs,
  and **small inline image thumbnails** in the list. See Visual design.
- Keyboard navigation (`j/k`, `h/l`, arrows, `Enter`, `Esc`) plus mouse
  (click = select, double-click = activate).
- **Preview pane** renders, for the highlighted entry: image, code/text quick-look,
  sub-directory listing, or a metadata fallback.
- **Activating** an entry (`Enter`/`l`/double-click) navigates the page to that
  `file://` URL in the same stack: descend into a directory (new columns) or open a
  file full-size (full scrollable editor for text, full image for images).
- Images travel host→page as bytes over the existing rkyv bin-event channel; the page
  builds a `Blob` object URL. No changes to the patched CEF crate.

## Non-goals

- No host-side downscaling for the **full-size** image view (browser scales the capped
  original via CSS `object-fit: contain`). Host-side downscaling is used **only** for the
  small ~64px list thumbnails (see Visual design / Thumbnails).
- No file mutation (rename/delete/move/create). Read-only browser.
- No hidden-file toggle UI in v1 (dotfiles hidden; toggle is a later add).
- No custom CEF scheme handler for filesystem bytes (considered and rejected — see
  Alternatives).

## Architecture

Unchanged host/page split. The host plugin (`vmux_editor/src/plugin.rs`, native) reads
the filesystem and pushes rkyv events; the wasm Dioxus page (`vmux_editor/src/page.rs`)
renders. Every `file://` URL is a page-open: navigation is "re-open a `file://` URL in
the same stack", handled by the existing `handle_file_page_open` (which clears the
stack's children and spawns a fresh `FileView`). The webview's `WebviewSource` carries
the `file://` URL; the editor wasm app is loaded because the `files` page manifest maps
the webview to the embedded editor page. The host reads the path from the URL and emits
the appropriate content events; the same app re-renders.

### Three render modes

The page chooses a mode from the events it receives for the current `file://` URL:

1. **Dir mode** — miller columns. Host emits `FileDirEvent` (extended with parent
   listing). New.
2. **Text mode** — full scrollable highlighted view. Host emits `FileMetaEvent` +
   windowed `FileViewportPatch`. Exists, unchanged.
3. **Image mode** — full scaled image. Host emits `FileImageEvent { mime, bytes }`.
   New.

## Interaction (dir mode)

```
┌─────┬───────────┬────────────┐
│ ..  │ src/      │  preview   │   left   = parent dir entries (current highlighted)
│ doc │>main.rs   │ (img/code/ │   middle = current dir entries (selection)
│ src▌│ lib.rs    │  dir/info) │   right  = preview of highlighted middle entry
└─────┴───────────┴────────────┘
```

- `j` / `ArrowDown` — move selection down in current dir (in-page only).
- `k` / `ArrowUp` — move selection up.
- `l` / `ArrowRight` / `Enter` / double-click — **activate** selected entry → emit
  `FileOpenEvent { path }`.
- `h` / `ArrowLeft` / `Esc` — go to parent → emit `FileOpenEvent { path: parent }`.
  No-op at filesystem root (`/`).
- Single mouse click on a middle entry — selects it (updates preview), does not
  activate.

Selection lives in the page (signal). Moving selection does not round-trip to the host
except to fetch the preview (below). Activation round-trips through the host so it
reuses page-open + history + title handling.

### Preview fetch

On selection change, the page sends a **debounced** (~80 ms) `FilePreviewRequest { path }`
over the bin channel. The host replies `FilePreviewEvent { path, kind }`. The page
ignores a reply whose `path` no longer matches the current selection (stale guard). The
`kind` is:

- `Dir(entries)` — selected entry is a directory; show its child listing.
- `Text(lines)` — first N (=200) highlighted lines via the existing `Highlighter`.
- `Image { mime, bytes }` — for `png/jpg/jpeg/gif/webp`; page builds a `Blob` URL.
- `Info { size, modified, kind }` — binary/unknown/other; show metadata.
- `Error(message)` — unreadable / failed.

## Visual design (soft glass)

Styled with Tailwind utilities (project rule: prefer utilities/arbitrary values over
hand-written CSS), reusing the existing theme tokens already used by the editor page
(`bg-term-bg`, `text-term-fg`, `text-muted-foreground`, the `font_family`/`font_size`/
`line_height` from `FileThemeEvent`). Target a modern macOS feel, not the old grid.

- **Panes**: three columns, each a translucent rounded card —
  `rounded-xl bg-white/[0.04] backdrop-blur-md ring-1 ring-white/[0.06]` — separated by
  small gaps (`gap-2`), generous inner padding (`p-2`/`p-3`). The whole view sits on the
  page's `bg-term-bg`.
- **Rows**: comfortable height with `rounded-lg px-3 py-1.5`, left-aligned glyph + name,
  `truncate` names. Hover `hover:bg-white/[0.05]`.
- **Selection pill**: the selected middle-column row gets a filled rounded pill
  (`bg-white/[0.10] ring-1 ring-white/15`) with an accent left marker; it scrolls into
  view on `j/k`. The parent column highlights the current directory's row with a quieter
  variant.
- **Glyphs**: directory and generic-file glyphs reuse the existing `Icon` component (as
  the current grid does). Image rows show a thumbnail (below) instead of the generic
  glyph once loaded; until then, an image glyph placeholder.
- **Preview card** (right column): centered content on a translucent card. Image →
  `object-fit: contain` filling the card with rounded corners; text → small highlighted
  snippet; dir → child listing styled like the middle column; info → name + size +
  modified + kind in a tidy stack.
- Columns are independently scrollable (`overflow-y-auto`), with momentum-style thin
  scrollbars (existing utility classes).

### Thumbnails (lazy, bounded)

Inline list thumbnails reuse the **same** preview mechanism with a `thumb` flag rather
than introducing a parallel protocol:

- The page issues `FilePreviewRequest { path, thumb: true }` for image rows in the
  **current** directory, lazily and debounced (only rows scrolled into view; a tiny
  in-page cache keyed by `path` prevents refetching). Full-size selection previews use
  `thumb: false`.
- When `thumb` is set, the host decodes and downscales the image to a max edge of
  **64px** (`image` crate), re-encodes to PNG, and returns `PreviewKind::Image`. Decode
  + downscale runs **off the main thread** (`bevy::tasks::IoTaskPool`, mirroring the
  localhost responser) so the app never blocks; the result is emitted when ready.
- Failure to decode → the row keeps the image glyph placeholder (no error surfaced for a
  thumbnail).
- Thumbnail Blob URLs are cached per path and revoked when the current directory changes.

This adds the `image` crate as a `vmux_editor` (native) dependency. The 64px cap keeps
each thumbnail tiny over the bin channel; the full-size view path is unchanged (capped
original bytes, browser scales).

## Protocol changes — `crates/vmux_core/src/event.rs`

All types derive the existing set (`Debug, Clone, PartialEq, Serialize, Deserialize,
rkyv::{Archive, Serialize, Deserialize}`). New event-name consts alongside the existing
`FILE_*` ones.

Extend `FileDirEvent`:

```rust
pub struct FileDirEvent {
    pub path: String,
    pub entries: Vec<FileDirEntry>,
    pub parent_path: String,            // new; empty when at root (no parent)
    pub parent_entries: Vec<FileDirEntry>, // new; empty when at root
}
```

New:

```rust
pub const FILE_PREVIEW_REQUEST_EVENT: &str = "file_preview_request"; // page→host
pub const FILE_PREVIEW_EVENT: &str = "file_preview";                 // host→page
pub const FILE_OPEN_EVENT: &str = "file_open";                       // page→host
pub const FILE_IMAGE_EVENT: &str = "file_image";                     // host→page

pub struct FilePreviewRequest { pub path: String, pub thumb: bool } // thumb=true → ~64px list thumbnail

pub enum PreviewKind {
    Dir(Vec<FileDirEntry>),
    Text(Vec<FileLine>),
    Image { mime: String, bytes: Vec<u8> },
    Info { size: u64, modified: String, kind: String },
    Error(String),
}

pub struct FilePreviewEvent { pub path: String, pub kind: PreviewKind }

pub struct FileOpenEvent { pub path: String }

pub struct FileImageEvent { pub mime: String, pub bytes: Vec<u8> }
```

`FileResizeEvent` / `FileScrollEvent` remain text-mode-only and untouched.

## Host behavior — `crates/vmux_editor/src/plugin.rs`

- **Dir load** (`load_file_buffers` path for directories): in addition to `entries`,
  compute `parent_path` = `path.parent()` and `parent_entries` = `list_dir(parent)`
  (empty when no parent). Emit the extended `FileDirEvent` from `send_initial_dir`.
- **Image full mode**: when a `FileView`'s path is an image extension, do not build a
  `FileBuffer`; instead read bytes (subject to the size cap) and emit `FileImageEvent`.
  A new `FileImage` marker/component mirrors `FileDir`/`FileBuffer` so the existing
  ready/meta gating applies. Title = file name.
- **Preview handler**: observe `BinReceive<FilePreviewRequest>`. Classify the path by
  type and extension, build the `PreviewKind`, emit `FilePreviewEvent`. Directory →
  `Dir`. Image ext → `Image` (full bytes under the 25 MB cap when `thumb=false`; when
  `thumb=true`, decode + downscale to a 64px max edge via the `image` crate, off the main
  thread with `IoTaskPool`, re-encode PNG). Text (highlighter succeeds) → `Text` (first
  200 lines). Else → `Info` (size/modified/kind from `fs::metadata`). Failure → `Error`
  (full preview) or silently drop the thumbnail (page keeps the glyph).
- **Open handler**: observe `BinReceive<FileOpenEvent>`. Resolve the `FileView`'s pane
  (walk `ChildOf` to the stack → pane) and write a single `PageOpenRequest` with
  `PageOpenTarget::ActiveStackInPane(pane)` and `url = file://<path>`. This **replaces
  in place** (reuses the existing `handle_file_page_open`, which clears the stack's
  children and spawns the new `FileView`) rather than spawning a new tab — so descending
  and ascending directories does not explode tabs. `vmux_editor` already depends on
  `vmux_core::page_open`, so it writes `PageOpenRequest` directly (no `vmux_layout`
  coupling). Parent ascent is itself a `FileOpenEvent { path: parent }`, so it never
  relies on history. Returning from an opened **file** back to its directory uses the
  layout's existing back/forward navigation (each open is a normal page-open).
- **Image size cap**: 25 MB. Over cap → `Info` fallback (preview) / `FileErrorEvent`
  (full mode) with a "too large to preview" message; never send the bytes.

### Extension → kind classification

Pure function (see helpers). Image set: `png, jpg, jpeg, gif, webp`. mime derived from
the same small table (no new dependency). Everything the highlighter can load → text;
otherwise info.

## Page behavior — `crates/vmux_editor/src/page.rs`

- Add `dir_entries`, `parent_entries`, `selected: usize` signals; `preview` signal
  holding a decoded preview kind; an `image_url` signal for `Blob` object URLs.
- Render mode switch: image event → `<img>` full; dir event → miller columns; else the
  existing text view.
- Miller columns: three flex columns, each scrollable; middle column rows are
  selectable/focusable; selected row scrolls into view on `j/k`.
- Keymap on the columns container per Interaction above; emit `FileOpenEvent` /
  `FilePreviewRequest` via `try_cef_bin_emit_rkyv`.
- Blob lifecycle: when a new `Image` preview or `FileImageEvent` arrives, revoke the
  previous object URL before creating the next (no leak).
- Debounce selection preview requests (~80 ms) so holding `j` over large images does not
  spam the host.
- **Inline thumbnails**: maintain a `HashMap<path, blobUrl>` cache. For image rows in the
  current dir that scroll into view, issue `FilePreviewRequest { path, thumb: true }`
  (skip if cached/in-flight); on the `Image` reply build a Blob URL and store it. Render
  the thumbnail in place of the generic glyph. Revoke all cached thumbnail URLs and clear
  the cache when the dir changes (new `FileDirEvent`). Soft-glass styling per Visual
  design.

## Pure / testable helpers — `crates/vmux_editor/src/page_model.rs`

Non-wasm, unit-tested:

- `classify(path, is_dir) -> ContentClass` (Dir | Image{mime} | Text | Other).
- `clamp_selection(idx, len) -> usize`.
- `parent_highlight_index(parent_entries, current_path) -> Option<usize>` (which parent
  entry is the current dir, for left-column highlight).

Host-side unit tests (native, `cargo test -p vmux_editor`):

- parent computation for a nested dir vs. root (`/` → empty parent).
- preview-kind selection by extension/type, including image cap → `Info`.
- thumbnail downscale: a known test image downscales to ≤64px on its longest edge and
  re-encodes to valid PNG.
- `FileOpenEvent` resolves to a `PageOpenRequest` for the originating pane's stack.

## Edge cases

- Root `/`: `parent_path`/`parent_entries` empty; `h`/`Esc` is a no-op.
- Empty directory: middle column empty; preview pane empty/neutral.
- Image over 25 MB: `Info`/`error` fallback, bytes never sent.
- Unreadable entry (perms): `PreviewKind::Error`.
- Dotfiles: filtered out in `list_dir` for both current and parent listings (v1).
- Stale preview reply: dropped via `path` match guard.
- Symlinks: followed via `fs::metadata` (existing `is_dir` semantics); broken symlink →
  `Error`.

## Testing & verification

- Native: `cargo test -p vmux_editor` (and `-p vmux_core` for event types).
- `cargo check -p vmux_editor` during the edit loop; `cargo fmt` + `cargo clippy` before
  commit (CI gate).
- Page UI is wasm: `vmux_editor/src` is already in
  `vmux_server/build.rs::track_manifest_rel_paths` (line 16), so page edits rebuild.
  Per project rule, **the user runtime-tests** the visual result; the agent does not
  launch `make dev`.
- Verify observable output (the events the page receives), not internal host state.

## Files touched

- `crates/vmux_core/src/event.rs` — new consts, `PreviewKind` enum, new event structs,
  extend `FileDirEvent`.
- `crates/vmux_editor/src/plugin.rs` — parent listing, preview handler, open handler,
  image full-mode load, classification, registrations.
- `crates/vmux_editor/src/page.rs` — miller-column UI, keymap, preview pane, image
  render + Blob lifecycle, mode switch.
- `crates/vmux_editor/src/page_model.rs` — classification/selection/parent-index helpers
  + tests.
- `crates/vmux_editor/Cargo.toml` — add `image` (native, non-wasm) for 64px thumbnail
  downscale.
- Possibly `crates/vmux_editor/src/highlight.rs` — expose a "first N lines" load if not
  already convenient (reuse `load_file`).

## Alternatives considered

- **Custom CEF scheme handler** to stream filesystem bytes (`<img src=cef://localhost/fs/…>`):
  browser-native streaming/zoom and no IPC byte bloat, but edits the fragile, excluded
  patched `bevy_cef_core` crate (larger blast radius, extra package checks). Rejected in
  favor of the rkyv→Blob path, which matches how all other file content already flows.
- **base64 data URLs**: simpler page code but ~33% larger payloads and encode/decode
  cost; the bin channel is already binary (rkyv), so raw bytes + `Blob` is strictly
  better.
- **Two-pane (list + preview)** and **grid + preview** layouts: rejected in favor of
  full miller columns for keyboard-first parity with yazi.
