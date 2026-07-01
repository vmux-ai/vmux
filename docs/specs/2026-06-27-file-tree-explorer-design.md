# File Tree Explorer — Design

**Date:** 2026-06-27
**Status:** Proposed
**Crate(s):** `vmux_editor`, `vmux_core`, `vmux_ui`, `vmux_setting`

## Goal

Add a VS Code-style Explorer panel to the left of the `files://` editor. Toggleable
with `Cmd+B` / `Ctrl+B`. Three collapsible sections:

1. **OPEN EDITORS** — files opened this editor session (session list, not real tabs).
2. **`<ROOT>` folder tree** — lazy-expanding project tree rooted at the git root.
3. **OUTLINE** — document symbols (LSP `documentSymbol`, markdown-heading fallback).

No activity-bar rail. The only affordances are `Cmd+B` and a toggle button in the
editor header.

## Locked Decisions

| Decision | Choice |
|----------|--------|
| Scope | All three sections (tree + open editors + outline) |
| Architecture | Inside the editor page (`vmux_editor`), not a layout pane |
| Open Editors semantics | Session opened-files list (no real multi-buffer tabs) |
| Activity bar | None — panel + `Cmd+B` toggle + header button only |
| State ownership | Backend (Bevy plugin) owns all state; page is dumb |

## Architecture Principle: Dumb Frontend

Per project convention, the WASM/Dioxus page stays dumb: it **renders pushed
view-models and emits intents**. All geometry, logic, and state live in the native
Bevy plugin. Concretely:

- The backend owns the tree model, expansion set, open-editors list, outline symbols,
  and panel chrome (visible + width). It computes **derived, render-ready view-models**
  (flattened rows with depth/flags) and pushes them over the rkyv bridge.
- The page holds only transient render state (hover, in-progress splitter drag) and
  emits intents (`toggle`, `expand`, `open`, `close`, `goto`, `set-width`).

This keeps the existing `EditCore`/plugin as the single source of truth and matches
how `FileViewport`, diagnostics, and completions already flow.

## Layout

`#file-container` (`page.rs`) is reworked into a horizontal flex:

```
┌─────────────────────────┬──────────────────────────────────┐
│  Explorer panel         │  editor main                     │
│  (≈240px, resizable)    │  (flex-1)                        │
│                         │                                  │
│  EXPLORER               │  [header: ⟨toggle⟩ path · dirty] │
│  ▸ OPEN EDITORS         │  GitBar                          │
│  ▾ VMUX                 │  <Image | Dir | Text view>       │
│  ▸ OUTLINE              │                                  │
└─────────────────────────┴──────────────────────────────────┘
```

- Panel hidden when chrome `visible=false`; main expands to full width.
- A small toggle button is added to the **left of the path** in the existing header
  (`page.rs` `flex h-9` bar) so the panel can be re-shown when hidden.
- Resizable splitter between panel and editor. Width persisted (see Persistence).
- Soft-glass styling consistent with the existing `PANE_CLASS` idiom (translucent
  fill, cyan inset ring, blur). Sections use `vmux_ui` `Collapsible*`.
- The existing Miller-column **Dir mode stays** as the editor main-area view when the
  URL targets a directory. The tree is additive, not a replacement.

## Backend State Model (`vmux_editor` native)

A per-`FileView` component holds Explorer state:

```rust
#[derive(Component, Default)]
struct ExplorerState {
    root: PathBuf,                              // git root (or opened dir)
    expanded: HashSet<PathBuf>,                 // expanded directories
    children: HashMap<PathBuf, Vec<FileDirEntry>>, // cached listings
    open_editors: Vec<PathBuf>,                 // session-opened files, in order
    outline: Vec<OutlineRow>,                   // current file's symbols
}
```

Panel chrome is a separate resource (global, like VS Code), seeded from settings:

```rust
#[derive(Resource)]
struct ExplorerChrome { visible: bool, width: u32 } // default visible=true, width=240
```

**Project root** is computed when a `FileView` spawns: walk up from the file/dir for a
`.git` entry; fall back to the containing directory. Helper added in `dir.rs`.

## Events (`vmux_core/src/event.rs`)

All `rkyv::Archive + Serialize + Deserialize` (mirroring existing `FILE_*` structs).

