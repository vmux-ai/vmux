# File Tree Explorer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **NOTE for vmux:** CEF builds are huge and long-lived subagents drop sockets — execute this plan INLINE in one session with a warm target dir, not via subagents.

**Goal:** Add a VS Code-style, toggleable Explorer panel to the left of the `files://` editor with three collapsible sections: Open Editors, a lazy project file tree, and Outline.

**Architecture:** Dumb frontend — the native `vmux_editor` Bevy plugin owns all state (tree, expansion, open-editors, outline, panel chrome) and pushes render-ready view-models over the rkyv bridge; the Dioxus page renders them and emits intents. Pure logic lives in a native `explorer_model.rs` (unit-tested on host); shared row/intent structs live in `vmux_core`.

**Tech Stack:** Rust, Bevy ECS, Dioxus (WASM), rkyv binary event bridge, syntect/LSP (`lsp-types`), Tailwind (soft-glass), `notify` file watcher.

**Reference spec:** `docs/specs/2026-06-27-file-tree-explorer-design.md`

**Conventions:** No inline comments (rustdoc `///` ok). `#[cfg(...)]`-gate native/wasm code. Chain consecutive Bevy `App` builder calls. Prefer message+system integration in tests. Commit after each task. Run `cargo fmt` before each commit; `git checkout -- patches/` if fmt touches vendored crates.

---

## File Structure

| File | Responsibility |
|------|----------------|
| `crates/vmux_core/src/event.rs` | Shared rkyv structs + string consts for all Explorer intents/view-models |
| `crates/vmux_editor/src/explorer_model.rs` *(new, native)* | Pure builders: tree flatten, markdown outline, LSP symbol flatten, open-editors ops + unit tests |
| `crates/vmux_editor/src/explorer.rs` *(new, wasm)* | Dumb render components: `ExplorerPanel` + 3 sections |
| `crates/vmux_editor/src/plugin.rs` | `ExplorerState`/`ExplorerChrome`, root detect, intent observers, view-model emits, watcher hook, goto-line, emitter registration |
| `crates/vmux_editor/src/dir.rs` | `project_root()` helper |
| `crates/vmux_editor/src/lsp/manager.rs` | `documentSymbol` request + parse |
| `crates/vmux_editor/src/lsp/client.rs` | Advertise `documentSymbol` capability |
| `crates/vmux_editor/src/page.rs` | Layout rework, header toggle button, `Cmd+B`, mount `ExplorerPanel` |
| `crates/vmux_editor/src/lib.rs` | Module gating for `explorer` (wasm) + `explorer_model` (native+test) |
| `crates/vmux_ui/src/file_icon.rs` | Chevron glyph; open/closed folder variant |
| `crates/vmux_setting/...` | `editor.explorer { visible, width }` |
| `crates/vmux_editor/tests/page_source.rs` *(new)* | Source-scrape test for panel sections + handlers |

---

# Milestone M0 — Layout + Folder Tree

### Task 0.1: Shared event types in `vmux_core`

**Files:**
- Modify: `crates/vmux_core/src/event.rs` (consts near line 16–51; structs follow the existing `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]` pattern used by `FileDirEntry`)

- [ ] **Step 1: Add consts** (after `FILE_*` block):

```rust
pub const EXPLORER_TREE_EVENT: &str = "explorer_tree";
pub const EXPLORER_OPEN_EDITORS_EVENT: &str = "explorer_open_editors";
pub const EXPLORER_OUTLINE_EVENT: &str = "explorer_outline";
pub const EXPLORER_CHROME_EVENT: &str = "explorer_chrome";
pub const EXPLORER_TREE_TOGGLE_EVENT: &str = "explorer_tree_toggle";
pub const EXPLORER_CLOSE_EDITOR_EVENT: &str = "explorer_close_editor";
pub const EXPLORER_PANEL_TOGGLE_EVENT: &str = "explorer_panel_toggle";
pub const EXPLORER_PANEL_WIDTH_EVENT: &str = "explorer_panel_width";
pub const EXPLORER_GOTO_EVENT: &str = "explorer_goto";
```

- [ ] **Step 2: Add structs** (copy the exact derive attribute list from a neighbour like `FileDirEntry`):

