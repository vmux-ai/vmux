# Text Editor — `file://` Read-Only IDE Viewer (v1)

Date: 2026-06-21
Status: Approved design, pre-plan
Scheme revised 2026-06-21: use the real `file://` scheme (override the built-in CEF
scheme with our own handler factory) instead of a custom `files://` scheme — see "Why
overriding `file://` costs more" and §1. Internal host id stays `"files"`.

## Goal

First step of the text editor feature: render a file's contents in a vim/VSCode-style
page — path header, line-number gutter, monospace content, syntax highlighting —
addressed by the real `file://` scheme (overridden with our own CEF handler factory) and
streamed with terminal-style server-owned viewport virtualization.

Read-only. Editing, file tree, watch/live-reload, and LSP are explicitly out of scope.

## Decisions (locked)

| Decision | Choice |
| --- | --- |
| URL scheme | Override the built-in `file://` scheme with our own CEF handler factory (e.g. `file:///Users/me/src/main.rs`); internal host id `"files"` |
| v1 scope | Read-only viewer + line numbers + path header + **syntax highlighting** |
| Virtualization | Server-owned viewport (terminal-style); backend streams only the visible window |
| Highlighter | `syntect` (native/backend side only) |

## Why overriding `file://` costs more (consequences)

The existing built-in pages are served at `vmux://<host>/` and selected by host. We
override the built-in `file://` scheme with our own CEF handler factory instead, which has
three direct consequences:

1. **Page routing.** The shared WASM app selects its page from
   `window.location.host()` (`vmux_server/src/lib.rs:66`). `file:///path` has **no
   host**. Routing must switch to `location.protocol === "file:"` (mapping to the internal
   host id `"files"`), and the file path is read from `location.pathname`.
2. **No cross-scheme assets.** A `file://` document gets Chromium's locked-down file
   origin — it cannot pull subresources cross-scheme from `vmux://` (the built-in `file`
   scheme's CORS/secure flags are baked into Chromium and are **not** ours to change, even
   with the fork). So `index.html` and **all** of `./wasm/*`, `./assets/*` must be served
   **same-scheme** by the `file` handler itself (root-absolute refs), never touching
   `vmux://`.
3. **File-origin fetch.** `fetch()` / `WebAssembly.instantiateStreaming` of a `file://`
   subresource needs the `--allow-file-access-from-files` switch plus a correct
   `Content-Type` (`application/wasm`) from the handler. **This is the primary spike —
   validate it before building anything else.**

## Architecture

New crate **`vmux_editor`**, mirroring `vmux_terminal`:
- Backend: file reader + syntect highlighter + viewport driver (native Bevy systems).
- Frontend: `vmux_editor::page::Page` (Dioxus/WASM), compiled into the shared
  `vmux_server` app.

No separate host process (unlike the terminal's `vmux_service` PTY host) — a file read
is in-process.

### 1. Scheme override + handler (`patches/bevy_cef_core-0.5.2`)

We own the binding layer, so we override the built-in `file` scheme rather than adding a
new custom one:

- `browser_process/app.rs` (`on_register_custom_schemes` / scheme registrar): register a
  `CefSchemeHandlerFactory` for the built-in `"file"` scheme. (You can replace the handler
  for a built-in scheme, but you **cannot** re-flag its CORS/secure policy via
  `AddCustomScheme` — only the bytes are ours.)
- New `SchemeHandlerFactory` for `file://` in `browser_process/localhost.rs`: serve the
  shared `index.html` for a document request, and serve `/wasm/*` and `/assets/*` from the
  embedded assets **same-scheme** (root-absolute refs in `index.html`). The handler does
  **not** read the target source file — file content arrives over the rkyv bin bridge (§4).
- **Bridge gate:** extend the render-process trust check (`has_embedded_scheme` in the
  patched `render_process/cef_api_handler.rs`) to also trust `file://`, so `window.cef`
  (binEmit/binListen) is injected into the file document.
- **Launch switch:** add `--allow-file-access-from-files` so the file document may
  `fetch()` its same-scheme wasm/js/css.

**Global-hijack caveat:** a factory for `file` intercepts **every** `file://` request in
the browser. Acceptable only because vmux does not otherwise load raw local files; revisit
if that changes. (A custom `files://` scheme would avoid this collision — keep it as the
fallback if the spike below fails.)

The `file://` origin policy + wasm-load path is the primary technical risk; spike
consequence (3) above before building the rest.

### 2. App page routing (`vmux_server/src/lib.rs`)

- `current_host()`: if `location.protocol() == "file:"`, return `"files"` (the internal
  host id stays `"files"`).
- Add `render_files: "files" => vmux_editor::page::Page` to the page-render macro.

### 3. Native page-open handler (`vmux_editor`, in `PageOpenSet::HandleKnownPages`)

- Match `task.url.starts_with("file:")` → clear stack children, attach a CEF webview to
  the `file://` URL (reuse the existing `attach_cef_page` path so the SPA loads), and
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

- `snapshot.rs:140` (`build_stack`): add `file:` → kind `"files"`.
- `page.rs` `StackIcon`: a document/file icon for `file:` URLs.
- `page.rs` `format_address`: show the file path for `file:` URLs (like the `vmux://`
  branch returns the raw URL).
- `command_bar/page.rs:130`: treat a `file:` input as a navigable URL, not a search query.

### Opening a file (v1)

By URL only: address bar, the `open` command, or MCP `open` with
`url=file:///abs/path`. A dedicated "open file" command / picker is later scope.

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
- `file://` document request → served the shared `index.html` with root-absolute asset
  refs (no `vmux://` references).
- `file://` `/wasm/*` and `/assets/*` requests → mapped to embedded asset bytes with the
  correct `Content-Type` (`application/wasm` for wasm).
- Spike (manual, pre-build): a factory-served `file://` wasm subresource instantiates
  under `--allow-file-access-from-files`.

Frontend (pure helpers, like `render_model.rs` tests):
- Gutter width from `total_lines`.
- `top_line` clamping for wheel/key deltas.
- `current_host()` returns `"files"` for the `files:` protocol (factor a pure helper).

## Risks

1. **`file://` origin + wasm load** — Chromium's built-in file origin blocks cross-scheme
   assets, so everything is served same-scheme by our handler behind
   `--allow-file-access-from-files`. The unproven part is wasm instantiate on a
   factory-served `file://` subresource. Mitigation: spike consequence (3)/§1 before
   building; a custom `files://` scheme remains the fallback if the spike fails.
2. **Patched CEF crate** edit → run the appropriate package checks (per AGENTS.md).
3. **Whole-file highlight memory** on very large files → size cap for v1; lazy
   highlighting later.

## Out of scope (later steps)

Editing + save, dirty state + undo, file tree / picker, watch / live-reload, LSP /
diagnostics, incremental (checkpointed) highlighting, minimap, search-in-file.