### Intents (page → native)

| Const | Struct | Meaning |
|-------|--------|---------|
| `file_open` *(reuse)* | `FileOpenEvent { path }` | Open a file in the editor |
| `explorer_tree_toggle` | `ExplorerTreeToggle { path }` | Expand/collapse a directory |
| `explorer_close_editor` | `ExplorerCloseEditor { path }` | Remove from Open Editors |
| `explorer_panel_toggle` | `ExplorerPanelToggle` | Show/hide the panel |
| `explorer_panel_width` | `ExplorerPanelWidth { px }` | Commit a new panel width |
| `explorer_goto` | `ExplorerGoto { path, line }` | Jump editor to a symbol's line |

### View-models (native → page)

| Const | Struct | Meaning |
|-------|--------|---------|
| `explorer_tree` | `ExplorerTreeEvent { root_name, rows: Vec<TreeRow> }` | Flattened visible tree |
| `explorer_open_editors` | `OpenEditorsEvent { items: Vec<OpenEditorItem> }` | Open-editors list |
| `explorer_outline` | `OutlineEvent { items: Vec<OutlineRow> }` | Current file symbols |
| `explorer_chrome` | `ExplorerChromeEvent { visible, width }` | Panel visibility + width |

### Row structs

```rust
struct TreeRow      { name: String, path: String, depth: u16, is_dir: bool, expanded: bool }
struct OpenEditorItem { name: String, path: String, active: bool, dirty: bool }
struct OutlineRow   { name: String, kind: u8, line: u32, depth: u16 } // kind = LSP SymbolKind
```

Page→native intents are registered in the editor's `BinEventEmitterPlugin<(...)>`
tuple; native→page emits use `BinHostEmitEvent::from_rkyv(entity, EXPLORER_*, &payload)`.

## Section 1 — OPEN EDITORS

Backend-owned, cheap. On every file open (the existing `on_file_open` observer path),
the opened path is appended to `ExplorerState.open_editors` if new; if already present
it keeps its position (open-order, like VS Code). The current file is marked `active`;
`dirty` mirrors
`EditState` dirty. Backend emits `OpenEditorsEvent`. Page renders the list; clicking a
row emits `FileOpenEvent`; the `×` emits `ExplorerCloseEditor`.

No new buffers are created — this is a navigation history of the single reused webview.

## Section 2 — `<ROOT>` folder tree

- **Initial:** on spawn, backend lists `root`, marks it expanded, emits
  `ExplorerTreeEvent { root_name = uppercased basename, rows }`.
- **Lazy expand:** page emits `ExplorerTreeToggle { path }`. Backend flips the path in
  `expanded`, lazily `list_dir`s it if not cached, rebuilds the flattened visible row
  list, and re-emits `ExplorerTreeEvent`. No upfront recursive walk — scales to large
  repos.
- **Flattening** (pure, testable): walk `root` depth-first; for each expanded dir,
  inline its cached children; emit `TreeRow`s with `depth`. Dirs-first ordering reuses
  `dir.rs` sort.
- **Rows:** rendered with `type_icon` / folder glyph + a chevron (rotated when
  expanded). Click file → `FileOpenEvent`; click dir → `ExplorerTreeToggle`.
- **Watcher:** extend the existing `notify` watcher so changes under an expanded dir
  re-`list_dir` that dir and re-emit `ExplorerTreeEvent`.

## Section 3 — OUTLINE

- **LSP path:** add `ReqKind::DocumentSymbol` in `lsp/manager.rs` (a near-clone of
  `references()` but `textDocument`-only, no position). Send on `didOpen` and on
  debounced `didChange`. Parse both response shapes — hierarchical
  `DocumentSymbol[]` (range + children) and flat `SymbolInformation[]` (location) —
  and flatten to `Vec<OutlineRow>` with `depth`. Advertise the capability in
  `lsp/client.rs`.
- **Markdown fallback:** when the language is markdown, or the LSP returns no symbols,
  a pure `markdown_outline(text) -> Vec<OutlineRow>` scans `^#{1,6}\s+` headings
  (`depth = level − 1`, `kind = String`) — the `abc` rows in the reference screenshot.