```rust
// row structs
pub struct TreeRow { pub name: String, pub path: String, pub depth: u16, pub is_dir: bool, pub expanded: bool }
pub struct OpenEditorItem { pub name: String, pub path: String, pub active: bool, pub dirty: bool }
pub struct OutlineRow { pub name: String, pub kind: u8, pub line: u32, pub depth: u16 }

// view-models (native -> page)
pub struct ExplorerTreeEvent { pub root_name: String, pub rows: Vec<TreeRow> }
pub struct OpenEditorsEvent { pub items: Vec<OpenEditorItem> }
pub struct OutlineEvent { pub items: Vec<OutlineRow> }
pub struct ExplorerChromeEvent { pub visible: bool, pub width: u32 }

// intents (page -> native)
pub struct ExplorerTreeToggle { pub path: String }
pub struct ExplorerCloseEditor { pub path: String }
pub struct ExplorerPanelToggle;
pub struct ExplorerPanelWidth { pub px: u32 }
pub struct ExplorerGoto { pub path: String, pub line: u32 }
```

- [ ] **Step 3: Round-trip test** in the `event.rs` test module (mirror existing rkyv tests if present, else):

```rust
#[test]
fn explorer_tree_event_roundtrip() {
    let e = ExplorerTreeEvent { root_name: "VMUX".into(), rows: vec![
        TreeRow { name: "src".into(), path: "/r/src".into(), depth: 0, is_dir: true, expanded: true },
    ]};
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&e).unwrap();
    let back = rkyv::from_bytes::<ExplorerTreeEvent, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(e, back);
}
```

- [ ] **Step 4:** `cargo test -p vmux_core explorer` → PASS.
- [ ] **Step 5:** Commit: `feat(core): explorer event types`.

---

### Task 0.2: `explorer_model.rs` — tree flattening

**Files:**
- Create: `crates/vmux_editor/src/explorer_model.rs`
- Modify: `crates/vmux_editor/src/lib.rs` (add `#[cfg(not(target_arch = "wasm32"))] pub mod explorer_model;`)

Interface (uses `vmux_core::event::{FileDirEntry, TreeRow}`):

```rust
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use vmux_core::event::{FileDirEntry, TreeRow};

pub fn flatten_tree(
    root: &PathBuf,
    expanded: &HashSet<PathBuf>,
    children: &HashMap<PathBuf, Vec<FileDirEntry>>,
) -> Vec<TreeRow> {
    let mut out = Vec::new();
    fn walk(dir: &PathBuf, depth: u16, expanded: &HashSet<PathBuf>,
            children: &HashMap<PathBuf, Vec<FileDirEntry>>, out: &mut Vec<TreeRow>) {
        let Some(entries) = children.get(dir) else { return };
        for e in entries {
            let p = PathBuf::from(&e.path);
            let is_open = e.is_dir && expanded.contains(&p);
            out.push(TreeRow { name: e.name.clone(), path: e.path.clone(), depth, is_dir: e.is_dir, expanded: is_open });
            if is_open { walk(&p, depth + 1, expanded, children, out); }
        }
    }
    walk(root, 0, expanded, children, &mut out);
    out
}
```

- [ ] **Step 1:** Write tests first (root with two entries; expanded subdir inlines children with depth+1; collapsed subdir hides children; missing cache yields no rows). Example:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn entry(name: &str, path: &str, is_dir: bool) -> FileDirEntry {
        FileDirEntry { name: name.into(), path: path.into(), is_dir }
    }
    #[test]
    fn expanded_dir_inlines_children() {
        let root = PathBuf::from("/r");
        let mut children = HashMap::new();
        children.insert(PathBuf::from("/r"), vec![entry("src", "/r/src", true), entry("a.rs", "/r/a.rs", false)]);
        children.insert(PathBuf::from("/r/src"), vec![entry("b.rs", "/r/src/b.rs", false)]);
        let expanded = HashSet::from([PathBuf::from("/r/src")]);
        let rows = flatten_tree(&root, &expanded, &children);
        let names: Vec<_> = rows.iter().map(|r| (r.name.as_str(), r.depth)).collect();
        assert_eq!(names, vec![("src", 0), ("b.rs", 1), ("a.rs", 0)]);
    }
    #[test]
    fn collapsed_dir_hides_children() {
        let root = PathBuf::from("/r");
        let mut children = HashMap::new();
        children.insert(PathBuf::from("/r"), vec![entry("src", "/r/src", true)]);
        children.insert(PathBuf::from("/r/src"), vec![entry("b.rs", "/r/src/b.rs", false)]);
        let rows = flatten_tree(&root, &HashSet::new(), &children);
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].expanded);
    }
}
```

- [ ] **Step 2:** `cargo test -p vmux_editor flatten` → FAIL (not implemented).
- [ ] **Step 3:** Implement `flatten_tree` (above).
- [ ] **Step 4:** `cargo test -p vmux_editor flatten` → PASS.
- [ ] **Step 5:** Commit: `feat(editor): tree flatten helper`.

---

### Task 0.3: `project_root()` in `dir.rs`

**Files:** Modify `crates/vmux_editor/src/dir.rs`

```rust
pub fn project_root(start: &std::path::Path) -> std::path::PathBuf {
    let mut dir = if start.is_dir() { start } else { start.parent().unwrap_or(start) };
    loop {
        if dir.join(".git").exists() { return dir.to_path_buf(); }
        match dir.parent() { Some(p) => dir = p, None => break }
    }
    if start.is_dir() { start.to_path_buf() } else { start.parent().unwrap_or(start).to_path_buf() }
}
```

- [ ] **Step 1:** Test with `tempfile`: create `root/.git/`, `root/sub/`, assert `project_root(root/sub/file)==root`; assert fallback to containing dir when no `.git`.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS.
- [ ] **Step 5:** Commit: `feat(editor): project_root walk-up helper`.

---

### Task 0.4: `ExplorerState`/`ExplorerChrome` + initial tree emit

**Files:** Modify `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1:** Add components/resource:

