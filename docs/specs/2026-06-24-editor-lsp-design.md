# editor LSP — milestone 1: client core + diagnostics

Date: 2026-06-24
Branch: `editor-lsp`
Status: design

## Motivation

The `files://` editor (`crates/vmux_editor`) follows the VS Code stack: syntect
highlighting (via `two-face`) for color, LSP for intelligence. Highlighting shipped;
LSP is the next milestone. The north star is an editor that can replace VS Code /
Cursor, which means broad language coverage and the full IDE feature set
(diagnostics, hover, go-to-definition, completion, rename, formatting, …).

That north star is **multiple subsystems**, not one spec. This document covers only
the **first sub-project**: an LSP **client core** plus the first visible feature,
**diagnostics**. It is deliberately read-only — it does not require an editing
surface (cursor/edits/save), which the editor does not yet have. Diagnostics are
push-based (`textDocument/publishDiagnostics`), so they prove the entire pipe
end-to-end on the current read-only viewer.

## Scope decomposition (north star, for context)

Build order for the larger effort. Each gets its own spec → plan → implementation:

1. **LSP client core + diagnostics** ← this spec.
2. Hover / go-to-definition / document symbols (request-based, still read-only).
3. Editing surface (cursor, edits, save, document version counter).
4. Completion / signature help / rename / formatting (need the editing surface).

## Goals

- A reusable **LSP client core** in the `vmux_editor` backend: spawn a language
  server, speak JSON-RPC over stdio, manage server lifecycle, correlate
  request/response, and dispatch server notifications.
- A **registry** mapping ~15-25 common languages to their conventional server
  command, spawned on-demand **only if the binary is found on PATH**. The Helix /
  Neovim provisioning model: the user installs servers; vmux discovers and drives
  them. User-extensible via `settings.ron`.
- **Diagnostics** rendered in the editor: a severity dot in the gutter, a wavy
  underline (squiggle) on the diagnostic range, and the message in a **soft-glass
  hover card**.
- Self-contained in the editor backend: **no tokio**, no new daemon, matching the
  existing `vmux_git` spawn→thread→outbox→drain→emit pattern.

## Non-goals

- No editing surface (cursor, text mutation, save-to-disk, version tracking).
  Documents are opened read-only; the only "change" is an external edit picked up by
  the existing file watcher.
- No interactive request features (hover, go-to-def, completion, symbols) — milestone
  2+.
- No server auto-install / marketplace / bundling. PATH detection only.
- No multi-root / workspace-folder management beyond a single resolved root per file.
- No semantic-tokens highlighting (syntect remains the highlighter; tree-sitter stays
  rejected).
- No diagnostics for files above the existing 5 MiB load cap.

## Architecture

Unchanged host/page split. The **host** (`vmux_editor`, native, `cfg(not(wasm32))`)
owns all LSP machinery and the filesystem; the **page** (`vmux_editor/src/page.rs`,
wasm/Dioxus) only renders diagnostics it receives over the existing rkyv bin-event
channel. No tokio is introduced: each server runs on its own OS threads (reader /
writer / stderr), and results reach Bevy through a shared outbox drained once per
frame — exactly the `vmux_git` model (`drain_git_outbox`).

### Module layout

New tree in `crates/vmux_editor/src`, native-only, filename-based module pattern
(no `mod.rs`):

- `lsp.rs` — `LspPlugin`, submodule declarations, shared host-side types.
- `lsp/registry.rs` — embedded language→server table, PATH detection, workspace-root
  resolution, `settings.ron` override.
- `lsp/client.rs` — `ServerClient`: spawn child (piped stdio), writer thread, reader
  thread (Content-Length framing + JSON-RPC parse), stderr drain, request-id
  correlation, diagnostics outbox.
- `lsp/manager.rs` — `LspManager` Bevy `Resource`, observers/systems, document
  lifecycle (didOpen/didChange/didClose), UTF-16→char column conversion, emit.

`LspPlugin` is added to the existing editor plugin set in `vmux_editor/src/plugin.rs`
(or `lib.rs`), behind the same `cfg(not(wasm32))` gate.

### Components

**Registry (`lsp/registry.rs`)**

- Embedded `const` table: `&[(language_id, ServerSpec)]` where
  `ServerSpec { command: String, args: Vec<String>, root_markers: Vec<String> }`.
  Initial set (~15-25): rust-analyzer, pyright (or pylsp), typescript-language-server,
  gopls, clangd, lua-language-server, jdtls, solargraph, zls, bash-language-server,
  vscode-json-language-server, yaml-language-server, taplo, marksman, etc. Final list
  decided in the plan.
- Keyed off the language already detected for highlighting (`FileBuffer.language` from
  syntect). One mapping, no second language-detection path.
- **PATH detection**: resolve `command` against `PATH` (which-style lookup). If absent,
  no server is spawned for that language — logged once at info, never an error.
