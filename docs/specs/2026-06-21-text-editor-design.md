# Text Editor — `files://` Read-Only IDE Viewer (v1)

Date: 2026-06-21
Status: Approved design, pre-plan

## Goal

First step of the text editor feature: render a file's contents in a vim/VSCode-style
page — path header, line-number gutter, monospace content, syntax highlighting —
addressed by a genuine `files://` custom scheme and streamed with terminal-style
server-owned viewport virtualization.

Read-only. Editing, file tree, watch/live-reload, and LSP are explicitly out of scope.

## Decisions (locked)

| Decision | Choice |
| --- | --- |
| URL scheme | Real top-level `files://` custom CEF scheme (e.g. `files:///Users/me/src/main.rs`) |
| v1 scope | Read-only viewer + line numbers + path header + **syntax highlighting** |
| Virtualization | Server-owned viewport (terminal-style); backend streams only the visible window |
| Highlighter | `syntect` (native/backend side only) |

## Why the real scheme costs more (consequences)

The existing built-in pages are served at `vmux://<host>/` and selected by host. A real
`files://` scheme bypasses that, with two direct consequences:

1. **Page routing.** The shared WASM app selects its page from
   `window.location.host()` (`vmux_server/src/lib.rs:66`). `files:///path` has **no
   host**. Routing must switch to `location.protocol === "files:"`, and the file path is
   read from `location.pathname`.
2. **Relative assets.** `index.html` references assets relatively (`./wasm/...`,
   `./assets/...`). Under a `files:///Users/me/x.rs` base they resolve to
   `files:///Users/me/wasm/...`. Fix: the `files://` scheme handler injects
   `<base href="vmux://files/">` so assets load via the existing (working) embedded
   scheme, while the document stays on `files://` for path + routing.

## Architecture

New crate **`vmux_editor`**, mirroring `vmux_terminal`:
- Backend: file reader + syntect highlighter + viewport driver (native Bevy systems).
- Frontend: `vmux_editor::page::Page` (Dioxus/WASM), compiled into the shared
  `vmux_server` app.