```rust
#[derive(Component, Default)]
pub(crate) struct ExplorerState {
    pub root: std::path::PathBuf,
    pub expanded: std::collections::HashSet<std::path::PathBuf>,
    pub children: std::collections::HashMap<std::path::PathBuf, Vec<vmux_core::event::FileDirEntry>>,
    pub open_editors: Vec<std::path::PathBuf>,
    pub outline: Vec<vmux_core::event::OutlineRow>,
}
```

- [ ] **Step 2:** In `new_file_view_bundle` add `ExplorerState::default()` to the bundle.
- [ ] **Step 3:** Add a system `init_explorer_tree` (run in the same set as `load_file_buffers`): for any `FileView` whose `ExplorerState.root` is empty, set `root = dir::project_root(&fv.path)`, insert root into `expanded`, `children.insert(root, dir::list_dir(&root))`, then emit `ExplorerTreeEvent` via `BinHostEmitEvent::from_rkyv(entity, EXPLORER_TREE_EVENT, &ExplorerTreeEvent { root_name, rows: flatten_tree(...) })`. `root_name` = uppercased final path component.
- [ ] **Step 4:** Register the emit + (in next task) intent receivers in `EditorPlugin::build`. Add the message integration test:

```rust
#[test]
fn emits_initial_tree_on_spawn() {
    // build minimal App with EditorPlugin pieces, spawn a FileView on a temp git repo,
    // app.update(), assert a BinHostEmitEvent with name EXPLORER_TREE_EVENT was produced
    // whose decoded rows contain the root's entries.
}
```

(Follow the existing test harness used by `edit_flow_tests`/navigation tests in `plugin.rs` for constructing the App and reading emitted bin events.)

- [ ] **Step 5:** `cargo test -p vmux_editor explorer` → PASS. Commit: `feat(editor): explorer state + initial tree emit`.

---

### Task 0.5: `ExplorerTreeToggle` intent — lazy expand/collapse

**Files:** Modify `crates/vmux_editor/src/plugin.rs`; register intent in the page→native `BinEventEmitterPlugin<(...)>` tuple (near line ~1448).

- [ ] **Step 1:** Add observer/system `on_explorer_tree_toggle`: read `ExplorerTreeToggle { path }`, resolve the `FileView`'s `ExplorerState`; toggle `path` in `expanded`; if newly expanded and not cached, `children.insert(path, list_dir(path))`; re-emit `ExplorerTreeEvent`.
- [ ] **Step 2:** Register `ExplorerTreeToggle` in the emitter tuple + add the system in `build()`.
- [ ] **Step 3:** Integration test: spawn view, send `ExplorerTreeToggle{ subdir }` message, `update()`, assert re-emitted `ExplorerTreeEvent` now includes the subdir's children at depth 1; send again → collapsed.
- [ ] **Step 4:** Run → PASS. **Step 5:** Commit: `feat(editor): lazy tree expand/collapse`.

---

### Task 0.6: Watcher re-emits tree on change

