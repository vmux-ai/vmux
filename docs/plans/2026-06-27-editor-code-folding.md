# Editor Code Folding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **vmux note:** Do NOT subagent-drive this — CEF builds are huge and long-lived agents drop sockets. Execute inline with a warm `target/`. Defer runtime testing to ONE pass at the end (the user runtime-tests). Run native `cargo test -p vmux_editor -p vmux_core` during the loop.

**Goal:** Collapse/expand line regions in the file editor like VSCode/Vim, sourced from LSP `foldingRange` with an indentation fallback, toggled by gutter chevrons and keyboard, persisted per file across restarts.

**Architecture:** The backend (Bevy plugin + `EditCore`, native Rust) owns all fold state and windows the document in **visual-row** space; the Dioxus/WASM page renders fully-resolved rows and emits intents (chevron click, keystroke, scroll) — it computes no fold geometry. A `FoldView` (merged hidden line-ranges) is the shared derived structure used by both motion resolution (`EditCore`) and row↔line mapping (plugin).

**Tech Stack:** Rust, Bevy ECS, ropey, syntect, Dioxus (WASM/CEF), rkyv + serde wire types, RON persistence.

**Spec:** `docs/specs/2026-06-27-editor-code-folding-design.md`

---

## File Structure

**Create:**
- `crates/vmux_editor/src/fold.rs` — `FoldRegion`, `FoldState`, `FoldView`, mapping + fold ops, `indent_regions`.
- `crates/vmux_editor/src/fold_store.rs` — `folds.ron` load/save.

**Modify:**
- `crates/vmux_core/src/editor.rs` — `row` on `CursorPos` + `SelSpan`.
- `crates/vmux_core/src/event.rs` — `FoldGutter`, `FileLine.fold`, `FileViewportPatch` fields, `FileFoldToggle` + `FILE_FOLD_TOGGLE_EVENT`.
- `crates/vmux_editor/src/lib.rs` — declare `fold`, `fold_store`.
- `crates/vmux_editor/src/edit/command.rs` — fold `EditCommand` variants.
- `crates/vmux_editor/src/edit/core.rs` — `fold_view` field; fold-aware vertical motions; `cursor_pos`/`sel_spans` unchanged (row set by plugin).
- `crates/vmux_editor/src/edit/highlight_cache.rs` — set `fold: FoldGutter::None` in `line_window`.
- `crates/vmux_editor/src/plugin.rs` — `EditState.folds`; row-space windowing; fold command + reveal in `run_commands`; `on_file_fold_toggle`; persistence load/save; region recompute on open/edit; `FileViewport` field rename `top_line`→`top_row`.
- `crates/vmux_editor/src/keymap/vim.rs` — `z_pending` + fold keys.
- `crates/vmux_editor/src/keymap/vscode.rs` — fold shortcuts.
- `crates/vmux_editor/src/page.rs` — render by visual row, chevrons, `⋯` placeholder, emit `FileFoldToggle`, scroll `top_row`.
- `crates/vmux_editor/src/lsp/manager.rs` — `foldingRange` request + response (M2).
- `crates/vmux_editor/src/bin/vmux_mock_lsp.rs` — mock `foldingRange` (M2).

---

# Milestone 1 — Fold engine, indent source, gutter + keyboard, persistence

## Task 1: Wire types

**Files:**
- Modify: `crates/vmux_core/src/editor.rs`
- Modify: `crates/vmux_core/src/event.rs`

- [ ] **Step 1: Add `row` to `CursorPos` and `SelSpan`**

In `crates/vmux_core/src/editor.rs`, add a `row` field to both structs (after `line`):

```rust
pub struct CursorPos {
    pub line: u32,
    pub row: u32,
    pub col: u32,
}
```
```rust
pub struct SelSpan {
    pub line: u32,
    pub row: u32,
    pub start: u32,
    pub end: u32,
}
```

- [ ] **Step 2: Add `FoldGutter` enum + `FileLine.fold` + patch fields + toggle event in `event.rs`**

Add near the other file types (mirror the derive block used by `FileLine`):

```rust
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default,
    Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub enum FoldGutter {
    #[default]
    None,
    Open,
    Collapsed,
}
```

Add field to `FileLine`:

```rust
pub struct FileLine {
    pub line_no: u32,
    pub fold: FoldGutter,
    pub spans: Vec<StyledSpan>,
}
```

Replace `FileViewportPatch`:

```rust
pub struct FileViewportPatch {
    pub first_row: u32,
    pub total_rows: u32,
    pub total_lines: u32,
    pub lines: Vec<FileLine>,
}
```

Add a channel const beside the others (`FILE_SCROLL_EVENT` etc.):

```rust
pub const FILE_FOLD_TOGGLE_EVENT: &str = "file_fold_toggle";
```

Add the event (Copy/Eq, like `FileScrollEvent`):

```rust
pub struct FileFoldToggle {
    pub line: u32,
}
```

- [ ] **Step 3: Fix the existing rkyv round-trip test for `FileLine`**

In `event.rs` tests (~line 582), the `FileViewportPatch`/`FileLine` literal must set the new fields. Update it:

```rust
let patch = FileViewportPatch {
    first_row: 0,
    total_rows: 1,
    total_lines: 1,
    lines: vec![FileLine {
        line_no: 100,
        fold: crate::event::FoldGutter::None,
        spans: vec![],
    }],
};
```
(Keep the existing assertion on `decoded.lines[0].line_no`.)

- [ ] **Step 4: Build core**