- **Config override**: `settings.ron` `editor.lsp.servers` may add or override entries
  by `language_id`. Absent key → embedded fallback (defaults live in code, **never
  auto-seeded** into `settings.ron`).
- **Workspace root**: walk up from the file's directory for the spec's `root_markers`
  (e.g. `Cargo.toml`, `.git`, `package.json`, `go.mod`, `pyproject.toml`); fall back to
  the file's parent directory.

**ServerClient (`lsp/client.rs`)**

- `spawn(spec, root) -> io::Result<ServerClient>`: `std::process::Command` with
  `Stdio::piped()` for stdin/stdout/stderr.
- **Writer**: outgoing JSON messages sent through a channel to a writer thread that
  frames each as `Content-Length: N\r\n\r\n<body>` and writes to the child's stdin.
- **Reader thread**: parses Content-Length frames from stdout, deserializes JSON-RPC.
  Messages with an `id` (responses) are routed to the matching pending request via
  `HashMap<i64, mpsc::Sender<serde_json::Value>>`. Notifications
  (`textDocument/publishDiagnostics`) are pushed to the outbox.
- **stderr thread**: drains stderr to the log (servers are chatty there).
- **Outbox**: `Arc<Mutex<Vec<(PathBuf, Vec<lsp_types::Diagnostic>)>>>`, shared with the
  manager and drained by a Bevy system. (Mirrors `GitOutbox`.)
- **Request id correlation**: `AtomicI64` counter; pending map as above. Milestone 1
  only issues one request (`initialize`); the mechanism is general for later milestones.
- **Handshake**: on spawn, send `initialize` (with `rootUri` + a minimal client
  capabilities advertising `publishDiagnostics`), await the response (bounded 10s),
  then send the `initialized` notification.

**LspManager (`lsp/manager.rs`)**

- Bevy `Resource` (all handles are `Send`). Owns:
  - `servers: HashMap<ServerKey, ServerClient>` where `ServerKey = (root: PathBuf,
    server_id: String)` — one server per workspace-root + language, reused across files.
  - `open_docs: HashMap<PathBuf, OpenDoc { server_key, version: i32, language_id }>`.
  - `failed: HashSet<ServerKey>` — keys whose spawn/init failed, to avoid retry storms.