**Files:** Modify `crates/vmux_editor/src/plugin.rs` (`reload_changed_files`, ~line 783)

- [ ] **Step 1:** When a changed path's parent is in `ExplorerState.children`, re-`list_dir` that parent, update the cache, and re-emit `ExplorerTreeEvent`.
- [ ] **Step 2:** Test: with an expanded dir cached, simulate a change message for a new child path under it (drive the existing change-drain system), assert tree re-emitted with the new entry. If the watcher is hard to drive in-test, factor the "apply changed dirs to ExplorerState" logic into a pure fn in `explorer_model.rs` and unit-test that.
- [ ] **Step 3:** Run → PASS. **Step 4:** Commit: `feat(editor): tree refresh on fs change`.

---

### Task 0.7: `editor.explorer` setting

**Files:** Modify `crates/vmux_setting/...` (locate the `editor` settings struct; add nested `explorer`)

```rust
#[derive(...)] pub struct ExplorerSettings { pub visible: Option<bool>, pub width: Option<u32> }
// editor settings gains: pub explorer: Option<ExplorerSettings>
```

Accessors default when absent: `visible -> true`, `width -> 240`. **No auto-seed** — do not write defaults back.

- [ ] **Step 1:** Test: absent → `visible()==true`, `width()==240`; present overrides.
- [ ] **Step 2:** Run → FAIL → implement → PASS.
- [ ] **Step 3:** Commit: `feat(setting): editor.explorer visible/width`.

---

### Task 0.8: `ExplorerChrome` resource + toggle/width intents + persistence

**Files:** Modify `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1:** Add `#[derive(Resource)] struct ExplorerChrome { visible: bool, width: u32 }`; init from settings in `build()`.
- [ ] **Step 2:** Emit `ExplorerChromeEvent` once per `FileView` after spawn (so the page learns initial state), and on every change.
- [ ] **Step 3:** Add observers: `ExplorerPanelToggle` flips `visible`; `ExplorerPanelWidth{px}` sets `width` (clamp 160..=600). Both persist to settings (debounce width writes) and re-emit `ExplorerChromeEvent`. Register both intents in the emitter tuple.
- [ ] **Step 4:** Integration test: send toggle → assert `ExplorerChromeEvent.visible` flips; send width → assert clamped value emitted.
- [ ] **Step 5:** Run → PASS. Commit: `feat(editor): panel chrome state + persistence`.

---

### Task 0.9: Page layout + `ExplorerPanel` + `TreeSection` + toggle

**Files:**
- Create: `crates/vmux_editor/src/explorer.rs` (wasm)
- Modify: `crates/vmux_editor/src/lib.rs` (`#[cfg(target_arch = "wasm32")] pub mod explorer;`)
- Modify: `crates/vmux_editor/src/page.rs` (root layout ~513, header ~643, keyboard handler)