- Backend emits `OutlineEvent`. Page renders the symbol list; clicking a row emits
  `ExplorerGoto { path, line }`. A native observer scrolls/cursors the editor to that
  line via the existing `FileViewport` mechanism.

## Toggle & Persistence

- **`Cmd+B` / `Ctrl+B`** handler in the page emits `ExplorerPanelToggle`. Header toggle
  button does the same. Backend flips `ExplorerChrome.visible`, persists, re-emits
  `ExplorerChromeEvent`. (Key is platform-gated; editor text keymaps use bare `b`, so
  the `Meta`/`Ctrl` modifier avoids conflict.)
- **Splitter drag:** page updates width locally for smoothness (transient render
  state); on release emits `ExplorerPanelWidth { px }`.
- **Settings:** `vmux_setting` gains `editor.explorer { visible, width }`. Absent key →
  defaults (`visible=true`, `width=240`); no auto-seed. Width writes are debounced.
  Expanded-folder state is **session-only** in v1 (not persisted).

## Files to Touch

| File | Change |
|------|--------|
| `vmux_core/src/event.rs` | New consts + 4 view-model + 5 intent structs + row structs |
| `vmux_editor/src/explorer_model.rs` *(new, native)* | Pure: tree flatten, markdown outline, symbol flatten, open-editors ops + inline tests |
| `vmux_editor/src/explorer.rs` *(new, wasm)* | Dumb render: `ExplorerPanel`, `OpenEditorsSection`, `TreeSection`, `OutlineSection` |
| `vmux_editor/src/plugin.rs` | `ExplorerState`, `ExplorerChrome`, root detection, intent observers, view-model emits, watcher extension, goto-line, register emitters |
| `vmux_editor/src/dir.rs` | `project_root(path)` helper |
| `vmux_editor/src/lsp/manager.rs` | `ReqKind::DocumentSymbol` request + parse |
| `vmux_editor/src/lsp/client.rs` | Advertise `documentSymbol` capability |
| `vmux_editor/src/page.rs` | Layout rework, header toggle button, `Cmd+B`, mount `ExplorerPanel` |
| `vmux_editor/src/lib.rs` | Module gating for `explorer` (wasm) + `explorer_model` (native) |
| `vmux_ui/src/file_icon.rs` | Chevron glyph; open/closed folder variant (if needed) |
| `vmux_setting/...` | `editor.explorer { visible, width }` |
| `vmux_editor/tests/page_source.rs` *(new)* | Scrape test for panel sections + handlers |

No new crates (project rule). `vmux_server/assets/index.css` already `@source`s
`vmux_editor/src`; `vmux_server/build.rs` tracks the crate for WASM rebuilds — confirm
new files are covered by the existing dir glob.

## Testing

- **Pure logic** (`explorer_model.rs`, inline `#[cfg(test)]`): markdown heading parse;
  `documentSymbol` flatten (both shapes); tree flatten given children map + expanded
  set; open-editors insert/dedup/move/remove/active.
- **Bevy message integration** (per AGENTS.md): send `ExplorerTreeToggle` → run
  schedule → assert `ExplorerTreeEvent` rows; same for goto-line and panel toggle.
  Register written message types in the plugin `build()` (idempotent).
- **LSP**: `documentSymbol` request/parse unit test in `manager.rs`.
- **Source-scrape** (`tests/page_source.rs`, mirrors `vmux_layout`): assert the panel
  renders the three section headers and wires the toggle/open/expand handlers.

## Milestones

- **M0** — Layout + header toggle + `Cmd+B` + chrome persistence + folder tree (lazy
  expand, watcher).
- **M1** — OPEN EDITORS section.
- **M2** — OUTLINE (LSP `documentSymbol` + markdown fallback + goto-line).

## Out of Scope / Future

- Real multi-buffer tabs / split editors.
- Activity-bar rail; Search / Source Control / Run / Extensions views.
- File operations (new / rename / delete / drag-drop), context menus.
- Persisted expanded-folder state across restarts.
- Breadcrumbs, sticky scroll, recursive workspace search.

## Code Style Notes

- No explanatory inline comments; rustdoc `///` / `//!` allowed and expected.
- Platform-gate all native/wasm-specific code with `#[cfg(...)]`.
- Chain consecutive Bevy `App` builder calls; prefer message + system integration over
  ad hoc helpers.