- Driven by Bevy observers/systems wired in `LspPlugin`:
  - **On text file loaded**: a system reacts to a freshly-loaded text `FileView`
    (the same point the highlighted buffer becomes available). Resolve language → spec
    → root; get-or-spawn the server; send `textDocument/didOpen` (full text, languageId,
    version 0); record the open doc. Skip if no spec, binary missing, key failed, or
    file > 5 MiB.
  - **On external reload**: the existing watcher path
    (`reload_changed_files`) additionally sends `textDocument/didChange` (full-text sync,
    `version += 1`).
  - **On close / navigation away**: send `textDocument/didClose`; drop the open-doc
    entry. Servers stay resident (cache) until `AppExit`.
  - **Drain system** (`Update`): drain every server's outbox; for each
    `(path, Vec<Diagnostic>)`, find the webview `Entity`(ies) currently showing that
    path, convert each diagnostic to `FileDiagnostic` (UTF-16→char columns using the
    buffer's line text; severity map), and emit `FileDiagnosticsEvent` via
    `BinHostEmitEvent`, gated on `browsers.host_emit_ready`.

### Data model (host↔page contract)

Added to `crates/vmux_core/src/event.rs` (reuse — no new crate). All derive
`serde` + `rkyv::{Archive, Serialize, Deserialize}`, matching the existing
`StyledSpan`/`FileLine` types.

```rust
pub enum DiagSeverity { Error, Warning, Info, Hint }

pub struct FileDiagnostic {
    pub line: u32,        // 0-based absolute line
    pub start_col: u32,   // char index within the line (NOT UTF-16, NOT byte)
    pub end_col: u32,     // char index, exclusive
    pub severity: DiagSeverity,
    pub message: String,
    pub source: Option<String>, // e.g. "rustc", "eslint"
}

pub struct FileDiagnosticsEvent {
    pub path: String,
    pub diagnostics: Vec<FileDiagnostic>,
}

pub const FILE_DIAGNOSTICS_EVENT: &str = "file_diagnostics";
```

**Column model**: LSP positions are UTF-16 code-unit offsets. Conversion to **char
indices** happens host-side in the manager (it holds the buffer line text), so the page
stays char-indexed and consistent with how it already renders `StyledSpan` text.
Out-of-range columns are clamped to the line's char length; never index-panic.

### Frontend (`page.rs` + `page_model.rs`, wasm)

- Subscribe to `FILE_DIAGNOSTICS_EVENT`; store the latest `Vec<FileDiagnostic>` in a
  Dioxus `Signal`. A new path's event replaces the previous list (full set each push).
- **Gutter dot**: a small severity-colored dot in the line-number gutter for lines that
  have any diagnostic (error > warning > info > hint precedence per line).
- **Squiggle**: a wavy underline on `[start_col, end_col)`. Monospace makes this a clean
  overlay — `left: calc(start_col * 1ch)`, `width: calc((end_col - start_col) * 1ch)`,
  wavy underline in the severity color, positioned over the rendered line.
- **Soft-glass hover card**: hovering a squiggle (or its gutter dot) shows the message in
  a translucent, rounded, blurred card (matches the soft-glass UI direction), with the
  `source` as a subtle label. Not a native `title` tooltip.
- **Windowing**: the viewport is windowed (only visible lines are sent for rendering).
  Diagnostics are keyed by absolute `line`; the page filters to those within the visible
  window (it already tracks `first_line`). The full diagnostic list is small, so it is
  kept entirely on the page and filtered per render.
- **Pure helpers in `page_model.rs`** (natively unit-testable, like the existing
  `classify`/`gutter_width`/`span_style`): the visible-window filter and the col→`ch`
  overlay geometry.

## Data flow (end to end)

1. User opens a `files://` text file. The host loads `FileBuffer` (existing path).
2. The manager's load system resolves language→spec→root, get-or-spawns the server
   (initialize handshake on first spawn), and sends `didOpen` (full text, version 0).
3. The server analyzes and pushes `publishDiagnostics` → reader thread → outbox.
4. The Bevy drain system pulls the outbox, converts UTF-16→char columns, maps severity,
   and emits `FileDiagnosticsEvent` to the file's webview `Entity`.
5. The page's listener stores the list and renders gutter dots + squiggles for visible
   lines; hover shows the soft-glass card.
6. External edit → existing watcher reload → manager `didChange` (version++) → fresh
   `publishDiagnostics` → re-emit (the page replaces its list).
7. Navigation away / close → `didClose`. Servers stay resident; `AppExit` shuts them all
   down.

## Error handling

- **Binary not on PATH** → skip, log once at info. No diagnostics, no crash.
- **Spawn fails** → insert `ServerKey` into `failed`; do not retry on every subsequent
  file open. Log the error.
- **Server crash** (reader thread sees EOF / broken pipe) → drop the `ServerClient` and
  its open docs for that key; a later file open re-spawns. Log.
- **Malformed frame / undeserializable message** → skip that frame, continue reading;
  never panic.
- **initialize timeout** (bounded 10s on the response channel) → kill the child, mark
  the key `failed`. Log.
- **UTF-16→char conversion** → clamp out-of-range columns to line char length.
- **Files > 5 MiB** (existing load cap) → no `didOpen`, no LSP.
- **AppExit** → send `shutdown` then `exit` to each server, join threads with a short
  timeout, kill if unresponsive.

## Testing

Unit (native, `cargo test -p vmux_editor` / `-p vmux_core`):

- **Framing**: Content-Length encode/parse round-trip, including a body split across
  multiple reads (partial-read handling).
- **Registry**: language→spec mapping; workspace-root resolution against temp dirs
  seeded with markers; PATH detection (present vs absent); `settings.ron` override merge.
- **Column conversion**: UTF-16→char on multibyte lines (emoji, CJK), with clamping of
  out-of-range columns.
- **Diagnostic mapping**: `lsp_types::Diagnostic` → `FileDiagnostic` (severity map,
  range → char columns, `source` passthrough).
- **page_model**: visible-window filter + col→`ch` overlay geometry.

Integration:

- **Mock LSP server**: a tiny in-repo test binary that speaks LSP framing. The test
  spawns it through `ServerClient`, asserts the `initialize`/`initialized` handshake,
  feeds a `publishDiagnostics`, and asserts the **emitted `FileDiagnosticsEvent`
  payload** (the observable output the page receives — not internal manager state).

Manual / runtime (user-verified, the project norm): open a Rust file with a real error
under rust-analyzer; confirm the gutter dot, squiggle on the right range, and soft-glass
hover card; edit externally and confirm diagnostics refresh.

## Dependencies

- **`lsp-types`** (new external dep, types-only, official). Hand-rolling all LSP structs
  is error-prone and later milestones need many more types; this pays off. Pin a current
  version in the plan.
- **`serde_json`** — verify it's already in the tree (very likely); add if not.
- No tokio, no tower, no new workspace crate. Shared rkyv types go in `vmux_core`.

## Open items for the plan

- Final language/server table and each entry's `root_markers`.
- Exact wake/`host_emit_ready` gating and which existing system the "text file loaded"
  observer hooks into.
- `settings.ron` `editor.lsp` schema shape.
- Whether the diagnostics outbox carries raw `lsp_types::Diagnostic` (convert in the
  Bevy drain, where line text is available) vs converting in the reader thread — current
  plan: convert in the drain system, since the buffer is on the Bevy side.