- [ ] **Step 1:** In `explorer.rs`, add listeners + components. The panel subscribes via `use_bin_event_listener::<ArchivedExplorerTreeEvent,_>(EXPLORER_TREE_EVENT, ...)` etc., stores into signals, renders. Tree rows: indent `padding-left: {depth*12 + 8}px`, chevron for dirs (rotated when `expanded`), `type_icon(path, is_dir, ...)`. Click dir → `try_cef_bin_emit_rkyv(&ExplorerTreeToggle{path})`; click file → `try_cef_bin_emit_rkyv(&FileOpenEvent{path})`. Section header "EXPLORER" + collapsible `OPEN EDITORS` / `<root_name>` / `OUTLINE` using `vmux_ui` `Collapsible*`. Soft-glass classes consistent with `PANE_CLASS`.
- [ ] **Step 2:** In `page.rs`: wrap content in a horizontal flex; render `ExplorerPanel { width, visible }` (driven by an `ExplorerChromeEvent` listener) left of the editor main column; add a splitter div that updates a local width signal during drag and emits `ExplorerPanelWidth` on pointer-up. Add a toggle button to the left of the header path that emits `ExplorerPanelToggle`. Add a key handler: `Meta+b` (mac) / `Ctrl+b` else → emit `ExplorerPanelToggle` (guard so it doesn't reach editor text input).
- [ ] **Step 3:** `cargo check --target wasm32-unknown-unknown -p vmux_editor` → builds.
- [ ] **Step 4:** Create `crates/vmux_editor/tests/page_source.rs` mirroring `crates/vmux_layout/tests/page_source.rs`: assert the explorer source string contains `EXPLORER`, `OPEN EDITORS`, `OUTLINE`, `EXPLORER_TREE_TOGGLE_EVENT`/`explorer_tree_toggle`, and the `FILE_OPEN_EVENT` wiring. `cargo test -p vmux_editor page_source` → PASS.
- [ ] **Step 5:** Commit: `feat(editor): explorer panel + tree section + Cmd+B toggle`.

---

# Milestone M1 — Open Editors

### Task 1.1: Open-editors ops (pure)

**Files:** Modify `crates/vmux_editor/src/explorer_model.rs`

```rust
use std::path::PathBuf;
pub fn note_open(list: &mut Vec<PathBuf>, path: &PathBuf) { if !list.contains(path) { list.push(path.clone()); } }
pub fn close(list: &mut Vec<PathBuf>, path: &PathBuf) { list.retain(|p| p != path); }
```

- [ ] **Step 1:** Tests: `note_open` dedups + preserves order; `close` removes; closing absent is a no-op.
- [ ] **Step 2:** FAIL → implement → PASS.
- [ ] **Step 3:** Commit: `feat(editor): open-editors list ops`.

---

### Task 1.2: Maintain + emit open editors

**Files:** Modify `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1:** In the `on_file_open` path and initial load, call `note_open(&mut state.open_editors, &path)` and emit `OpenEditorsEvent` (build `OpenEditorItem`s: `name`=file_name, `active`= path==current `fv.path`, `dirty`= from `EditState`).
- [ ] **Step 2:** Add `on_explorer_close_editor` observer: `close(...)`; emit. Register `ExplorerCloseEditor` intent.
- [ ] **Step 3:** Integration test: open file A then B → `OpenEditorsEvent` lists [A,B] with B active; close A → lists [B].
- [ ] **Step 4:** Run → PASS. Commit: `feat(editor): open-editors tracking + emit`.

---

### Task 1.3: `OpenEditorsSection` render

**Files:** Modify `crates/vmux_editor/src/explorer.rs`

- [ ] **Step 1:** Subscribe to `OPEN_EDITORS` view-model; render rows with `type_icon`, active highlight (reuse selected-row glow), dirty dot, and an `×` button → `try_cef_bin_emit_rkyv(&ExplorerCloseEditor{path})`; row click → `FileOpenEvent`.
- [ ] **Step 2:** `cargo check --target wasm32 -p vmux_editor` → builds. Extend `page_source.rs` to assert close-handler wiring.
- [ ] **Step 3:** `cargo test -p vmux_editor page_source` → PASS. Commit: `feat(editor): open editors section`.

---

# Milestone M2 — Outline

### Task 2.1: Markdown outline parser (pure)

**Files:** Modify `crates/vmux_editor/src/explorer_model.rs`

```rust
use vmux_core::event::OutlineRow;
pub fn markdown_outline(text: &str) -> Vec<OutlineRow> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") { in_fence = !in_fence; continue; }
        if in_fence { continue; }
        let hashes = t.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            out.push(OutlineRow { name: t[hashes..].trim().to_string(), kind: 15, line: i as u32, depth: (hashes - 1) as u16 });
        }
    }
    out
}
```

(`kind = 15` = LSP `SymbolKind::String`, the `abc` icon.)

- [ ] **Step 1:** Tests: `# A`/`## B` → depths 0/1, lines correct; `#nospace` ignored; headings inside ``` fences ignored.
- [ ] **Step 2:** FAIL → implement → PASS. **Step 3:** Commit: `feat(editor): markdown outline parser`.

---

### Task 2.2: LSP symbol flatten (pure)

**Files:** Modify `crates/vmux_editor/src/explorer_model.rs`

- [ ] **Step 1:** `pub fn flatten_symbols(value: &serde_json::Value) -> Vec<OutlineRow>` handling both `DocumentSymbol[]` (recurse `children`, depth+1, line from `selectionRange.start.line` or `range.start.line`, `kind` from `kind`) and `SymbolInformation[]` (flat, line from `location.range.start.line`). Names from `name`, `kind` cast to `u8`.
- [ ] **Step 2:** Tests with two JSON fixtures (hierarchical + flat) asserting names/kinds/lines/depths.
- [ ] **Step 3:** FAIL → implement → PASS. **Step 4:** Commit: `feat(editor): lsp documentSymbol flatten`.

---