Run: `cargo build -p vmux_core`
Expected: PASS (downstream crates intentionally break until later tasks; that's fine).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/editor.rs crates/vmux_core/src/event.rs
git commit -m "feat(editor): fold wire types (FoldGutter, row, FileFoldToggle)"
```

---

## Task 2: Fold model + `FoldView` mapping

**Files:**
- Create: `crates/vmux_editor/src/fold.rs`
- Modify: `crates/vmux_editor/src/lib.rs`

- [ ] **Step 1: Declare the module**

In `lib.rs`, after `pub mod edit;`:

```rust
pub mod fold;
```

- [ ] **Step 2: Write `fold.rs` with the model, view, ops, and tests**

```rust
use std::collections::HashSet;

use ropey::Rope;
use vmux_core::event::FoldGutter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoldRegion {
    pub start: u32,
    pub end: u32,
}

impl FoldRegion {
    pub fn contains_body(self, line: u32) -> bool {
        line > self.start && line <= self.end
    }
    pub fn contains(self, line: u32) -> bool {
        line >= self.start && line <= self.end
    }
}

#[derive(Default, Clone, Debug)]
pub struct FoldState {
    pub regions: Vec<FoldRegion>,
    pub collapsed: HashSet<u32>,
}

impl FoldState {
    pub fn set_regions(&mut self, regions: Vec<FoldRegion>) {
        self.regions = regions;
        self.reconcile();
    }

    fn region_at_start(&self, start: u32) -> Option<FoldRegion> {
        self.regions.iter().copied().find(|r| r.start == start)
    }

    pub fn enclosing(&self, line: u32) -> Option<FoldRegion> {
        self.regions
            .iter()
            .copied()
            .filter(|r| r.contains(line))
            .min_by_key(|r| r.end - r.start)
    }

    pub fn gutter(&self, line: u32) -> FoldGutter {
        match self.region_at_start(line) {
            Some(_) if self.collapsed.contains(&line) => FoldGutter::Collapsed,
            Some(_) => FoldGutter::Open,
            None => FoldGutter::None,
        }
    }

    pub fn toggle(&mut self, line: u32) {
        if let Some(r) = self.enclosing(line)
            && !self.collapsed.remove(&r.start)
        {
            self.collapsed.insert(r.start);
        }
    }
    pub fn open(&mut self, line: u32) {
        if let Some(r) = self.enclosing(line) {
            self.collapsed.remove(&r.start);
        }
    }
    pub fn close(&mut self, line: u32) {
        if let Some(r) = self.enclosing(line) {
            self.collapsed.insert(r.start);
        }
    }
    pub fn toggle_recursive(&mut self, line: u32) {
        let Some(top) = self.enclosing(line) else {
            return;
        };
        let want_collapse = !self.collapsed.contains(&top.start);
        let inner: Vec<u32> = self
            .regions
            .iter()
            .filter(|r| top.contains(r.start) && top.contains(r.end))
            .map(|r| r.start)
            .collect();
        for s in inner {
            if want_collapse {
                self.collapsed.insert(s);
            } else {
                self.collapsed.remove(&s);
            }
        }
    }
    pub fn fold_all(&mut self) {
        self.collapsed = self.regions.iter().map(|r| r.start).collect();
    }
    pub fn unfold_all(&mut self) {
        self.collapsed.clear();
    }

    pub fn reveal(&mut self, line: u32) {
        let open: Vec<u32> = self
            .collapsed
            .iter()
            .copied()
            .filter(|s| self.region_at_start(*s).is_some_and(|r| r.contains_body(line)))
            .collect();
        for s in open {
            self.collapsed.remove(&s);
        }
    }

    pub fn shift(&mut self, at_line: u32, delta: i64) {
        if delta == 0 {
            return;
        }
        self.collapsed = self
            .collapsed
            .iter()
            .map(|&s| {
                if s >= at_line {
                    (s as i64 + delta).max(0) as u32
                } else {
                    s
                }
            })
            .collect();
    }

    pub fn reconcile(&mut self) {
        let starts: HashSet<u32> = self.regions.iter().map(|r| r.start).collect();
        self.collapsed.retain(|s| starts.contains(s));
    }

    pub fn view(&self, total: u32) -> FoldView {
        let mut spans: Vec<(u32, u32)> = self
            .collapsed
            .iter()
            .filter_map(|s| self.region_at_start(*s))
            .map(|r| (r.start + 1, r.end.min(total.saturating_sub(1))))
            .filter(|(a, b)| a <= b)
            .collect();
        spans.sort_unstable();
        let mut hidden: Vec<(u32, u32)> = Vec::new();
        for (a, b) in spans {
            match hidden.last_mut() {
                Some(last) if a <= last.1 + 1 => last.1 = last.1.max(b),
                _ => hidden.push((a, b)),
            }
        }
        FoldView { hidden, total }
    }
}

#[derive(Default, Clone, Debug)]
pub struct FoldView {
    hidden: Vec<(u32, u32)>,
    total: u32,
}

impl FoldView {
    pub fn is_hidden(&self, line: u32) -> bool {
        self.hidden.iter().any(|(a, b)| line >= *a && line <= *b)
    }
    pub fn hidden_before(&self, line: u32) -> u32 {
        let mut n = 0;
        for (a, b) in &self.hidden {
            if *b < line {
                n += b - a + 1;
            } else if *a < line {
                n += line - a;
            }
        }
        n
    }
    pub fn buffer_to_row(&self, line: u32) -> u32 {
        line - self.hidden_before(line)
    }
    pub fn visible_count(&self) -> u32 {
        let hidden: u32 = self.hidden.iter().map(|(a, b)| b - a + 1).sum();
        self.total.saturating_sub(hidden).max(1)
    }
    pub fn next_visible(&self, line: u32) -> u32 {
        let mut l = line;
        while l + 1 < self.total && self.is_hidden(l) {
            l += 1;
        }
        l
    }
    pub fn step_rows(&self, line: u32, delta: i64) -> u32 {
        if self.total == 0 {
            return 0;
        }
        let last = self.total - 1;
        let mut l = line as i64;
        let dir = delta.signum();
        let mut steps = delta.abs();
        while steps > 0 {
            let mut n = l + dir;
            while n >= 0 && (n as u32) <= last && self.is_hidden(n as u32) {
                n += dir;
            }
            if n < 0 || (n as u32) > last {
                break;
            }
            l = n;
            steps -= 1;
        }
        (l.max(0) as u32).min(last)
    }
    pub fn lines_for_window(&self, first_row: u32, rows: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut count = 0u32;
        let mut l = 0u32;
        while l < self.total && (out.len() as u32) < rows {
            if !self.is_hidden(l) {
                if count >= first_row {
                    out.push(l);
                }
                count += 1;
            }
            l += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> FoldState {
        let mut s = FoldState::default();
        s.set_regions(vec![
            FoldRegion { start: 1, end: 4 },
            FoldRegion { start: 6, end: 8 },
        ]);
        s
    }

    #[test]
    fn gutter_marks_headers() {
        let mut s = state();
        assert_eq!(s.gutter(1), FoldGutter::Open);
        assert_eq!(s.gutter(2), FoldGutter::None);
        s.close(1);
        assert_eq!(s.gutter(1), FoldGutter::Collapsed);
    }

    #[test]
    fn view_hides_body_only() {
        let mut s = state();
        s.close(1);
        let v = s.view(10);
        assert!(!v.is_hidden(1));
        assert!(v.is_hidden(2) && v.is_hidden(4));
        assert!(!v.is_hidden(5));
        assert_eq!(v.visible_count(), 10 - 4);
        assert_eq!(v.buffer_to_row(5), 5 - 4);
    }

    #[test]
    fn step_rows_skips_hidden() {
        let mut s = state();
        s.close(1);
        let v = s.view(10);
        assert_eq!(v.step_rows(1, 1), 5);
        assert_eq!(v.step_rows(5, -1), 1);
    }

    #[test]
    fn window_returns_visible_lines() {
        let mut s = state();
        s.close(1);
        let v = s.view(10);
        assert_eq!(v.lines_for_window(0, 4), vec![0, 1, 5, 6]);
    }

    #[test]
    fn toggle_recursive_folds_nested() {
        let mut s = FoldState::default();
        s.set_regions(vec![
            FoldRegion { start: 0, end: 9 },
            FoldRegion { start: 2, end: 4 },
        ]);
        s.toggle_recursive(0);
        assert!(s.collapsed.contains(&0) && s.collapsed.contains(&2));
        s.toggle_recursive(0);
        assert!(s.collapsed.is_empty());
    }

    #[test]
    fn reveal_opens_enclosing() {
        let mut s = state();
        s.close(1);
        s.reveal(3);
        assert!(!s.collapsed.contains(&1));
    }

    #[test]
    fn shift_moves_collapsed_starts() {
        let mut s = state();
        s.close(6);
        s.shift(2, 3);
        assert!(s.collapsed.contains(&9));
    }

    #[test]
    fn reconcile_drops_stale() {
        let mut s = state();
        s.close(6);
        s.set_regions(vec![FoldRegion { start: 1, end: 4 }]);
        assert!(!s.collapsed.contains(&6));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p vmux_editor fold::tests`
Expected: PASS (7 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_editor/src/fold.rs crates/vmux_editor/src/lib.rs
git commit -m "feat(editor): fold model + FoldView row mapping"
```

---

## Task 3: Indentation fold provider

**Files:**
- Modify: `crates/vmux_editor/src/fold.rs`

- [ ] **Step 1: Add `indent_regions` and tests**

Append to `fold.rs`:

```rust
fn indent_width(line: &str) -> Option<usize> {
    let mut w = 0;
    for c in line.chars() {
        match c {
            ' ' => w += 1,
            '\t' => w += 4,
            _ => return Some(w),
        }
    }
    None
}

pub fn indent_regions(rope: &Rope) -> Vec<FoldRegion> {
    let total = rope.len_lines();
    let indents: Vec<Option<usize>> = (0..total)
        .map(|i| {
            let s: String = rope
                .line(i)
                .chars()
                .filter(|c| *c != '\n' && *c != '\r')
                .collect();
            indent_width(&s)
        })
        .collect();
    let mut regions = Vec::new();
    for i in 0..total {
        let Some(cur) = indents[i] else { continue };
        let mut j = i + 1;
        let mut last = i;
        while j < total {
            match indents[j] {
                None => j += 1,
                Some(d) if d > cur => {
                    last = j;
                    j += 1;
                }
                Some(_) => break,
            }
        }
        if last > i {
            regions.push(FoldRegion {
                start: i as u32,
                end: last as u32,
            });
        }
    }
    regions
}

#[cfg(test)]
mod indent_tests {
    use super::*;

    #[test]
    fn folds_indented_block() {
        let r = Rope::from_str("fn a() {\n    x;\n    y;\n}\nz;\n");
        let regs = indent_regions(&r);
        assert!(regs.contains(&FoldRegion { start: 0, end: 2 }));
    }

    #[test]
    fn excludes_trailing_blanks() {
        let r = Rope::from_str("a:\n  b\n\n\nc\n");
        let regs = indent_regions(&r);
        assert_eq!(regs, vec![FoldRegion { start: 0, end: 1 }]);
    }

    #[test]
    fn nests_deeper_blocks() {
        let r = Rope::from_str("a:\n  b:\n    c\n  d\ne\n");
        let regs = indent_regions(&r);
        assert!(regs.contains(&FoldRegion { start: 0, end: 3 }));
        assert!(regs.contains(&FoldRegion { start: 1, end: 2 }));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p vmux_editor indent_tests`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/fold.rs
git commit -m "feat(editor): indentation fold provider"
```

---

## Task 4: Fold persistence store

**Files:**
- Create: `crates/vmux_editor/src/fold_store.rs`
- Modify: `crates/vmux_editor/src/lib.rs`

- [ ] **Step 1: Declare module**

In `lib.rs`, after `pub mod fold;`:

```rust
pub mod fold_store;
```

- [ ] **Step 2: Write `fold_store.rs`**

The on-disk file maps absolute path → collapsed header lines. Path comes from `vmux_core::profile::shared_data_dir()`.

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct FoldStore {
    pub files: HashMap<String, Vec<u32>>,
}

fn store_path() -> PathBuf {
    vmux_core::profile::shared_data_dir().join("folds.ron")
}

fn key(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

impl FoldStore {
    pub fn load() -> Self {
        let Ok(text) = std::fs::read_to_string(store_path()) else {
            return Self::default();
        };
        ron::from_str(&text).unwrap_or_default()
    }

    pub fn get(&self, path: &Path) -> Vec<u32> {
        self.files.get(&key(path)).cloned().unwrap_or_default()
    }

    pub fn set(&mut self, path: &Path, collapsed: &[u32]) {
        let k = key(path);
        if collapsed.is_empty() {
            self.files.remove(&k);
        } else {
            let mut v = collapsed.to_vec();
            v.sort_unstable();
            self.files.insert(k, v);
        }
    }

    pub fn save(&self) {
        let path = store_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = ron::ser::to_string(self) {
            let tmp = path.with_extension("ron.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_roundtrip_and_zero_removes() {
        let mut s = FoldStore::default();
        let p = Path::new("/tmp/vmux-fold-test.rs");
        s.set(p, &[3, 1]);
        assert_eq!(s.get(p), vec![1, 3]);
        s.set(p, &[]);
        assert!(s.get(p).is_empty());
    }
}
```

- [ ] **Step 3: Confirm `ron` is a dependency**

Run: `grep '^ron' crates/vmux_editor/Cargo.toml || grep 'ron' crates/vmux_editor/Cargo.toml`
If absent, add `ron = "0.8"` under `[dependencies]` (match the workspace version used elsewhere: `grep -rh '^ron' crates/*/Cargo.toml | head -1`).

- [ ] **Step 4: Run tests**

Run: `cargo test -p vmux_editor fold_store`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/fold_store.rs crates/vmux_editor/src/lib.rs crates/vmux_editor/Cargo.toml
git commit -m "feat(editor): per-file fold persistence store"
```

---

## Task 5: Fold commands + fold-aware motions in `EditCore`

**Files:**
- Modify: `crates/vmux_editor/src/edit/command.rs`
- Modify: `crates/vmux_editor/src/edit/core.rs`

- [ ] **Step 1: Add fold `EditCommand` variants**

In `command.rs`, extend the enum (after `TriggerCompletion`):

```rust
    FoldToggle,
    FoldOpen,
    FoldClose,
    FoldToggleRecursive,
    FoldAll,
    UnfoldAll,
```

- [ ] **Step 2: Add `fold_view` to `EditCore` + make vertical motions fold-aware**

In `core.rs`, add the field to the struct and initialize it in `new`:

```rust
    pub fold_view: crate::fold::FoldView,
```
```rust
            fold_view: crate::fold::FoldView::default(),
```

Change `vertical` to step over visible rows:

```rust
    fn vertical(&self, from: usize, delta: i64) -> usize {
        let (l, c) = self.buffer.char_to_coords(from);
        let target = self.fold_view.step_rows(l as u32, delta) as usize;
        self.buffer.coords_to_char(target, c)
    }
```

Add fold commands as no-ops in `apply` (they are handled at the plugin level — group them with the existing `Save | GotoDefinition | ...` arm):

```rust
            EditCommand::Save
            | EditCommand::GotoDefinition
            | EditCommand::FindReferences
            | EditCommand::Hover
            | EditCommand::TriggerCompletion
            | EditCommand::FoldToggle
            | EditCommand::FoldOpen
            | EditCommand::FoldClose
            | EditCommand::FoldToggleRecursive
            | EditCommand::FoldAll
            | EditCommand::UnfoldAll => {}
```

- [ ] **Step 3: Add a test (default view = no folds keeps current behavior; with folds, Down skips)**

Append to `core.rs` tests:

```rust
    #[test]
    fn down_skips_collapsed_body() {
        let mut c = core("a\nb\nc\nd\ne\n");
        let mut fs = crate::fold::FoldState::default();
        fs.set_regions(vec![crate::fold::FoldRegion { start: 0, end: 2 }]);
        fs.close(0);
        c.fold_view = fs.view(c.buffer.len_lines() as u32);
        c.set_caret(0);
        c.apply(EditCommand::Move(Motion::Down));
        let (line, _) = c.buffer.char_to_coords(c.primary().head);
        assert_eq!(line, 3);
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p vmux_editor edit::core`
Expected: PASS (existing `autoscroll_follows_caret_down` and `down_skips_collapsed_body` included).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/edit/command.rs crates/vmux_editor/src/edit/core.rs
git commit -m "feat(editor): fold commands + fold-aware vertical motion"
```

---

## Task 6: `FileLine.fold` default in highlighter

**Files:**
- Modify: `crates/vmux_editor/src/edit/highlight_cache.rs`
- Modify: `crates/vmux_editor/src/highlight.rs`

- [ ] **Step 1: Set the new field**

In `highlight_cache.rs` `line_window`, the `FileLine` literal becomes:

```rust
            out.push(FileLine {
                line_no: i as u32,
                fold: vmux_core::event::FoldGutter::None,
                spans,
            });
```

In `highlight.rs`, both `FileLine { ... }` literals (`highlight_snippet` ~line 50 and the windowed builder ~line 103) gain `fold: vmux_core::event::FoldGutter::None,` (import is already `use vmux_core::event::{FileLine, StyledSpan};` — extend to include `FoldGutter`).

- [ ] **Step 2: Build**

Run: `cargo build -p vmux_editor`
Expected: compile past these files (plugin/page errors remain until later tasks).

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/edit/highlight_cache.rs crates/vmux_editor/src/highlight.rs
git commit -m "feat(editor): default FoldGutter on highlighted lines"
```

---

## Task 7: Plugin — row-space windowing + cursor rows

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1: Add folds to `EditState` and rename the viewport field**

```rust
pub struct EditState {
    pub core: EditCore,
    pub hl: HighlightCache,
    pub folds: crate::fold::FoldState,
}
```
```rust
pub struct FileViewport {
    pub top_row: u32,
    pub rows: u16,
}
```
Update every `vp.top_line` / `top_line:` initializer to `top_row` (the `FileViewport { top_line: 0, rows: 0 }` at ~line 132, and the reset at ~line 696). `clamp_top_line`/`window_range` in `viewport.rs` are generic (operate on counts) — reuse as-is against the **visible row count**.

- [ ] **Step 2: Add a fold-aware window helper + rewrite `emit_window`**

Replace `emit_window`:

```rust
fn emit_window(
    entity: Entity,
    edit: &mut EditState,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    let total = edit.core.buffer.len_lines() as u32;
    let view = edit.folds.view(total);
    let visible = view.visible_count();
    let (first_row, end_row) = window_range(visible, vp.top_row, vp.rows);
    let first_row = first_row.saturating_sub(SCROLL_OVERSCAN);
    let end_row = (end_row + SCROLL_OVERSCAN).min(visible);
    let line_nos = view.lines_for_window(first_row, end_row - first_row);
    let mut lines = Vec::with_capacity(line_nos.len());
    for ln in line_nos {
        let mut fl = edit
            .hl
            .line_window(&edit.core.buffer.rope, ln as usize, ln as usize + 1);
        if let Some(mut l) = fl.pop() {
            l.fold = edit.folds.gutter(ln);
            lines.push(l);
        }
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_VIEWPORT_EVENT,
        &FileViewportPatch {
            first_row,
            total_rows: visible,
            total_lines: total,
            lines,
        },
    ));
}
```

(Calling `line_window` per visible line keeps emission to visible lines only; syntect `befores` are cached so per-line calls reuse state.)

- [ ] **Step 3: Rewrite `emit_cursor` to set rows + drop hidden selection lines**

```rust
fn emit_cursor(
    entity: Entity,
    edit: &EditState,
    keymap: &dyn Keymap,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    let total = edit.core.buffer.len_lines() as u32;
    let view = edit.folds.view(total);
    let (first, end) = window_range(view.visible_count(), vp.top_row, vp.rows);
    let rows = (end - first).min(u16::MAX as u32) as u16;
    let mut primary = edit.core.cursor_pos();
    primary.row = view.buffer_to_row(primary.line);
    let selections = edit
        .core
        .sel_spans(0, total as u16)
        .into_iter()
        .filter(|s| !view.is_hidden(s.line))
        .map(|mut s| {
            s.row = view.buffer_to_row(s.line);
            s
        })
        .collect();
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_CURSOR_EVENT,
        &FileCursorEvent {
            mode: keymap.mode(),
            mode_label: keymap.mode_label(),
            primary,
            selections,
        },
    ));
}
```

Note the signature changed (`core: &EditCore` → `edit: &EditState`). Update its three call sites (`on_file_resize`, `on_file_scroll`, `run_commands`) to pass `&edit` / `edit`. `sel_spans(0, total)` returns all selection lines; the plugin now does the visible-window filtering.

- [ ] **Step 4: Update `EditState` constructor + `on_file_scroll`**

In `load_file_buffers` (~line 300), build folds from indentation at load and seed persisted collapses (persistence wired in Task 9; for now default + indent):

```rust
        let mut folds = crate::fold::FoldState::default();
        folds.set_regions(crate::fold::indent_regions(&core.buffer.rope));
        commands.entity(entity).insert((
            EditState { core, hl, folds },
            EditorKeymap(kind.make()),
        ));
```

In `on_file_scroll`, clamp against visible rows:

```rust
    let total = edit.core.buffer.len_lines() as u32;
    let visible = edit.folds.view(total).visible_count();
    vp.top_row = clamp_top_line(evt.top_line, visible, vp.rows);
```
(The `FileScrollEvent` field is still named `top_line` on the wire until Task 12 renames it; keep reading `evt.top_line` here and rename together in Task 12, OR rename the wire field now and read `evt.top_row`. Pick one and stay consistent — this plan renames the wire field in Task 1? No: Task 1 left `FileScrollEvent` alone. Rename `FileScrollEvent.top_line`→`top_row` HERE in event.rs, and read `evt.top_row`.)

Apply the rename now: in `crates/vmux_core/src/event.rs`, `FileScrollEvent { pub top_row: u32 }`, and fix its rkyv test (~line 607) to `top_row: 42`.

- [ ] **Step 5: Set `core.fold_view` whenever folds/text change**

Add a helper near `run_commands`:

```rust
fn sync_fold_view(edit: &mut EditState) {
    let total = edit.core.buffer.len_lines() as u32;
    edit.core.fold_view = edit.folds.view(total);
}
```
Call it right after the `EditState` is constructed (after the insert in Step 4 is not possible — do it where mutable; instead initialize `core.fold_view` before constructing): set `core.fold_view = folds.view(core.buffer.len_lines() as u32);` just before the `commands.entity(...).insert(...)`.

- [ ] **Step 6: Build**

Run: `cargo build -p vmux_editor`
Expected: remaining errors only in `run_commands` (fold handling, Task 8) and `page.rs` (Task 12).

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs crates/vmux_core/src/event.rs
git commit -m "feat(editor): row-space windowing + cursor/selection rows"
```

---

## Task 8: Plugin — fold command handling + reveal + edit remap

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1: Handle fold commands in `run_commands`**

Inside the `for cmd in cmds` loop, before the `EditCommand::Save` block, add a fold arm that mutates `edit.folds`, re-syncs the view, and forces a re-render via `text_changed`:

```rust
        match cmd {
            EditCommand::FoldToggle | EditCommand::FoldOpen | EditCommand::FoldClose
            | EditCommand::FoldToggleRecursive | EditCommand::FoldAll | EditCommand::UnfoldAll => {
                let line = edit.core.cursor_pos().line;
                match cmd {
                    EditCommand::FoldToggle => edit.folds.toggle(line),
                    EditCommand::FoldOpen => edit.folds.open(line),
                    EditCommand::FoldClose => edit.folds.close(line),
                    EditCommand::FoldToggleRecursive => edit.folds.toggle_recursive(line),
                    EditCommand::FoldAll => edit.folds.fold_all(),
                    EditCommand::UnfoldAll => edit.folds.unfold_all(),
                    _ => unreachable!(),
                }
                sync_fold_view(edit);
                fold_changed = true;
                continue;
            }
            _ => {}
        }
```

Declare `let mut fold_changed = false;` alongside `text_changed` at the top, and treat it like `text_changed` for re-emission (window + cursor) and for triggering a persistence save (Task 9). At the end:

```rust
    if text_changed || fold_changed {
        emit_window(entity, edit, &vpc, browsers, commands);
    }
    if text_changed || sel_or_mode || fold_changed {
        emit_cursor(entity, edit, keymap, &vpc, browsers, commands);
    }
    if fold_changed {
        commands.entity(entity).insert(FoldsDirty);
    }
```

Add a marker component near the others:

```rust
#[derive(Component)]
struct FoldsDirty;
```

- [ ] **Step 2: After edits, remap folds + recompute regions + reveal caret**

After the loop, where `out.text_changed` was accumulated, when `text_changed` is true recompute indentation regions and keep collapses stable, then reveal the caret line if hidden. Insert before the autoscroll block:

```rust
    if text_changed {
        let regions = crate::fold::indent_regions(&edit.core.buffer.rope);
        edit.folds.set_regions(regions);
        sync_fold_view(edit);
    }
    {
        let total = edit.core.buffer.len_lines() as u32;
        let caret_line = edit.core.cursor_pos().line;
        if edit.folds.view(total).is_hidden(caret_line) {
            edit.folds.reveal(caret_line);
            sync_fold_view(edit);
            fold_changed = true;
        }
    }
```

(The simple recompute drops collapses on structural edits where a header line moves — acceptable per spec. A precise `shift` by the edit delta is a future refinement; `set_regions` + `reconcile` already prevents stale collapses.)

Change `autoscroll` to row space:

```rust
    if let Some(top) = edit.core.autoscroll_rows(vp.top_row, vp.rows, &edit.folds) {
        vp.top_row = top;
        text_changed = true;
    }
```

Add `autoscroll_rows` to `core.rs` (next step).

- [ ] **Step 3: Add `autoscroll_rows` to `EditCore`**

In `core.rs`:

```rust
    pub fn autoscroll_rows(
        &self,
        top: u32,
        rows: u16,
        folds: &crate::fold::FoldState,
    ) -> Option<u32> {
        if rows == 0 {
            return None;
        }
        let total = self.buffer.len_lines() as u32;
        let (line, _) = self.buffer.char_to_coords(self.primary().head);
        let row = folds.view(total).buffer_to_row(line as u32);
        if row < top {
            Some(row)
        } else if row >= top + rows as u32 {
            Some(row + 1 - rows as u32)
        } else {
            None
        }
    }
```

- [ ] **Step 4: Build + test**

Run: `cargo build -p vmux_editor && cargo test -p vmux_editor edit::core`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs crates/vmux_editor/src/edit/core.rs
git commit -m "feat(editor): fold command handling, reveal, row autoscroll"
```

---

## Task 9: Plugin — gutter-click event + persistence

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1: Add the `FoldStore` resource and load it**

Insert as a non-send resource in `build` beside `ClipboardHandle`:

```rust
        app.insert_non_send(ClipboardHandle(arboard::Clipboard::new().ok()))
            .insert_non_send(SelfWrites::default())
            .insert_non_send(crate::fold_store::FoldStore::load())
```

- [ ] **Step 2: Seed persisted collapses at load**

In `load_file_buffers`, after building regions, apply the store. Add `mut store: NonSendMut<crate::fold_store::FoldStore>` to the system params, then:

```rust
        let mut folds = crate::fold::FoldState::default();
        folds.set_regions(crate::fold::indent_regions(&core.buffer.rope));
        for s in store.get(&fv.path) {
            folds.close(s);
        }
        core.fold_view = folds.view(core.buffer.len_lines() as u32);
```

- [ ] **Step 3: Persist on `FoldsDirty`**

Add a system that drains `FoldsDirty` and writes the store:

```rust
fn persist_folds(
    q: Query<(Entity, &FileView, &EditState), With<FoldsDirty>>,
    mut store: NonSendMut<crate::fold_store::FoldStore>,
    mut commands: Commands,
) {
    let mut changed = false;
    for (entity, fv, edit) in q.iter() {
        let mut collapsed: Vec<u32> = edit.folds.collapsed.iter().copied().collect();
        collapsed.sort_unstable();
        store.set(&fv.path, &collapsed);
        commands.entity(entity).remove::<FoldsDirty>();
        changed = true;
    }
    if changed {
        store.save();
    }
}
```

Register it in the `Update` systems tuple.

- [ ] **Step 4: Add the gutter-click observer**

```rust
fn on_file_fold_toggle(
    trigger: On<BinReceive<FileFoldToggle>>,
    mut q: Query<(&mut EditState, &EditorKeymap, &mut FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let line = trigger.event().payload.line;
    let Ok((mut edit, keymap, mut vp)) = q.get_mut(entity) else {
        return;
    };
    edit.folds.toggle(line);
    sync_fold_view(&mut edit);
    let vpc = *vp;
    let _ = &mut vp;
    emit_window(entity, &mut edit, &vpc, &browsers, &mut commands);
    emit_cursor(entity, &edit, keymap.0.as_ref(), &vpc, &browsers, &mut commands);
    commands.entity(entity).insert(FoldsDirty);
}
```

- [ ] **Step 5: Register the event + observer**

Add `FileFoldToggle` to one of the `BinEventEmitterPlugin::<(...)>` tuples, and `.add_observer(on_file_fold_toggle)` to the observer chain.

- [ ] **Step 6: Build**

Run: `cargo build -p vmux_editor`
Expected: PASS (page.rs still pending — if page.rs blocks the build, proceed to Task 12 before this compiles fully; build `--lib` may still surface page errors. Run `cargo check -p vmux_editor --lib` to scope.)

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs
git commit -m "feat(editor): gutter-click toggle + fold persistence"
```

---

## Task 10: Vim fold keys

**Files:**
- Modify: `crates/vmux_editor/src/keymap/vim.rs`

- [ ] **Step 1: Add `z_pending` state**

Add to the `VimKeymap` struct (beside `g_pending`):

```rust
    z_pending: bool,
```
Reset it in `reset()` (beside `self.g_pending = false;`):

```rust
        self.z_pending = false;
```

- [ ] **Step 2: Handle the `z` prefix**

Near the `g_pending` block (~line 90), add an analogous block:

```rust
        if self.z_pending {
            self.z_pending = false;
            use EditCommand::*;
            return match key {
                "a" => vec![FoldToggle],
                "o" => vec![FoldOpen],
                "c" => vec![FoldClose],
                "A" => vec![FoldToggleRecursive],
                "R" => vec![UnfoldAll],
                "M" => vec![FoldAll],
                _ => vec![],
            };
        }
```

And in the normal-mode `match key` add (beside `"g" => { self.g_pending = true; vec![] }`):

```rust
            "z" => {
                self.z_pending = true;
                vec![]
            }
```

- [ ] **Step 3: Test**

Append to vim keymap tests:

```rust
    #[test]
    fn za_toggles_fold() {
        let mut km = VimKeymap::default();
        assert!(km.handle(&KeyInput { key: "z".into(), mods: Mods::default(), repeat: false }).is_empty());
        let cmds = km.handle(&KeyInput { key: "a".into(), mods: Mods::default(), repeat: false });
        assert_eq!(cmds, vec![EditCommand::FoldToggle]);
    }
```

Run: `cargo test -p vmux_editor keymap::vim`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_editor/src/keymap/vim.rs
git commit -m "feat(editor): vim fold keys (za/zo/zc/zA/zR/zM)"
```

---

## Task 11: VSCode fold shortcuts

**Files:**
- Modify: `crates/vmux_editor/src/keymap/vscode.rs`

- [ ] **Step 1: Inspect the existing shortcut dispatch**

Read `vscode.rs` to find where `mods.cmd()` shortcuts map to commands (e.g. save/undo). Add fold binds in the same place:

```rust
        if k.mods.cmd() && k.mods.shift {
            match key {
                "[" | "{" => return vec![EditCommand::FoldClose],
                "]" | "}" => return vec![EditCommand::FoldOpen],
                "0" | ")" => return vec![EditCommand::FoldAll],
                "j" | "J" => return vec![EditCommand::UnfoldAll],
                _ => {}
            }
        }
```

(`Cmd+Shift+[`/`]` may arrive as `{`/`}` depending on layout — accept both. `Cmd+Shift+0` may arrive as `)`.)

- [ ] **Step 2: Test**

```rust
    #[test]
    fn cmd_shift_bracket_folds() {
        let mut km = VscodeKeymap;
        let cmds = km.handle(&KeyInput {
            key: "[".into(),
            mods: Mods { meta: true, shift: true, ..Default::default() },
            repeat: false,
        });
        assert_eq!(cmds, vec![EditCommand::FoldClose]);
    }
```

Run: `cargo test -p vmux_editor keymap::vscode`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/keymap/vscode.rs
git commit -m "feat(editor): vscode fold shortcuts"
```

---

## Task 12: Frontend — render by visual row + chevrons

**Files:**
- Modify: `crates/vmux_editor/src/page.rs`

- [ ] **Step 1: Rename the row signal + patch fields**

The `FileViewportPatch` listener (~line 303) now sets `first_row` and a new `total_rows` signal. Rename the `first_line` signal to `first_row`; add `total_rows`. Spacer height (~line 774) uses `total_rows()` not `total_lines()`:

```rust
    let _vp = use_bin_event_listener::<FileViewportPatch, _>(FILE_VIEWPORT_EVENT, move |p| {
        first_row.set(p.first_row);
        total_rows.set(p.total_rows);
        total_lines.set(p.total_lines);
        lines.set(p.lines);
        lsp_hover.set(None);
    });
```
```rust
    let spacer = total_rows() as f64 * ch;
```

- [ ] **Step 2: Position lines by visual row, not buffer line**

In the line loop (~line 804), enumerate and position by `first_row + i`. Gutter shows `line.line_no + 1`. Replace the per-line `let ln = line.line_no; let lt = ln as f64 * ch;` with:

```rust
                        for (i, line) in lines().iter().enumerate() {
                            {
                                let ln = line.line_no;
                                let row = first_row() + i as u32;
                                let lt = row as f64 * ch;
                                let fold = line.fold;
```
(`ln` stays the buffer line for click→pointer mapping + the gutter number; `row` drives `top:{lt}px`.)

- [ ] **Step 3: Render the chevron + collapsed placeholder**

In the gutter span (~line 907), before `"{ln + 1}"`, add a chevron when foldable. Click toggles via `FileFoldToggle`:

```rust
                                        match fold {
                                            FoldGutter::Open => rsx! {
                                                span {
                                                    class: "cursor-pointer opacity-0 group-hover:opacity-70 hover:opacity-100",
                                                    onmousedown: move |e: Event<MouseData>| {
                                                        e.stop_propagation();
                                                        e.prevent_default();
                                                        let _ = try_cef_bin_emit_rkyv(&FileFoldToggle { line: ln });
                                                    },
                                                    "▾"
                                                }
                                            },
                                            FoldGutter::Collapsed => rsx! {
                                                span {
                                                    class: "cursor-pointer opacity-80 hover:opacity-100",
                                                    onmousedown: move |e: Event<MouseData>| {
                                                        e.stop_propagation();
                                                        e.prevent_default();
                                                        let _ = try_cef_bin_emit_rkyv(&FileFoldToggle { line: ln });
                                                    },
                                                    "▸"
                                                }
                                            },
                                            FoldGutter::None => rsx! {},
                                        }
```

After the text-span content (~line 938, after the spans loop, before closing the text span), add the placeholder when collapsed:

```rust
                                            if fold == FoldGutter::Collapsed {
                                                span { class: "ml-1 rounded bg-white/10 px-1 text-foreground/40", "⋯" }
                                            }
```

Add `FoldGutter` and `FileFoldToggle` to the `vmux_core::event` import at the top of `page.rs`.

- [ ] **Step 4: Position caret + selection by row**

Cursor `cy` (~line 773) uses `cursor().row`:

```rust
                            let cy = cursor().row as f64 * ch;
```
Selection loop (~line 946) uses `s.row`:

```rust
                                let top = s.row as f64 * ch;
```

- [ ] **Step 5: Scroll handler emits `top_row`**

In `onscroll` (~line 800), the field is now `top_row`:

```rust
                                            let _ = try_cef_bin_emit_rkyv(&FileScrollEvent { top_row: vis_first });
```

- [ ] **Step 6: Build wasm + native lib**

Run: `cargo check -p vmux_editor --target wasm32-unknown-unknown` (typechecks the page) and `cargo build -p vmux_editor`
Expected: PASS.

- [ ] **Step 7: Run page source-scrape tests**

Run: `cargo test -p vmux_editor`
Expected: PASS. If `tests/page_source.rs` or `style.rs` `include_str!` assertions fail due to the gutter markup change, update those expected snippets.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_editor/src/page.rs
git commit -m "feat(editor): render folds — visual rows, gutter chevrons, placeholder"
```

---

## Task 13: Plugin message-integration test

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs` (tests)

- [ ] **Step 1: Add an end-to-end fold test via the public flow**

Per AGENTS.md, drive the production flow: construct an `EditCore` + `FoldState`, send a fold command through `run_commands`-equivalent state, and assert the emitted window hides the body. Since `run_commands` needs `Browsers`, test the engine seam instead: assert `emit_window`'s line selection. Add to a `#[cfg(test)] mod fold_window_tests`:

```rust
#[cfg(test)]
mod fold_window_tests {
    use crate::fold::{indent_regions, FoldState};
    use ropey::Rope;

    #[test]
    fn collapsed_region_hidden_from_window() {
        let r = Rope::from_str("fn a() {\n    x;\n    y;\n}\nz;\n");
        let mut folds = FoldState::default();
        folds.set_regions(indent_regions(&r));
        folds.close(0);
        let view = folds.view(r.len_lines() as u32);
        let visible = view.lines_for_window(0, view.visible_count());
        assert!(visible.contains(&0));
        assert!(!visible.contains(&1) && !visible.contains(&2));
        assert!(visible.contains(&3));
    }
}
```

- [ ] **Step 2: Run + full crate test**

Run: `cargo test -p vmux_editor`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs
git commit -m "test(editor): fold window integration"
```

---

# Milestone 2 — LSP folding ranges

## Task 14: Request + apply `textDocument/foldingRange`

**Files:**
- Modify: `crates/vmux_editor/src/lsp/manager.rs`
- Modify: `crates/vmux_editor/src/plugin.rs`
- Modify: `crates/vmux_editor/src/bin/vmux_mock_lsp.rs`

- [ ] **Step 1: Inspect how an existing request (e.g. hover/definition) is issued + its response message**

Read `manager.rs` for the `definition`/`hover` request methods and the `LspGoto`-style result message + the system that applies it (`apply_goto`). Fold ranges follow the same shape: a `foldingRange(entity, path)` request and an `LspFolds { entity, path, regions: Vec<FoldRegion> }` message applied by a new system.

- [ ] **Step 2: Add the request method**

In `manager.rs`, mirror `definition` but send method `"textDocument/foldingRange"` with params `{ "textDocument": { "uri": <file-uri> } }`. On response, parse the array; each item has `startLine` / `endLine` (0-based, end inclusive of the folded body):

```rust
pub fn parse_folding_ranges(value: &serde_json::Value) -> Vec<crate::fold::FoldRegion> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    let s = r.get("startLine")?.as_u64()? as u32;
                    let e = r.get("endLine")?.as_u64()? as u32;
                    (e > s).then_some(crate::fold::FoldRegion { start: s, end: e })
                })
                .collect()
        })
        .unwrap_or_default()
}
```

Emit an `LspFolds { entity, path, regions }` message when the response arrives (register the message type in the LSP plugin like the other `Lsp*` results).

- [ ] **Step 3: Apply folds, preserving collapsed state**

New system in `plugin.rs`:

```rust
fn apply_lsp_folds(
    mut msgs: MessageReader<crate::lsp::manager::LspFolds>,
    mut q: Query<(&mut EditState, &FileView, &EditorKeymap, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for f in msgs.read() {
        for (mut edit, fv, keymap, vp) in q.iter_mut() {
            if canon(&fv.path) != canon(&f.path) {
                continue;
            }
            edit.folds.set_regions(f.regions.clone());
            sync_fold_view(&mut edit);
            let vpc = *vp;
            emit_window(f.entity, &mut edit, &vpc, &browsers, &mut commands);
            emit_cursor(f.entity, &edit, keymap.0.as_ref(), &vpc, &browsers, &mut commands);
        }
    }
}
```

Register it in the `Update` systems tuple. `set_regions` + `reconcile` keeps collapses whose start lines still begin a region.

- [ ] **Step 4: Request folds on open + after change**

Where the editor first connects/opens a file for LSP (find the existing open/`didOpen` path), also issue `manager.folding_range(entity, &path)`. Where `flush_lsp_changes` / `didChange` fires, re-request (debounced like the existing change flush).

- [ ] **Step 5: Mock LSP support for tests**

In `vmux_mock_lsp.rs`, advertise `foldingRangeProvider: true` in capabilities and answer `textDocument/foldingRange` with a fixed range (e.g. `[{ "startLine": 0, "endLine": 2 }]`) so an integration test can assert regions arrive.

- [ ] **Step 6: Test the parser**

```rust
    #[test]
    fn parses_folding_ranges() {
        let v = serde_json::json!([{ "startLine": 0, "endLine": 3 }, { "startLine": 1, "endLine": 1 }]);
        let regs = parse_folding_ranges(&v);
        assert_eq!(regs, vec![crate::fold::FoldRegion { start: 0, end: 3 }]);
    }
```

Run: `cargo test -p vmux_editor parses_folding_ranges`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_editor/src/lsp/manager.rs crates/vmux_editor/src/plugin.rs crates/vmux_editor/src/bin/vmux_mock_lsp.rs
git commit -m "feat(editor): LSP foldingRange source"
```

---

## Final verification

- [ ] **Workspace fmt + clippy + tests**

Run: `cargo fmt --all && cargo clippy -p vmux_editor -p vmux_core --all-targets && cargo test -p vmux_editor -p vmux_core`
Expected: clean. (Per the `cargo fmt patches` note: if fmt touches `patches/`, `git checkout -- patches/` and commit only `crates/` changes.)

- [ ] **Runtime pass (user-driven):** open a code file, verify gutter chevrons fold/unfold, `za`/`zR`/`zM` (vim) and `Cmd+Shift+[`/`]` (vscode) work, cursor `j`/`k` skips folded bodies, goto-definition into a fold reveals it, and folds survive reopen + app restart.

- [ ] **Delete this plan file** once fully implemented (per AGENTS.md), and open the PR.

---

## Self-Review notes

- **Spec coverage:** LSP+indent source (Tasks 3, 14) ✓; gutter+keyboard (Tasks 9–12) ✓; persistence across restarts (Tasks 4, 9) ✓; visual-row model + dumb frontend (Tasks 7, 12) ✓; nested folds (`toggle_recursive`, `view` merge) ✓; jump-into-fold reveal (Task 8) ✓; wire types (Task 1) ✓; tests (every task + 13) ✓.
- **Type consistency:** `FoldGutter` (`None`/`Open`/`Collapsed`), `FoldRegion{start,end}`, `FoldState`, `FoldView`, `FileViewportPatch{first_row,total_rows,total_lines}`, `FileScrollEvent{top_row}`, `FileFoldToggle{line}`, `CursorPos.row`/`SelSpan.row`, `EditState.folds`, `EditCore.fold_view`, `sync_fold_view`, `autoscroll_rows`, `FoldsDirty`, `persist_folds`, `on_file_fold_toggle`, `LspFolds`, `parse_folding_ranges`, `indent_regions` — used consistently across tasks.
- **Known simplifications (per spec, acceptable v1):** edit-time region recompute uses `set_regions`+`reconcile` (collapses may drop on structural edits) rather than precise `shift`; large collapsed regions are still highlighted then filtered (syntect state is sequential anyway).
```