No separate host process (unlike the terminal's `vmux_service` PTY host) — a file read
is in-process.

### 1. Scheme registration + handler (`patches/bevy_cef_core-0.5.2`)

- `browser_process/app.rs:92` (`on_register_custom_schemes`): register a second custom
  scheme `files` alongside the existing embedded scheme. Flags: standard + secure +
  CORS-enabled, so a `files://` document may pull assets cross-scheme from `vmux://`.
- New `SchemeHandlerFactory` for `files://` in `browser_process/localhost.rs`: serve the
  shared `index.html` for the document request, with `<base href="vmux://files/">`
  injected into `<head>`. The handler does **not** read the target file — file content
  arrives over the rkyv bin bridge (§4).
- **Fallback** if cross-scheme asset loading fails CORS/secure checks: serve `/wasm/*`
  and `/assets/*` from embedded assets inside the `files` handler itself, and serve
  `index.html` (rewritten to root-absolute asset refs) for any other path. This keeps
  everything on `files://` at the cost of asset/document branching in the handler.

This is the primary technical risk; validate the `<base>` approach first, fall back if needed.

### 2. App page routing (`vmux_server/src/lib.rs`)

- `current_host()`: if `location.protocol() == "files:"`, return `"files"`.
- Add `render_files: "files" => vmux_editor::page::Page` to the page-render macro.

### 3. Native page-open handler (`vmux_editor`, in `PageOpenSet::HandleKnownPages`)

- Match `task.url.starts_with("files:")` → clear stack children, attach a CEF webview to
  the `files://` URL (reuse the existing `attach_cef_page` path so the SPA loads), and
  spawn a backend `FileView { path }` entity bound to that webview. Insert
  `PageOpenHandled`.
- Parse the path from the URL (percent-decoded). Guards (→ `FILE_ERROR_EVENT`):
  not found, not a regular file, exceeds size cap, non-UTF-8/binary.
- Set `PageMetadata { url, title: <file name or path>, .. }` for the header/tab.

### 4. Streaming protocol (rkyv bin events; mirrors terminal)

Defined alongside the terminal events (new constants/structs; `u32` line indices because
files exceed `u16`, unlike terminal rows).

Backend → frontend:
- `FILE_META_EVENT` → `FileMetaEvent { path: String, language: String, total_lines: u32 }`
  (drives the header + scrollbar; sent once on open and on reload).
- `FILE_VIEWPORT_EVENT` → `FileViewportPatch { first_line: u32, total_lines: u32, lines: Vec<FileLine> }`
  — the visible window only.
  - `FileLine { line_no: u32, spans: Vec<StyledSpan> }`
  - `StyledSpan { text: String, fg: [u8; 3], bold: bool, italic: bool }`
- `FILE_ERROR_EVENT` → `FileErrorEvent { message: String }`.

Frontend → backend:
- `FILE_RESIZE_EVENT` → `FileResizeEvent { char_height: f32, viewport_height: f32 }`
  (backend derives `rows = floor(viewport_height / char_height)`).
- `FILE_SCROLL_EVENT` → `FileScrollEvent { top_line: u32 }` (absolute; frontend owns the
  synthetic scrollbar position).

Note: v1 sends the full visible window on each scroll/resize (window is bounded by
`rows`, so transfer is constant regardless of file size — this is the virtualization).
Per-row delta patching (as the terminal does for streaming PTY output) is unnecessary for
static-file scroll and can be added later if profiling shows a need.

### 5. Highlighting (`syntect`, backend)

- On open, syntect highlights the **whole file once** into a cached `Vec<FileLine>` stored
  with the `FileView` entity. Windows are then served O(rows) from cache.
- Language detected by file extension; fall back to plaintext.
- Map syntect token colors onto the existing theme (`--ansi-*` / theme CSS vars) so the
  viewer respects the active theme.
- Tradeoff: cached spans use memory ∝ file size. Acceptable for v1; note for later —
  parse-state checkpoints to highlight lazily per-window.

### 6. Frontend page (`vmux_editor::page::Page`)

Mirrors `vmux_terminal::page::Page`:
- **Path header / breadcrumb** from `FileMetaEvent.path`.
- **Line-number gutter**: width = digit count of `total_lines`, right-aligned; absolute
  number = `first_line + row_index + 1`.
- **Content**: monospace, per-row Dioxus signals (cheap re-render), colored spans from
  `StyledSpan`.
- **No native scroll container** — renders exactly the server-provided window (like the
  terminal). A synthetic scrollbar reflects `top_line / total_lines`; wheel + arrow/page
  keys emit `FILE_SCROLL_EVENT { top_line }` (clamped to `[0, total_lines - rows]`).
- Reuses the terminal's cell-measurement span + `ResizeObserver`, emitting
  `FILE_RESIZE_EVENT`.
- Read-only: no input/cursor/selection editing.

### 7. Layout glue (`vmux_layout`)

- `snapshot.rs:140` (`build_stack`): add `files:` → kind `"files"`.
- `page.rs` `StackIcon`: a document/file icon for `files:` URLs.
- `page.rs` `format_address`: show the file path for `files:` URLs (like the `vmux://`
  branch returns the raw URL).
- `command_bar/page.rs:130`: treat a `files:` input as a navigable URL, not a search query.

### Opening a file (v1)

By URL only: address bar, the `open` command, or MCP `open` with
`url=files:///abs/path`. A dedicated "open file" command / picker is later scope.

## Testing (TDD)

Native (`cargo test -p vmux_editor`, `-p vmux_layout`):
- Page-open handler claims a `files:` task and parses the percent-decoded path.
- Reader produces expected `StyledSpan`s for a known snippet in a known language
  (assert a couple of token colors), and plaintext fallback for unknown extensions.
- Window slice: given `(top_line, rows, total_lines)`, returns the correct line range;
  clamps at top and bottom; empty file; single line.
- Guards: missing / directory / oversize / binary → `FileErrorEvent`.
- `vmux_layout`: `build_stack` kind for `files:`; `format_address` for `files:`.

Scheme (`patches/bevy_cef_core-0.5.2`):
- `files://` document request → served `index.html` contains `<base href="vmux://files/">`.
- Fallback branch: `/wasm/*` and `/assets/*` map to embedded asset paths; other paths →
  index.html.

Frontend (pure helpers, like `render_model.rs` tests):
- Gutter width from `total_lines`.
- `top_line` clamping for wheel/key deltas.
- `current_host()` returns `"files"` for the `files:` protocol (factor a pure helper).

## Risks

1. **Cross-scheme asset loading** (document on `files://`, assets on `vmux://`) — the main
   uncertainty. Mitigation: correct scheme flags; fallback handler in §1.
2. **Patched CEF crate** edit → run the appropriate package checks (per AGENTS.md).
3. **Whole-file highlight memory** on very large files → size cap for v1; lazy
   highlighting later.

## Out of scope (later steps)

Editing + save, dirty state + undo, file tree / picker, watch / live-reload, LSP /
diagnostics, incremental (checkpointed) highlighting, minimap, search-in-file.