### Task 2.3: `documentSymbol` request + emit

**Files:** Modify `crates/vmux_editor/src/lsp/manager.rs` (clone `references()` ~296 and `send_doc_request` ~239; add `ReqKind::DocumentSymbol`, parse arm ~553+); `crates/vmux_editor/src/lsp/client.rs` (~179 capability)

- [ ] **Step 1:** Add `document_symbol(entity, path)` sending `textDocument/documentSymbol` with `{ textDocument:{ uri } }` only. Add `ReqKind::DocumentSymbol`. On response, `flatten_symbols(&json)` → store in `ExplorerState.outline` → emit `OutlineEvent`. Send on `didOpen` and debounced `didChange`.
- [ ] **Step 2:** Fallback: when the file language is markdown OR `flatten_symbols` yields empty, use `markdown_outline(buffer_text)` (markdown only) and emit. Wire in the native open/change path (the buffer text is available in `EditState`/`FileBuffer`).
- [ ] **Step 3:** Advertise `documentSymbolProvider` client capability in `client.rs`.
- [ ] **Step 4:** Unit-test the parse arm via `flatten_symbols` (already covered) + a manager test if the existing harness supports faking a response (mirror references test if present). Otherwise rely on 2.2 + manual.
- [ ] **Step 5:** Commit: `feat(editor): documentSymbol outline + markdown fallback`.

---

### Task 2.4: `ExplorerGoto` → scroll editor to line

**Files:** Modify `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1:** Add `on_explorer_goto` observer: read `ExplorerGoto{path,line}`; if `path` is the current file, set the `FileViewport` scroll so `line` is visible (reuse the mechanism `on_file_scroll`/goto uses) and place the cursor at `line`. Register `ExplorerGoto` intent.
- [ ] **Step 2:** Integration test: send `ExplorerGoto{ current, line: N }`, `update()`, assert the emitted `FileViewportPatch` (or viewport component) targets line N.
- [ ] **Step 3:** Run → PASS. Commit: `feat(editor): outline goto-line`.

---

### Task 2.5: `OutlineSection` render

**Files:** Modify `crates/vmux_editor/src/explorer.rs`

- [ ] **Step 1:** Subscribe to `OUTLINE` view-model; render indented rows (`depth*12px`), a small `kind` glyph (use `abc`-style for `kind==15`, else a generic symbol icon), click → `try_cef_bin_emit_rkyv(&ExplorerGoto{path, line})` (path = current file).
- [ ] **Step 2:** `cargo check --target wasm32 -p vmux_editor` → builds. Extend `page_source.rs` to assert goto wiring.
- [ ] **Step 3:** `cargo test -p vmux_editor page_source` → PASS. Commit: `feat(editor): outline section`.

---

# Final Verification Pass (single pass, end of plan)

- [ ] `cargo fmt --all`; `git checkout -- patches/` if vendored crates were reformatted.
- [ ] `cargo clippy -p vmux_core -p vmux_editor -p vmux_setting --all-targets` → no warnings.
- [ ] `cargo test -p vmux_core -p vmux_editor -p vmux_setting` → green.
- [ ] WASM rebuild check: ensure new editor files are tracked in `crates/vmux_server/build.rs` `track_manifest_rel_paths`; `cargo check --target wasm32-unknown-unknown -p vmux_editor`.
- [ ] Runtime test by the user: `make dev`, open a file, verify panel toggles with `Cmd+B`, tree expands/opens files, open-editors list updates, outline lists symbols/headings and jumps on click.
- [ ] Delete this plan file (`docs/plans/2026-06-28-file-tree-explorer.md`) once fully implemented.
- [ ] Open PR via `gh pr create`.

---

## Self-Review Notes

- **Spec coverage:** layout/toggle (0.9, 0.8), tree (0.2/0.4/0.5/0.6), open editors (1.x), outline LSP+markdown+goto (2.x), persistence (0.7/0.8), dumb-frontend (all view-models in 0.x/1.x/2.x), tests (per task + final). Covered.
- **Type consistency:** `flatten_tree`, `note_open`/`close`, `markdown_outline`, `flatten_symbols`, `project_root`, event structs/consts used consistently across tasks.
- **Risk:** Bevy in-test harness for emitted bin events — reuse the exact pattern from `plugin.rs` existing tests; if reading emitted `BinHostEmitEvent`s in-test is awkward, assert on `ExplorerState`/viewport component mutations instead (still message-driven).
