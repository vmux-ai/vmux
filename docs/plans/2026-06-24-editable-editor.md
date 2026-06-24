# Editable Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task (NOT subagent-driven — vmux CEF builds are huge and long agents drop sockets; implement directly with a warm target dir). Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make the `file://` editor editable with a native-authoritative rope core driven by two interchangeable keymaps (Vim + VSCode), IME input, selections, clipboard, undo/redo, and save.

**Architecture:** `EditCore` (rope + cursor + mode + undo) is the single source of truth per `FileView` entity. Both keymaps translate keys into one shared `EditCommand` vocabulary the core applies. The WASM page hosts a hidden `<textarea>` for IME + keystrokes and renders caret/selection over the existing virtualized window; a native→page mode hint routes keys as text vs command.

**Tech Stack:** Rust, Bevy ECS, `ropey`, `arboard`, `syntect`/`two-face`, Dioxus (WASM), `rkyv` events over CEF bin.

**Spec:** `docs/specs/2026-06-24-editable-editor-design.md`

**Base:** branch `feat/editable-editor` off `main` (post LSP merge #157).

---

## File Structure

Native (`#[cfg(not(target_arch = "wasm32"))]`), new:
- `crates/vmux_editor/src/edit.rs` — module re-exports.
- `crates/vmux_editor/src/edit/command.rs` — `EditCommand`, `Motion`, `EditMode`, `Selection`, `CursorPos`, `SelSpan`.
- `crates/vmux_editor/src/edit/buffer.rs` — `TextBuffer` (ropey wrapper + grapheme/word helpers).
- `crates/vmux_editor/src/edit/core.rs` — `EditCore`, `EditOutcome`, undo stack, clipboard register.
- `crates/vmux_editor/src/edit/highlight_cache.rs` — incremental syntect.
- `crates/vmux_editor/src/keymap.rs` — `Keymap` trait, `KeymapKind`, `KeyInput`, `Mods`, factory.
- `crates/vmux_editor/src/keymap/vim.rs` — `VimKeymap`.
- `crates/vmux_editor/src/keymap/vscode.rs` — `VscodeKeymap`.

Native, modified:
- `crates/vmux_editor/src/lib.rs` — add module decls + re-exports.
- `crates/vmux_editor/src/highlight.rs` — extract `select_syntax`/`select_theme` helpers for reuse.
- `crates/vmux_editor/src/plugin.rs` — build `EditCore`/cache/keymap on load; observers `on_file_key`/`on_file_text_input`/`on_file_pointer`; save; external-conflict; LSP debounce; emit cursor/dirty.
- `crates/vmux_editor/Cargo.toml` — add `ropey`, `arboard` (native), web-sys features (wasm).
- `crates/vmux_core/src/event.rs` — new event structs + `FILE_*_EVENT` consts.
- `crates/vmux_setting/src/plugin/runtime.rs` — `EditorSettings { keymap: KeymapKind }` on `AppSettings`.

WASM (`#[cfg(target_arch = "wasm32")]`), modified:
- `crates/vmux_editor/src/page.rs` — hidden textarea input controller, caret/selection render, pointer, mode badge, dirty dot.

---

## Milestone M1 — Edit core (pure Rust, no Bevy/wasm)

### Task 1: Command vocabulary + Selection types

**Files:**
- Create: `crates/vmux_editor/src/edit/command.rs`
- Create: `crates/vmux_editor/src/edit.rs`
- Modify: `crates/vmux_editor/src/lib.rs`

- [ ] **Step 1: Add module decls to lib.rs**

In `crates/vmux_editor/src/lib.rs`, under the existing `#[cfg(not(target_arch = "wasm32"))]` block (next to `pub mod highlight;`):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod edit;
#[cfg(not(target_arch = "wasm32"))]
pub mod keymap;
```

- [ ] **Step 2: Create edit.rs re-exports**

```rust
pub mod buffer;
pub mod command;
pub mod core;
pub mod highlight_cache;

pub use command::{CursorPos, EditCommand, EditMode, Motion, SelSpan, Selection};
pub use core::{EditCore, EditOutcome};
```

- [ ] **Step 3: Write command.rs with tests**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMode {
    Normal,
    Insert,
    Visual,
    VisualLine,
}

impl EditMode {
    pub fn label(self) -> &'static str {
        match self {
            EditMode::Normal => "NORMAL",
            EditMode::Insert => "INSERT",
            EditMode::Visual => "VISUAL",
            EditMode::VisualLine => "V-LINE",
        }
    }
    pub fn is_visual(self) -> bool {
        matches!(self, EditMode::Visual | EditMode::VisualLine)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordNext,
    WordPrev,
    WordEnd,
    LineStart,
    FirstNonBlank,
    LineEnd,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
    GotoLine(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditCommand {
    Move(Motion),
    Select(Motion),
    InsertText(String),
    InsertNewline,
    InsertTab,
    DeleteBack,
    DeleteForward,
    DeleteWordBack,
    DeleteToLineEnd,
    DeleteRange(Motion),
    YankRange(Motion),
    DeleteSelection,
    DeleteLine,
    Yank,
    Cut,
    Paste,
    PasteBefore,
    SetMode(EditMode),
    Undo,
    Redo,
    Save,
}

/// Selection in absolute char offsets. Caret == empty selection (anchor == head).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(at: usize) -> Self {
        Self { anchor: at, head: at }
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
    pub fn range(&self) -> std::ops::Range<usize> {
        if self.anchor <= self.head {
            self.anchor..self.head
        } else {
            self.head..self.anchor
        }
    }
}

/// Caret position sent to the page: line + visual column (display width).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CursorPos {
    pub line: u32,
    pub col: u32,
}

/// A selection span on a single visible line, in visual columns. `end == u32::MAX`
/// means "to end of line" (line-spanning selection tail).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelSpan {
    pub line: u32,
    pub start: u32,
    pub end: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_range_normalizes_direction() {
        assert_eq!(Selection { anchor: 2, head: 5 }.range(), 2..5);
        assert_eq!(Selection { anchor: 5, head: 2 }.range(), 2..5);
    }

    #[test]
    fn caret_is_empty() {
        assert!(Selection::caret(3).is_empty());
        assert!(!Selection { anchor: 1, head: 2 }.is_empty());
    }

    #[test]
    fn mode_labels() {
        assert_eq!(EditMode::Normal.label(), "NORMAL");
        assert!(EditMode::VisualLine.is_visual());
        assert!(!EditMode::Insert.is_visual());
    }
}
```

- [ ] **Step 4: Add deps to Cargo.toml**

In `crates/vmux_editor/Cargo.toml`, under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`:

```toml
ropey = "1"
arboard = "3"
unicode-width = "0.2"
unicode-segmentation = "1"
```

- [ ] **Step 5: Run tests, expect PASS**

Run: `cargo test -p vmux_editor edit::command`
Expected: 3 passed. (Compiles the new modules — `core.rs`, `buffer.rs`, `highlight_cache.rs` must at least exist as empty stubs; create them empty now so `edit.rs` compiles, fill in later tasks. Create `crates/vmux_editor/src/edit/buffer.rs`, `core.rs`, `highlight_cache.rs` with a single `// stub` line, and `crates/vmux_editor/src/keymap.rs` with `// stub` so lib.rs compiles.)

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_editor/src/edit.rs crates/vmux_editor/src/edit/ crates/vmux_editor/src/keymap.rs crates/vmux_editor/src/lib.rs crates/vmux_editor/Cargo.toml
git commit -m "feat(editor): edit command vocabulary + selection types"
```

### Task 2: TextBuffer (ropey wrapper)

**Files:**
- Modify: `crates/vmux_editor/src/edit/buffer.rs`

- [ ] **Step 1: Write buffer.rs with tests**

```rust
use std::path::PathBuf;

use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

pub struct TextBuffer {
    pub rope: Rope,
    pub path: PathBuf,
    pub language: String,
}

impl TextBuffer {
    pub fn from_text(path: PathBuf, language: String, text: &str) -> Self {
        Self { rope: Rope::from_str(text), path, language }
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        // ropey counts a trailing newline as starting a new (empty) line; clamp to >=1.
        self.rope.len_lines().max(1)
    }

    pub fn char_to_line(&self, char_idx: usize) -> usize {
        self.rope.char_to_line(char_idx.min(self.len_chars()))
    }

    pub fn line_to_char(&self, line: usize) -> usize {
        let line = line.min(self.len_lines().saturating_sub(1));
        self.rope.line_to_char(line)
    }

    /// Char length of a line excluding its trailing newline.
    pub fn line_len_chars(&self, line: usize) -> usize {
        if line >= self.len_lines() {
            return 0;
        }
        let slice = self.rope.line(line);
        let n = slice.len_chars();
        // strip trailing \n and \r\n
        let mut n = n;
        let s = slice;
        if n > 0 && s.char(n - 1) == '\n' {
            n -= 1;
            if n > 0 && s.char(n - 1) == '\r' {
                n -= 1;
            }
        }
        n
    }

    /// (line, col-in-chars) for an absolute char offset.
    pub fn char_to_coords(&self, char_idx: usize) -> (usize, usize) {
        let char_idx = char_idx.min(self.len_chars());
        let line = self.char_to_line(char_idx);
        let col = char_idx - self.rope.line_to_char(line);
        (line, col)
    }

    pub fn coords_to_char(&self, line: usize, col: usize) -> usize {
        let line = line.min(self.len_lines().saturating_sub(1));
        let base = self.rope.line_to_char(line);
        base + col.min(self.line_len_chars(line))
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.rope.insert(char_idx.min(self.len_chars()), text);
    }

    pub fn remove(&mut self, range: std::ops::Range<usize>) {
        let end = range.end.min(self.len_chars());
        let start = range.start.min(end);
        self.rope.remove(start..end);
    }

    pub fn to_string(&self) -> String {
        self.rope.to_string()
    }

    /// Next grapheme boundary at or after `char_idx` (for cursor Right).
    pub fn next_grapheme(&self, char_idx: usize) -> usize {
        let line = self.char_to_line(char_idx);
        let line_start = self.rope.line_to_char(line);
        let line_str: String = self.rope.line(line).chars().collect();
        let off = char_idx - line_start;
        let mut iter = line_str.grapheme_indices(true).map(|(i, g)| {
            (line_str[..i].chars().count(), g.chars().count())
        });
        for (gstart, glen) in iter.by_ref() {
            if gstart >= off {
                return char_idx + (gstart - off);
            }
            if gstart + glen > off {
                return line_start + gstart + glen;
            }
        }
        (line_start + line_str.chars().count()).min(self.len_chars())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(text: &str) -> TextBuffer {
        TextBuffer::from_text(PathBuf::from("a.txt"), "Plain Text".into(), text)
    }

    #[test]
    fn line_len_excludes_newline() {
        let b = buf("ab\ncde\n");
        assert_eq!(b.line_len_chars(0), 2);
        assert_eq!(b.line_len_chars(1), 3);
    }

    #[test]
    fn coords_roundtrip() {
        let b = buf("ab\ncde\n");
        assert_eq!(b.char_to_coords(4), (1, 1));
        assert_eq!(b.coords_to_char(1, 1), 4);
    }

    #[test]
    fn coords_to_char_clamps_col() {
        let b = buf("ab\ncde\n");
        assert_eq!(b.coords_to_char(0, 99), 2);
    }

    #[test]
    fn insert_remove() {
        let mut b = buf("ac");
        b.insert(1, "b");
        assert_eq!(b.to_string(), "abc");
        b.remove(1..2);
        assert_eq!(b.to_string(), "ac");
    }
}
```

- [ ] **Step 2: Run tests, expect PASS**

Run: `cargo test -p vmux_editor edit::buffer`
Expected: 4 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/edit/buffer.rs
git commit -m "feat(editor): TextBuffer rope wrapper"
```

### Task 3: EditCore — mode, insert, delete, motion, selection

**Files:**
- Modify: `crates/vmux_editor/src/edit/core.rs`

- [ ] **Step 1: Write core.rs (state + apply for non-undo/non-clipboard commands) with tests**

```rust
use std::path::PathBuf;

use unicode_width::UnicodeWidthStr;

use crate::edit::buffer::TextBuffer;
use crate::edit::command::{CursorPos, EditCommand, EditMode, Motion, SelSpan, Selection};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Insert,
    Delete,
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditOutcome {
    pub text_changed: bool,
    pub sel_changed: bool,
    pub mode_changed: bool,
    pub dirty_changed: bool,
    pub scroll_to: Option<u32>,
    /// Text that should be pushed to the system clipboard (Yank/Cut).
    pub yank: Option<(String, bool)>,
}

pub struct EditCore {
    pub buffer: TextBuffer,
    pub selections: Vec<Selection>,
    pub mode: EditMode,
    pub rows: u16,
    pub dirty: bool,
    /// In-memory register. Plugin mirrors this to/from arboard. (text, linewise)
    pub register: Option<(String, bool)>,
    undo: Vec<(ropey::Rope, Vec<Selection>)>,
    redo: Vec<(ropey::Rope, Vec<Selection>)>,
    last_group: Option<Group>,
}

impl EditCore {
    pub fn new(path: PathBuf, language: String, text: &str, default_mode: EditMode) -> Self {
        Self {
            buffer: TextBuffer::from_text(path, language, text),
            selections: vec![Selection::caret(0)],
            mode: default_mode,
            rows: 0,
            dirty: false,
            register: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_group: None,
        }
    }

    pub fn primary(&self) -> Selection {
        self.selections[0]
    }
    fn set_caret(&mut self, at: usize) {
        self.selections = vec![Selection::caret(at)];
    }
    fn set_head(&mut self, head: usize) {
        let anchor = self.selections[0].anchor;
        self.selections = vec![Selection { anchor, head }];
    }

    /// Primary caret as (line, visual-col) for the page.
    pub fn cursor_pos(&self) -> CursorPos {
        let head = self.primary().head;
        let (line, col) = self.buffer.char_to_coords(head);
        let line_start = self.buffer.line_to_char(line);
        let prefix: String = self.buffer.rope.slice(line_start..line_start + col).chars().collect();
        CursorPos { line: line as u32, col: UnicodeWidthStr::width(prefix.as_str()) as u32 }
    }

    /// Selection spans for the visible window [first, first+rows), in visual columns.
    pub fn sel_spans(&self, first: u32, rows: u16) -> Vec<SelSpan> {
        let sel = self.primary();
        if sel.is_empty() {
            return Vec::new();
        }
        let r = sel.range();
        let (l0, _) = self.buffer.char_to_coords(r.start);
        let (l1, _) = self.buffer.char_to_coords(r.end);
        let mut out = Vec::new();
        let end_line = (first as usize + rows as usize).min(self.buffer.len_lines());
        for line in (first as usize).max(l0)..end_line.min(l1 + 1) {
            let ls = self.buffer.line_to_char(line);
            let llen = self.buffer.line_len_chars(line);
            let sc = if line == l0 { r.start - ls } else { 0 };
            let ec = if line == l1 { r.end - ls } else { llen };
            let vis = |c: usize| {
                let s: String = self.buffer.rope.slice(ls..ls + c).chars().collect();
                UnicodeWidthStr::width(s.as_str()) as u32
            };
            let end = if line < l1 { u32::MAX } else { vis(ec) };
            out.push(SelSpan { line: line as u32, start: vis(sc), end });
        }
        out
    }

    fn break_group(&mut self) {
        self.last_group = None;
    }
    fn snapshot(&mut self) {
        self.undo.push((self.buffer.rope.clone(), self.selections.clone()));
        self.redo.clear();
    }
    fn checkpoint(&mut self, group: Group) {
        if self.last_group != Some(group) || group == Group::Other {
            self.snapshot();
        }
        self.last_group = Some(group);
        self.dirty = true;
    }

    fn resolve_motion(&self, from: usize, motion: Motion) -> usize {
        let len = self.buffer.len_chars();
        match motion {
            Motion::Left => from.saturating_sub(1),
            Motion::Right => self.buffer.next_grapheme(from).min(len),
            Motion::Up => self.vertical(from, -1),
            Motion::Down => self.vertical(from, 1),
            Motion::PageUp => self.vertical(from, -(self.rows.max(1) as i64)),
            Motion::PageDown => self.vertical(from, self.rows.max(1) as i64),
            Motion::LineStart => {
                let (l, _) = self.buffer.char_to_coords(from);
                self.buffer.line_to_char(l)
            }
            Motion::FirstNonBlank => self.first_non_blank(from),
            Motion::LineEnd => {
                let (l, _) = self.buffer.char_to_coords(from);
                self.buffer.line_to_char(l) + self.buffer.line_len_chars(l)
            }
            Motion::DocStart => 0,
            Motion::DocEnd => len,
            Motion::GotoLine(n) => self.buffer.line_to_char(n as usize),
            Motion::WordNext => self.word_next(from),
            Motion::WordPrev => self.word_prev(from),
            Motion::WordEnd => self.word_end(from),
        }
    }

    fn vertical(&self, from: usize, delta: i64) -> usize {
        let (l, c) = self.buffer.char_to_coords(from);
        let target = (l as i64 + delta).max(0) as usize;
        self.buffer.coords_to_char(target, c)
    }
    fn first_non_blank(&self, from: usize) -> usize {
        let (l, _) = self.buffer.char_to_coords(from);
        let base = self.buffer.line_to_char(l);
        let llen = self.buffer.line_len_chars(l);
        for i in 0..llen {
            let ch = self.buffer.rope.char(base + i);
            if ch != ' ' && ch != '\t' {
                return base + i;
            }
        }
        base
    }

    fn class(c: char) -> u8 {
        if c.is_whitespace() {
            0
        } else if c.is_alphanumeric() || c == '_' {
            1
        } else {
            2
        }
    }
    fn word_next(&self, from: usize) -> usize {
        let len = self.buffer.len_chars();
        let mut i = from;
        if i >= len {
            return len;
        }
        let start_class = Self::class(self.buffer.rope.char(i));
        while i < len && Self::class(self.buffer.rope.char(i)) == start_class && start_class != 0 {
            i += 1;
        }
        while i < len && Self::class(self.buffer.rope.char(i)) == 0 {
            i += 1;
        }
        i
    }
    fn word_prev(&self, from: usize) -> usize {
        let mut i = from;
        while i > 0 && Self::class(self.buffer.rope.char(i - 1)) == 0 {
            i -= 1;
        }
        if i == 0 {
            return 0;
        }
        let cls = Self::class(self.buffer.rope.char(i - 1));
        while i > 0 && Self::class(self.buffer.rope.char(i - 1)) == cls {
            i -= 1;
        }
        i
    }
    fn word_end(&self, from: usize) -> usize {
        let len = self.buffer.len_chars();
        let mut i = (from + 1).min(len);
        while i < len && Self::class(self.buffer.rope.char(i)) == 0 {
            i += 1;
        }
        if i >= len {
            return len;
        }
        let cls = Self::class(self.buffer.rope.char(i));
        while i + 1 < len && Self::class(self.buffer.rope.char(i + 1)) == cls {
            i += 1;
        }
        i + 1
    }

    fn insert_text(&mut self, text: &str) -> bool {
        if self.mode.is_visual() {
            self.delete_selection();
        }
        self.checkpoint(Group::Insert);
        let at = self.primary().head;
        self.buffer.insert(at, text);
        self.set_caret(at + text.chars().count());
        true
    }
    fn delete_selection(&mut self) -> bool {
        let sel = self.primary();
        if sel.is_empty() {
            return false;
        }
        self.checkpoint(Group::Other);
        let r = sel.range();
        self.buffer.remove(r.clone());
        self.set_caret(r.start);
        true
    }

    pub fn apply(&mut self, cmd: EditCommand) -> EditOutcome {
        let before_head = self.primary();
        let before_mode = self.mode;
        let before_dirty = self.dirty;
        let mut text_changed = false;

        match cmd {
            EditCommand::Move(m) => {
                self.break_group();
                if self.mode.is_visual() {
                    let h = self.resolve_motion(self.primary().head, m);
                    self.set_head(h);
                } else {
                    let h = self.resolve_motion(self.primary().head, m);
                    self.set_caret(h);
                }
            }
            EditCommand::Select(m) => {
                self.break_group();
                let h = self.resolve_motion(self.primary().head, m);
                self.set_head(h);
            }
            EditCommand::InsertText(t) => text_changed = self.insert_text(&t),
            EditCommand::InsertTab => text_changed = self.insert_text("\t"),
            EditCommand::InsertNewline => text_changed = self.insert_text("\n"),
            EditCommand::DeleteBack => {
                if self.primary().is_empty() {
                    let head = self.primary().head;
                    if head > 0 {
                        self.checkpoint(Group::Delete);
                        let prev = head - 1;
                        self.buffer.remove(prev..head);
                        self.set_caret(prev);
                        text_changed = true;
                    }
                } else {
                    text_changed = self.delete_selection();
                }
            }
            EditCommand::DeleteForward => {
                let head = self.primary().head;
                if head < self.buffer.len_chars() {
                    self.checkpoint(Group::Delete);
                    self.buffer.remove(head..head + 1);
                    text_changed = true;
                }
            }
            EditCommand::DeleteWordBack => {
                let head = self.primary().head;
                let target = self.word_prev(head);
                if target < head {
                    self.checkpoint(Group::Delete);
                    self.buffer.remove(target..head);
                    self.set_caret(target);
                    text_changed = true;
                }
            }
            EditCommand::DeleteToLineEnd => {
                let head = self.primary().head;
                let end = self.resolve_motion(head, Motion::LineEnd);
                if end > head {
                    self.checkpoint(Group::Other);
                    self.buffer.remove(head..end);
                    text_changed = true;
                }
            }
            EditCommand::DeleteRange(m) => {
                let head = self.primary().head;
                let target = self.resolve_motion(head, m);
                let (a, b) = (head.min(target), head.max(target));
                if b > a {
                    self.checkpoint(Group::Other);
                    self.buffer.remove(a..b);
                    self.set_caret(a);
                    text_changed = true;
                }
            }
            EditCommand::DeleteSelection => text_changed = self.delete_selection(),
            EditCommand::DeleteLine => {
                let (l, _) = self.buffer.char_to_coords(self.primary().head);
                let start = self.buffer.line_to_char(l);
                let end = if l + 1 < self.buffer.len_lines() {
                    self.buffer.line_to_char(l + 1)
                } else {
                    self.buffer.len_chars()
                };
                if end > start {
                    self.checkpoint(Group::Other);
                    self.buffer.remove(start..end);
                    self.set_caret(start.min(self.buffer.len_chars()));
                    text_changed = true;
                }
            }
            EditCommand::SetMode(m) => {
                self.break_group();
                if m == EditMode::Normal && !self.primary().is_empty() {
                    self.set_caret(self.primary().head);
                }
                self.mode = m;
            }
            // Undo/Redo/clipboard implemented in Task 4.
            EditCommand::Undo
            | EditCommand::Redo
            | EditCommand::Yank
            | EditCommand::Cut
            | EditCommand::Paste
            | EditCommand::PasteBefore
            | EditCommand::Save => {}
        }

        EditOutcome {
            text_changed,
            sel_changed: self.primary() != before_head,
            mode_changed: self.mode != before_mode,
            dirty_changed: self.dirty != before_dirty,
            scroll_to: None,
            yank: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(text: &str) -> EditCore {
        EditCore::new(PathBuf::from("a.txt"), "Plain Text".into(), text, EditMode::Insert)
    }
    fn text_of(c: &EditCore) -> String {
        c.buffer.to_string()
    }

    #[test]
    fn insert_text_moves_caret() {
        let mut c = core("");
        c.apply(EditCommand::InsertText("hi".into()));
        assert_eq!(text_of(&c), "hi");
        assert_eq!(c.primary().head, 2);
        assert!(c.dirty);
    }

    #[test]
    fn backspace_deletes_prev_char() {
        let mut c = core("ab");
        c.set_caret(2);
        c.apply(EditCommand::DeleteBack);
        assert_eq!(text_of(&c), "a");
    }

    #[test]
    fn word_next_motion() {
        let mut c = core("foo bar");
        c.set_caret(0);
        c.apply(EditCommand::Move(Motion::WordNext));
        assert_eq!(c.primary().head, 4);
    }

    #[test]
    fn visual_select_then_delete() {
        let mut c = core("abcdef");
        c.set_caret(1);
        c.mode = EditMode::Visual;
        c.apply(EditCommand::Select(Motion::Right));
        c.apply(EditCommand::Select(Motion::Right));
        c.apply(EditCommand::DeleteSelection);
        assert_eq!(text_of(&c), "adef");
    }

    #[test]
    fn delete_range_word() {
        let mut c = core("foo bar");
        c.set_caret(0);
        c.apply(EditCommand::DeleteRange(Motion::WordNext));
        assert_eq!(text_of(&c), "bar");
    }

    #[test]
    fn cursor_pos_visual_col_for_wide_chars() {
        let mut c = core("あb");
        c.set_caret(1); // after the wide char
        assert_eq!(c.cursor_pos(), CursorPos { line: 0, col: 2 });
    }
}
```

- [ ] **Step 2: Run tests, expect PASS**

Run: `cargo test -p vmux_editor edit::core`
Expected: 6 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_editor/src/edit/core.rs
git commit -m "feat(editor): EditCore motions, insert, delete, selection"
```

### Task 4: EditCore — undo/redo, clipboard, autoscroll

**Files:**
- Modify: `crates/vmux_editor/src/edit/core.rs`

- [ ] **Step 1: Replace the `Undo | Redo | ... => {}` arm with real implementations**

Replace that match arm with:

```rust
            EditCommand::Undo => {
                if let Some((rope, sel)) = self.undo.pop() {
                    self.redo.push((self.buffer.rope.clone(), self.selections.clone()));
                    self.buffer.rope = rope;
                    self.selections = sel;
                    self.break_group();
                    self.dirty = true;
                    text_changed = true;
                }
            }
            EditCommand::Redo => {
                if let Some((rope, sel)) = self.redo.pop() {
                    self.undo.push((self.buffer.rope.clone(), self.selections.clone()));
                    self.buffer.rope = rope;
                    self.selections = sel;
                    self.break_group();
                    self.dirty = true;
                    text_changed = true;
                }
            }
            EditCommand::Yank => {
                let sel = self.primary();
                if !sel.is_empty() {
                    let s: String = self.buffer.rope.slice(sel.range()).chars().collect();
                    self.register = Some((s.clone(), false));
                    yank = Some((s, false));
                    if self.mode.is_visual() {
                        self.set_caret(sel.range().start);
                        self.mode = EditMode::Normal;
                    }
                }
            }
            EditCommand::Cut => {
                if !self.primary().is_empty() {
                    let r = self.primary().range();
                    let s: String = self.buffer.rope.slice(r.clone()).chars().collect();
                    self.register = Some((s.clone(), false));
                    yank = Some((s, false));
                    self.checkpoint(Group::Other);
                    self.buffer.remove(r.clone());
                    self.set_caret(r.start);
                    if self.mode.is_visual() {
                        self.mode = EditMode::Normal;
                    }
                    text_changed = true;
                }
            }
            EditCommand::Paste | EditCommand::PasteBefore => {
                if let Some((s, _linewise)) = self.register.clone() {
                    if self.mode.is_visual() {
                        self.delete_selection();
                        self.mode = EditMode::Normal;
                    }
                    self.checkpoint(Group::Other);
                    let at = self.primary().head;
                    self.buffer.insert(at, &s);
                    self.set_caret(at + s.chars().count());
                    text_changed = true;
                }
            }
            EditCommand::Save => {}
```

- [ ] **Step 2: Add `let mut yank = None;` near the top of `apply` and wire it into the returned `EditOutcome`**

At the start of `apply`, after `let mut text_changed = false;` add:

```rust
        let mut yank: Option<(String, bool)> = None;
```

Change the returned struct's `yank: None,` to `yank,` and compute `scroll_to`:

```rust
        let scroll_to = self.autoscroll(before_first_for_outcome);
```

For v1 keep autoscroll caller-driven: add a method and let the plugin call it (simpler than threading viewport here). Replace the `scroll_to: None,` line — leave it `scroll_to: None` in `apply`, and add this separate method:

```rust
    /// Given the page's current top line + rows, return a new top line that keeps
    /// the primary caret visible, or None if no scroll needed.
    pub fn autoscroll(&self, top: u32, rows: u16) -> Option<u32> {
        if rows == 0 {
            return None;
        }
        let (line, _) = self.buffer.char_to_coords(self.primary().head);
        let line = line as u32;
        if line < top {
            Some(line)
        } else if line >= top + rows as u32 {
            Some(line + 1 - rows as u32)
        } else {
            None
        }
    }
```

(Delete the `before_first_for_outcome` line from Step 1 — it was illustrative; `apply` returns `scroll_to: None` and the plugin computes scroll via `autoscroll` after applying.)

- [ ] **Step 3: Add tests**

```rust
    #[test]
    fn undo_redo_roundtrip() {
        let mut c = core("");
        c.apply(EditCommand::InsertText("abc".into()));
        c.apply(EditCommand::SetMode(EditMode::Normal)); // break group
        c.apply(EditCommand::InsertText("X".into()));
        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "abc");
        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "");
        c.apply(EditCommand::Redo);
        assert_eq!(text_of(&c), "abc");
    }

    #[test]
    fn typing_run_is_one_undo() {
        let mut c = core("");
        c.apply(EditCommand::InsertText("h".into()));
        c.apply(EditCommand::InsertText("i".into()));
        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "");
    }

    #[test]
    fn yank_and_paste() {
        let mut c = core("abcdef");
        c.set_caret(0);
        c.mode = EditMode::Visual;
        c.apply(EditCommand::Select(Motion::Right));
        c.apply(EditCommand::Select(Motion::Right));
        let out = c.apply(EditCommand::Yank);
        assert_eq!(out.yank, Some(("ab".to_string(), false)));
        c.mode = EditMode::Insert;
        c.set_caret(6);
        c.apply(EditCommand::Paste);
        assert_eq!(text_of(&c), "abcdefab");
    }

    #[test]
    fn autoscroll_follows_caret_down() {
        let mut c = core("a\nb\nc\nd\ne\nf\n");
        c.rows = 3;
        c.set_caret(c.buffer.coords_to_char(5, 0));
        assert_eq!(c.autoscroll(0, 3), Some(4));
    }
```

- [ ] **Step 4: Run tests, expect PASS**

Run: `cargo test -p vmux_editor edit::core`
Expected: 10 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/edit/core.rs
git commit -m "feat(editor): EditCore undo/redo, clipboard register, autoscroll"
```

---

## Milestone M2 — Keymaps (pure Rust)

### Task 5: Keymap trait + VscodeKeymap + factory

**Files:**
- Modify: `crates/vmux_editor/src/keymap.rs`
- Create: `crates/vmux_editor/src/keymap/vscode.rs`

- [ ] **Step 1: Write keymap.rs (trait, types, factory, KeymapKind)**

```rust
pub mod vim;
pub mod vscode;

use serde::{Deserialize, Serialize};

use crate::edit::command::{EditCommand, EditMode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Mods {
    pub fn none(&self) -> bool {
        !self.ctrl && !self.alt && !self.meta
    }
    /// Primary command modifier: Cmd on macOS, Ctrl elsewhere. Page sets `meta`
    /// from `event.metaKey`; we accept either for portability.
    pub fn cmd(&self) -> bool {
        self.meta || self.ctrl
    }
    pub fn word(&self) -> bool {
        self.alt || self.ctrl
    }
}

#[derive(Clone, Debug)]
pub struct KeyInput {
    pub key: String,
    pub mods: Mods,
    pub repeat: bool,
}

pub trait Keymap: Send {
    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand>;
    fn mode(&self) -> EditMode;
    fn mode_label(&self) -> String {
        self.mode().label().to_string()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeymapKind {
    #[default]
    Vscode,
    Vim,
}

impl KeymapKind {
    pub fn make(self) -> Box<dyn Keymap> {
        match self {
            KeymapKind::Vscode => Box::new(vscode::VscodeKeymap::default()),
            KeymapKind::Vim => Box::new(vim::VimKeymap::default()),
        }
    }
    /// Mode the EditCore should start in for this keymap.
    pub fn initial_mode(self) -> EditMode {
        match self {
            KeymapKind::Vscode => EditMode::Insert,
            KeymapKind::Vim => EditMode::Normal,
        }
    }
}
```

- [ ] **Step 2: Write vscode.rs with tests**

```rust
use crate::edit::command::{EditCommand, EditMode, Motion};
use crate::keymap::{KeyInput, Keymap};

#[derive(Default)]
pub struct VscodeKeymap;

impl Keymap for VscodeKeymap {
    fn mode(&self) -> EditMode {
        EditMode::Insert
    }
    fn mode_label(&self) -> String {
        String::new()
    }

    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        use EditCommand::*;
        let m = &k.mods;
        let sel = m.shift;
        let mv = |motion: Motion| if sel { vec![Select(motion)] } else { vec![Move(motion)] };

        // Command-modifier chords first.
        if m.cmd() && !m.alt {
            return match k.key.to_ascii_lowercase().as_str() {
                "c" => vec![Yank],
                "x" => vec![Cut],
                "v" => vec![Paste],
                "a" => vec![Move(Motion::DocStart), Select(Motion::DocEnd)],
                "s" => vec![Save],
                "z" if m.shift => vec![Redo],
                "z" => vec![Undo],
                "y" => vec![Redo],
                _ => vec![],
            };
        }

        match k.key.as_str() {
            "ArrowLeft" if m.word() => {
                if sel { vec![Select(Motion::WordPrev)] } else { vec![Move(Motion::WordPrev)] }
            }
            "ArrowRight" if m.word() => {
                if sel { vec![Select(Motion::WordNext)] } else { vec![Move(Motion::WordNext)] }
            }
            "ArrowLeft" => mv(Motion::Left),
            "ArrowRight" => mv(Motion::Right),
            "ArrowUp" => mv(Motion::Up),
            "ArrowDown" => mv(Motion::Down),
            "Home" => mv(Motion::LineStart),
            "End" => mv(Motion::LineEnd),
            "PageUp" => mv(Motion::PageUp),
            "PageDown" => mv(Motion::PageDown),
            "Backspace" if m.word() => vec![DeleteWordBack],
            "Backspace" => vec![DeleteBack],
            "Delete" => vec![DeleteForward],
            "Enter" => vec![InsertNewline],
            "Tab" => vec![InsertTab],
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Mods;

    fn key(k: &str, mods: Mods) -> KeyInput {
        KeyInput { key: k.into(), mods, repeat: false }
    }

    #[test]
    fn arrow_moves_shift_selects() {
        let mut km = VscodeKeymap;
        assert_eq!(km.handle(&key("ArrowRight", Mods::default())), vec![EditCommand::Move(Motion::Right)]);
        let shift = Mods { shift: true, ..Default::default() };
        assert_eq!(km.handle(&key("ArrowRight", shift)), vec![EditCommand::Select(Motion::Right)]);
    }

    #[test]
    fn cmd_chords() {
        let mut km = VscodeKeymap;
        let cmd = Mods { meta: true, ..Default::default() };
        assert_eq!(km.handle(&key("c", cmd)), vec![EditCommand::Yank]);
        assert_eq!(km.handle(&key("s", cmd)), vec![EditCommand::Save]);
        let cmd_shift = Mods { meta: true, shift: true, ..Default::default() };
        assert_eq!(km.handle(&key("z", cmd_shift)), vec![EditCommand::Redo]);
    }

    #[test]
    fn select_all_composes() {
        let mut km = VscodeKeymap;
        let cmd = Mods { meta: true, ..Default::default() };
        assert_eq!(
            km.handle(&key("a", cmd)),
            vec![EditCommand::Move(Motion::DocStart), EditCommand::Select(Motion::DocEnd)]
        );
    }

    #[test]
    fn word_backspace() {
        let mut km = VscodeKeymap;
        let alt = Mods { alt: true, ..Default::default() };
        assert_eq!(km.handle(&key("Backspace", alt)), vec![EditCommand::DeleteWordBack]);
    }
}
```

- [ ] **Step 3: Run tests, expect PASS**

Run: `cargo test -p vmux_editor keymap::vscode`
Expected: 4 passed. (Create `crates/vmux_editor/src/keymap/vim.rs` with a stub `#[derive(Default)] pub struct VimKeymap;` + a trivial `Keymap` impl returning `vec![]`/`EditMode::Normal` so the module compiles; Task 6 fills it.)

- [ ] **Step 4: Add `serde` to non-wasm deps if missing**

`crates/vmux_editor/Cargo.toml` already has `serde = { workspace = true }` at the top-level `[dependencies]`; no change needed. Verify `KeymapKind` compiles under the native target.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/keymap.rs crates/vmux_editor/src/keymap/
git commit -m "feat(editor): Keymap trait + VscodeKeymap + KeymapKind factory"
```

### Task 6: VimKeymap (modal)

**Files:**
- Modify: `crates/vmux_editor/src/keymap/vim.rs` (replace the stub)

- [ ] **Step 1: Write vim.rs with tests**

```rust
use crate::edit::command::{EditCommand, EditMode, Motion};
use crate::keymap::{KeyInput, Keymap};

#[derive(Default)]
pub struct VimKeymap {
    mode: EditMode,
    count: Option<usize>,
    pending_op: Option<char>,
    g_pending: bool,
    ex: Option<String>,
}

impl Default for EditMode {
    fn default() -> Self {
        EditMode::Normal
    }
}

fn rep(cmd: EditCommand, n: usize) -> Vec<EditCommand> {
    std::iter::repeat(cmd).take(n.max(1)).collect()
}

/// Motion-producing normal/visual keys (excluding count-sensitive G/gg).
fn motion_for(key: &str) -> Option<Motion> {
    Some(match key {
        "h" => Motion::Left,
        "l" => Motion::Right,
        "j" => Motion::Down,
        "k" => Motion::Up,
        "w" => Motion::WordNext,
        "b" => Motion::WordPrev,
        "e" => Motion::WordEnd,
        "0" => Motion::LineStart,
        "^" => Motion::FirstNonBlank,
        "$" => Motion::LineEnd,
        _ => return None,
    })
}

impl VimKeymap {
    fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1)
    }
    fn reset(&mut self) {
        self.count = None;
        self.pending_op = None;
        self.g_pending = false;
    }

    fn normal(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        use EditCommand::*;
        let key = k.key.as_str();

        // Count accumulation ('0' only counts when a count is already started).
        if key.len() == 1 {
            let c = key.chars().next().unwrap();
            if c.is_ascii_digit() && !(c == '0' && self.count.is_none()) {
                let d = c as usize - '0' as usize;
                self.count = Some(self.count.unwrap_or(0) * 10 + d);
                return vec![];
            }
        }

        // Operator-pending resolution.
        if let Some(op) = self.pending_op {
            let n = self.take_count();
            self.pending_op = None;
            if key.len() == 1 && key.chars().next() == Some(op) {
                // doubled: linewise
                return match op {
                    'd' => vec![DeleteLine],
                    'y' => vec![Move(Motion::LineStart), Select(Motion::LineEnd), Yank],
                    'c' => {
                        self.mode = EditMode::Insert;
                        vec![Move(Motion::LineStart), DeleteToLineEnd, SetMode(EditMode::Insert)]
                    }
                    _ => vec![],
                };
            }
            if let Some(m) = motion_for(key) {
                let _ = n; // v1: operators apply once
                return match op {
                    'd' => vec![DeleteRange(m)],
                    'y' => vec![YankRange(m)],
                    'c' => {
                        self.mode = EditMode::Insert;
                        vec![DeleteRange(m), SetMode(EditMode::Insert)]
                    }
                    _ => vec![],
                };
            }
            return vec![]; // unknown motion cancels operator
        }

        // g-prefix.
        if self.g_pending {
            self.g_pending = false;
            if key == "g" {
                self.count = None;
                return vec![Move(Motion::DocStart)];
            }
            return vec![];
        }

        if key == "r" && k.mods.ctrl {
            let n = self.take_count();
            return rep(Redo, n);
        }

        if let Some(m) = motion_for(key) {
            let n = self.take_count();
            return rep(Move(m), n);
        }

        match key {
            "g" => {
                self.g_pending = true;
                vec![]
            }
            "G" => {
                let cmd = match self.count.take() {
                    Some(n) => Move(Motion::GotoLine(n.saturating_sub(1) as u32)),
                    None => Move(Motion::DocEnd),
                };
                vec![cmd]
            }
            "i" => {
                self.mode = EditMode::Insert;
                vec![SetMode(EditMode::Insert)]
            }
            "a" => {
                self.mode = EditMode::Insert;
                vec![Move(Motion::Right), SetMode(EditMode::Insert)]
            }
            "I" => {
                self.mode = EditMode::Insert;
                vec![Move(Motion::FirstNonBlank), SetMode(EditMode::Insert)]
            }
            "A" => {
                self.mode = EditMode::Insert;
                vec![Move(Motion::LineEnd), SetMode(EditMode::Insert)]
            }
            "o" => {
                self.mode = EditMode::Insert;
                vec![Move(Motion::LineEnd), InsertNewline, SetMode(EditMode::Insert)]
            }
            "O" => {
                self.mode = EditMode::Insert;
                vec![Move(Motion::LineStart), InsertNewline, Move(Motion::Up), SetMode(EditMode::Insert)]
            }
            "x" => {
                let n = self.take_count();
                rep(DeleteForward, n)
            }
            "p" => vec![Paste],
            "P" => vec![PasteBefore],
            "u" => {
                let n = self.take_count();
                rep(Undo, n)
            }
            "d" | "c" | "y" => {
                self.pending_op = key.chars().next();
                vec![]
            }
            "v" => {
                self.mode = EditMode::Visual;
                vec![SetMode(EditMode::Visual)]
            }
            "V" => {
                self.mode = EditMode::VisualLine;
                vec![SetMode(EditMode::VisualLine)]
            }
            ":" => {
                self.ex = Some(String::new());
                vec![]
            }
            "Escape" => {
                self.reset();
                vec![]
            }
            _ => vec![],
        }
    }

    fn visual(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        use EditCommand::*;
        let key = k.key.as_str();
        if let Some(m) = motion_for(key) {
            return vec![Select(m)];
        }
        match key {
            "d" | "x" => {
                self.mode = EditMode::Normal;
                vec![DeleteSelection, SetMode(EditMode::Normal)]
            }
            "c" => {
                self.mode = EditMode::Insert;
                vec![DeleteSelection, SetMode(EditMode::Insert)]
            }
            "y" => {
                self.mode = EditMode::Normal;
                vec![Yank]
            }
            "v" | "V" | "Escape" => {
                self.mode = EditMode::Normal;
                vec![SetMode(EditMode::Normal)]
            }
            _ => vec![],
        }
    }

    fn insert(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        use EditCommand::*;
        match k.key.as_str() {
            "Escape" => {
                self.mode = EditMode::Normal;
                vec![Move(Motion::Left), SetMode(EditMode::Normal)]
            }
            "Backspace" => vec![DeleteBack],
            "Delete" => vec![DeleteForward],
            "Enter" => vec![InsertNewline],
            "Tab" => vec![InsertTab],
            "ArrowLeft" => vec![Move(Motion::Left)],
            "ArrowRight" => vec![Move(Motion::Right)],
            "ArrowUp" => vec![Move(Motion::Up)],
            "ArrowDown" => vec![Move(Motion::Down)],
            _ => vec![],
        }
    }

    fn ex_key(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        let buf = self.ex.as_mut().unwrap();
        match k.key.as_str() {
            "Enter" => {
                let cmd = self.ex.take().unwrap();
                match cmd.as_str() {
                    "w" | "wq" | "x" => vec![EditCommand::Save],
                    _ => vec![],
                }
            }
            "Escape" => {
                self.ex = None;
                vec![]
            }
            "Backspace" => {
                buf.pop();
                vec![]
            }
            key if key.len() == 1 => {
                buf.push_str(key);
                vec![]
            }
            _ => vec![],
        }
    }
}

impl Keymap for VimKeymap {
    fn mode(&self) -> EditMode {
        self.mode
    }
    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        if self.ex.is_some() {
            return self.ex_key(k);
        }
        match self.mode {
            EditMode::Insert => self.insert(k),
            EditMode::Visual | EditMode::VisualLine => self.visual(k),
            EditMode::Normal => self.normal(k),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Mods;

    fn k(key: &str) -> KeyInput {
        KeyInput { key: key.into(), mods: Mods::default(), repeat: false }
    }
    fn ctrl(key: &str) -> KeyInput {
        KeyInput { key: key.into(), mods: Mods { ctrl: true, ..Default::default() }, repeat: false }
    }

    #[test]
    fn dw_deletes_word() {
        let mut km = VimKeymap::default();
        assert_eq!(km.handle(&k("d")), vec![]);
        assert_eq!(km.handle(&k("w")), vec![EditCommand::DeleteRange(Motion::WordNext)]);
    }

    #[test]
    fn dd_deletes_line() {
        let mut km = VimKeymap::default();
        km.handle(&k("d"));
        assert_eq!(km.handle(&k("d")), vec![EditCommand::DeleteLine]);
    }

    #[test]
    fn count_repeats_motion() {
        let mut km = VimKeymap::default();
        assert_eq!(km.handle(&k("3")), vec![]);
        assert_eq!(
            km.handle(&k("j")),
            vec![EditCommand::Move(Motion::Down), EditCommand::Move(Motion::Down), EditCommand::Move(Motion::Down)]
        );
    }

    #[test]
    fn i_enters_insert() {
        let mut km = VimKeymap::default();
        assert_eq!(km.handle(&k("i")), vec![EditCommand::SetMode(EditMode::Insert)]);
        assert_eq!(km.mode(), EditMode::Insert);
    }

    #[test]
    fn esc_in_insert_returns_normal_and_steps_left() {
        let mut km = VimKeymap::default();
        km.handle(&k("i"));
        assert_eq!(
            km.handle(&k("Escape")),
            vec![EditCommand::Move(Motion::Left), EditCommand::SetMode(EditMode::Normal)]
        );
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn visual_select_and_yank() {
        let mut km = VimKeymap::default();
        assert_eq!(km.handle(&k("v")), vec![EditCommand::SetMode(EditMode::Visual)]);
        assert_eq!(km.handle(&k("l")), vec![EditCommand::Select(Motion::Right)]);
        assert_eq!(km.handle(&k("y")), vec![EditCommand::Yank]);
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn o_opens_line_below() {
        let mut km = VimKeymap::default();
        assert_eq!(
            km.handle(&k("o")),
            vec![
                EditCommand::Move(Motion::LineEnd),
                EditCommand::InsertNewline,
                EditCommand::SetMode(EditMode::Insert)
            ]
        );
    }

    #[test]
    fn ctrl_r_redo() {
        let mut km = VimKeymap::default();
        assert_eq!(km.handle(&ctrl("r")), vec![EditCommand::Redo]);
    }

    #[test]
    fn ex_write_saves() {
        let mut km = VimKeymap::default();
        km.handle(&k(":"));
        km.handle(&k("w"));
        assert_eq!(km.handle(&k("Enter")), vec![EditCommand::Save]);
    }
}
```

Note: the `impl Default for EditMode` block belongs in `edit/command.rs` (move it there to avoid an orphan-rule split across files). Put `#[derive(Default)]`-incompatible default on `EditMode` by adding `#[default] Normal` to the enum in command.rs instead, and remove the `impl Default for EditMode` shown above. Update command.rs:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EditMode {
    #[default]
    Normal,
    Insert,
    Visual,
    VisualLine,
}
```

- [ ] **Step 2: Run tests, expect PASS**

Run: `cargo test -p vmux_editor keymap::vim`
Expected: 9 passed.

- [ ] **Step 3: Run the whole pure-Rust core suite**

Run: `cargo test -p vmux_editor edit:: keymap::`
Expected: all green (M1 + M2 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_editor/src/keymap/vim.rs crates/vmux_editor/src/edit/command.rs
git commit -m "feat(editor): VimKeymap modal bindings"
```

### Task 7: EditorSettings (keymap selection)

**Files:**
- Modify: `crates/vmux_setting/src/plugin/runtime.rs`
- Modify: `crates/vmux_setting/Cargo.toml` (depend on `vmux_editor` for `KeymapKind`, OR define `KeymapKind` in `vmux_core` to avoid a cycle)

- [ ] **Step 1: Decide the type home to avoid a dependency cycle**

`vmux_editor` already depends on `vmux_setting`, so `vmux_setting` cannot depend on `vmux_editor` (cycle). Per the no-new-crates rule, define `KeymapKind` in `vmux_core` (which both depend on) and re-export it from `keymap.rs`.

Move the `KeymapKind` enum from Task 5 into `crates/vmux_core/src/lib.rs` (or a new `crates/vmux_core/src/editor.rs` module + `pub use`). Keep the `make()`/`initial_mode()` impls in `vmux_editor/src/keymap.rs` via:

```rust
pub use vmux_core::KeymapKind;
```

and an extension impl in keymap.rs:

```rust
impl KeymapKind {
    pub fn make(self) -> Box<dyn Keymap> { /* as Task 5 */ }
    pub fn initial_mode(self) -> EditMode { /* as Task 5 */ }
}
```

(Inherent impls on a re-exported type are fine since `vmux_editor` owns this impl block in its own crate only if `KeymapKind` is local — it is not. So instead define a `KeymapKindExt` trait in keymap.rs with `make`/`initial_mode` and `impl KeymapKindExt for KeymapKind`.)

`vmux_core` enum:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeymapKind {
    #[default]
    Vscode,
    Vim,
}
```

- [ ] **Step 2: Add EditorSettings to AppSettings**

In `crates/vmux_setting/src/plugin/runtime.rs`, beside `TerminalSettings`:

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EditorSettings {
    #[serde(default)]
    pub keymap: vmux_core::KeymapKind,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self { keymap: vmux_core::KeymapKind::default() }
    }
}
```

Add to `AppSettings` (mirror how `terminal: Option<TerminalSettings>` is declared):

```rust
    #[serde(default)]
    pub editor: Option<EditorSettings>,
```

- [ ] **Step 3: Test default resolves to Vscode when absent**

Add to `runtime.rs` tests (or a new test):

```rust
#[test]
fn editor_keymap_defaults_to_vscode_when_absent() {
    let s: AppSettings = ron::from_str("(version: 1)").unwrap_or_default();
    let kind = s.editor.map(|e| e.keymap).unwrap_or_default();
    assert_eq!(kind, vmux_core::KeymapKind::Vscode);
}
```

(Adjust the RON literal to a minimal valid `AppSettings` for the repo; the assertion is the point — absent `editor` → `Vscode`.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p vmux_setting editor_keymap`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/ crates/vmux_setting/src/plugin/runtime.rs crates/vmux_editor/src/keymap.rs
git commit -m "feat(settings): editor.keymap selection (default vscode)"
```

---

## Milestone M3 — Incremental highlight cache

### Task 8: HighlightCache (resumable syntect)

**Files:**
- Modify: `crates/vmux_editor/src/highlight.rs` (extract reusable helpers)
- Modify: `crates/vmux_editor/src/edit/highlight_cache.rs`

- [ ] **Step 1: Expose helpers from highlight.rs**

In `crates/vmux_editor/src/highlight.rs`, make the syntax set accessor and span builder reusable, and add syntax/theme selectors. Add:

```rust
pub fn syntax_set() -> &'static SyntaxSet {
    syntaxes()
}

pub fn select_syntax(path: &Path) -> &'static syntect::parsing::SyntaxReference {
    let ss = syntaxes();
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .unwrap_or_else(|| ss.find_syntax_plain_text())
}

pub fn default_theme() -> syntect::highlighting::Theme {
    ThemeSet::load_defaults().themes["base16-ocean.dark"].clone()
}

pub(crate) fn styled_span(style: Style, text: &str) -> StyledSpan {
    to_styled_span(style, text)
}
```

(`to_styled_span` already exists; just expose a `pub(crate)` wrapper. Keep `Highlighter` as-is for the dir/preview/initial-load paths.)

- [ ] **Step 2: Write highlight_cache.rs with tests**

```rust
use ropey::Rope;
use syntect::highlighting::{Highlighter, HighlightIterator, HighlightState, Theme};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference};
use vmux_core::event::{FileLine, StyledSpan};

use crate::highlight::{default_theme, select_syntax, styled_span, syntax_set};

/// Resumable per-line syntect state so edits only re-highlight from the edit
/// point, and only the visible window is materialised into spans.
pub struct HighlightCache {
    syntax: &'static SyntaxReference,
    theme: Theme,
    /// befores[i] = parser/highlight state *before* line i. befores[0] is initial.
    befores: Vec<(ParseState, HighlightState)>,
    pub language: String,
}

impl HighlightCache {
    pub fn new(path: &std::path::Path) -> Self {
        let syntax = select_syntax(path);
        Self {
            language: syntax.name.clone(),
            syntax,
            theme: default_theme(),
            befores: Vec::new(),
        }
    }

    fn initial(&self) -> (ParseState, HighlightState) {
        let hl = Highlighter::new(&self.theme);
        (ParseState::new(self.syntax), HighlightState::new(&hl, ScopeStack::new()))
    }

    pub fn invalidate_from(&mut self, line: usize) {
        // Keep states before unchanged lines (indices 0..=line).
        self.befores.truncate(line + 1);
    }

    fn ensure_before(&mut self, rope: &Rope, line: usize) {
        if self.befores.is_empty() {
            self.befores.push(self.initial());
        }
        let ss = syntax_set();
        let hl = Highlighter::new(&self.theme);
        let total = rope.len_lines();
        while self.befores.len() <= line && self.befores.len() - 1 < total {
            let i = self.befores.len() - 1;
            let (mut ps, mut hs) = self.befores[i].clone();
            let text: String = rope.line(i).chars().collect();
            let ops = ps.parse_line(&text, ss).unwrap_or_default();
            {
                let mut it = HighlightIterator::new(&mut hs, &ops, &text, &hl);
                for _ in it.by_ref() {}
            }
            self.befores.push((ps, hs));
        }
    }

    /// Highlighted FileLines for [start, end).
    pub fn line_window(&mut self, rope: &Rope, start: usize, end: usize) -> Vec<FileLine> {
        let total = rope.len_lines();
        let end = end.min(total);
        if start >= end {
            return Vec::new();
        }
        self.ensure_before(rope, end - 1);
        let ss = syntax_set();
        let hl = Highlighter::new(&self.theme);
        let mut out = Vec::with_capacity(end - start);
        for i in start..end {
            let (mut ps, mut hs) = self.befores[i].clone();
            let text: String = rope.line(i).chars().collect();
            let ops = ps.parse_line(&text, ss).unwrap_or_default();
            let spans: Vec<StyledSpan> = HighlightIterator::new(&mut hs, &ops, &text, &hl)
                .map(|(style, t)| styled_span(style, t))
                .filter(|s| !s.text.is_empty())
                .collect();
            out.push(FileLine { line_no: i as u32, spans });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(s: &str) -> Rope {
        Rope::from_str(s)
    }

    #[test]
    fn window_line_numbers_and_text() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let r = rope("fn a() {}\nlet x = 1;\nstruct S;\n");
        let w = c.line_window(&r, 1, 3);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].line_no, 1);
        let joined: String = w[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined.trim_end(), "let x = 1;");
    }

    #[test]
    fn invalidate_recomputes_tail() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let mut r = rope("let a = 1;\nlet b = 2;\nlet c = 3;\n");
        let _ = c.line_window(&r, 0, 3);
        // Edit line 0, invalidate from 0, re-highlight: line 2 text reflects buffer.
        r.insert(0, "// ");
        c.invalidate_from(0);
        let w = c.line_window(&r, 2, 3);
        let joined: String = w[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined.trim_end(), "let c = 3;");
    }

    #[test]
    fn multicolor_for_code() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let r = rope("fn main() {}\n");
        let w = c.line_window(&r, 0, 1);
        let colors: std::collections::HashSet<_> = w[0].spans.iter().map(|s| s.fg).collect();
        assert!(colors.len() > 1);
    }
}
```

- [ ] **Step 3: Run tests, expect PASS**

Run: `cargo test -p vmux_editor highlight_cache`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_editor/src/highlight.rs crates/vmux_editor/src/edit/highlight_cache.rs
git commit -m "feat(editor): resumable incremental highlight cache"
```

---

## Milestone M4 — Wire protocol + plugin wiring

### Task 9: Shared editor types + new events in vmux_core

**Files:**
- Create: `crates/vmux_core/src/editor.rs`
- Modify: `crates/vmux_core/src/lib.rs` (add `pub mod editor; pub use editor::*;`)
- Modify: `crates/vmux_core/src/event.rs`
- Modify: `crates/vmux_editor/src/edit/command.rs` (re-export shared types)
- Modify: `crates/vmux_editor/src/keymap.rs` (re-export `KeymapKind` from core)

- [ ] **Step 1: Relocate shared types to vmux_core (so the wasm page can use them)**

`crates/vmux_core/src/editor.rs` (use the same derive set as existing `event.rs` types so they cross the rkyv wire):

```rust
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default,
    Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub enum EditMode {
    #[default]
    Normal,
    Insert,
    Visual,
    VisualLine,
}

impl EditMode {
    pub fn label(self) -> &'static str {
        match self {
            EditMode::Normal => "NORMAL",
            EditMode::Insert => "INSERT",
            EditMode::Visual => "VISUAL",
            EditMode::VisualLine => "V-LINE",
        }
    }
    pub fn is_visual(self) -> bool {
        matches!(self, EditMode::Visual | EditMode::VisualLine)
    }
    /// Whether a plain printable key should be treated as text (vs a command).
    pub fn accepts_text(self) -> bool {
        matches!(self, EditMode::Insert)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default,
    Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct CursorPos {
    pub line: u32,
    pub col: u32,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq,
    Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct SelSpan {
    pub line: u32,
    pub start: u32,
    pub end: u32, // u32::MAX == to end of line
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default,
    Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum KeymapKind {
    #[default]
    Vscode,
    Vim,
}
```

Update `crates/vmux_editor/src/edit/command.rs`: delete the local `EditMode`, `CursorPos`, `SelSpan` definitions and replace with:

```rust
pub use vmux_core::{CursorPos, EditMode, SelSpan};
```

(Keep `Selection`, `Motion`, `EditCommand` local. The `EditMode::label`/`is_visual` calls in core.rs/keymaps now resolve to the core type.)

Update `crates/vmux_editor/src/keymap.rs`: delete the local `KeymapKind` enum, add `pub use vmux_core::KeymapKind;`, and (per Task 7) move `make`/`initial_mode` to a `KeymapKindExt` trait:

```rust
pub trait KeymapKindExt {
    fn make(self) -> Box<dyn Keymap>;
    fn initial_mode(self) -> EditMode;
}
impl KeymapKindExt for KeymapKind {
    fn make(self) -> Box<dyn Keymap> {
        match self {
            KeymapKind::Vscode => Box::new(vscode::VscodeKeymap),
            KeymapKind::Vim => Box::new(vim::VimKeymap::default()),
        }
    }
    fn initial_mode(self) -> EditMode {
        match self {
            KeymapKind::Vscode => EditMode::Insert,
            KeymapKind::Vim => EditMode::Normal,
        }
    }
}
```

- [ ] **Step 2: Add events to event.rs**

Append to `crates/vmux_core/src/event.rs` (consts beside the existing `FILE_*` block, structs with the standard derive set):

```rust
pub const FILE_TEXT_INPUT_EVENT: &str = "file_text_input";
pub const FILE_KEY_EVENT: &str = "file_key";
pub const FILE_POINTER_EVENT: &str = "file_pointer";
pub const FILE_CURSOR_EVENT: &str = "file_cursor";
pub const FILE_DIRTY_EVENT: &str = "file_dirty";
pub const FILE_EXTERNAL_CHANGE_EVENT: &str = "file_external_change";
```

```rust
// (each with: Debug, Clone, PartialEq, Serialize, Deserialize, rkyv::{Archive,Serialize,Deserialize})

pub struct FileTextInput {
    pub text: String,
}

pub struct KeyMods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}
// add Eq + Default to KeyMods

pub struct FileKeyEvent {
    pub key: String,
    pub code: String,
    pub mods: KeyMods,
    pub repeat: bool,
}

pub struct FilePointerEvent {
    pub line: u32,
    pub col: u32,
    pub extend: bool,
}

pub struct FileCursorEvent {
    pub mode: crate::editor::EditMode,
    pub mode_label: String,
    pub primary: crate::editor::CursorPos,
    pub selections: Vec<crate::editor::SelSpan>,
}

pub struct FileDirtyEvent {
    pub dirty: bool,
}

pub struct FileExternalChange {
    pub path: String,
}
```

- [ ] **Step 3: Add a roundtrip rkyv test (mirror existing event tests in event.rs)**

```rust
#[test]
fn file_cursor_event_roundtrips() {
    use crate::editor::{CursorPos, EditMode, SelSpan};
    let e = FileCursorEvent {
        mode: EditMode::Insert,
        mode_label: "INSERT".into(),
        primary: CursorPos { line: 3, col: 5 },
        selections: vec![SelSpan { line: 3, start: 0, end: 5 }],
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&e).unwrap();
    let back = rkyv::from_bytes::<FileCursorEvent, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(back, e);
}
```

- [ ] **Step 4: Build + test**

Run: `cargo test -p vmux_core file_cursor_event_roundtrips && cargo build -p vmux_editor`
Expected: PASS + clean build (command.rs/keymap.rs now reference the core types).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/editor.rs crates/vmux_core/src/lib.rs crates/vmux_core/src/event.rs crates/vmux_editor/src/edit/command.rs crates/vmux_editor/src/keymap.rs
git commit -m "feat(core): shared editor types + edit/cursor/dirty wire events"
```

### Task 10: Editor components + load + window emit on EditCore

**Files:**
- Modify: `crates/vmux_editor/src/keymap.rs` (trait bound `Send + Sync`)
- Modify: `crates/vmux_editor/src/plugin.rs`

**Context:** today the text branch inserts `FileBuffer { language, lines }`
(`plugin.rs:216-231`), and `emit_window` (`plugin.rs:340-363`),
`send_initial_meta` (`248-282`), `on_file_scroll` (`398-411`), `on_file_resize`
(`381-396`) read `FileBuffer`. We swap `FileBuffer` (text) for `EditCore` +
`HighlightCache`; dir/image keep their components.

- [ ] **Step 1: Make `Keymap` storable as a component**

In `keymap.rs` change `pub trait Keymap: Send {` → `pub trait Keymap: Send + Sync {`. (`VimKeymap`/`VscodeKeymap` are plain data, already `Send + Sync`.)

- [ ] **Step 2: Add components + clipboard resource to plugin.rs**

```rust
use crate::edit::{EditCore, EditMode};
use crate::edit::highlight_cache::HighlightCache;
use crate::keymap::{Keymap, KeymapKindExt};

#[derive(Component)]
pub struct EditState {
    pub core: EditCore,
    pub hl: HighlightCache,
}

#[derive(Component)]
pub struct EditorKeymap(pub Box<dyn Keymap>);

struct ClipboardHandle(Option<arboard::Clipboard>);
```

Insert the clipboard handle in `EditorPlugin::build` (beside the `FileWatch` insert):

```rust
app.insert_non_send(ClipboardHandle(arboard::Clipboard::new().ok()));
```

- [ ] **Step 3: Build EditState + keymap in the text branch of `load_file_buffers`**

Replace the final `let hl = Highlighter::new(); match hl.load_file(&fv.path) { ... }` block (the text-file branch, `plugin.rs:216-230`) with:

```rust
        let bytes = match std::fs::read(&fv.path) {
            Ok(b) => b,
            Err(e) => {
                commands.entity(entity).insert(FileBuffer {
                    language: format!("__error__:cannot read {}: {e}", fv.path.display()),
                    lines: Vec::new(),
                });
                continue;
            }
        };
        let text = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                commands.entity(entity).insert(FileBuffer {
                    language: format!("__error__:not a UTF-8 text file: {}", fv.path.display()),
                    lines: Vec::new(),
                });
                continue;
            }
        };
        let hl = HighlightCache::new(&fv.path);
        let kind = settings_keymap(&settings);
        let core = EditCore::new(fv.path.clone(), hl.language.clone(), &text, kind.initial_mode());
        commands.entity(entity).insert((EditState { core, hl }, EditorKeymap(kind.make())));
```

`load_file_buffers` must gain `settings: Res<vmux_setting::AppSettings>` as a
param. Add a helper at module scope:

```rust
fn settings_keymap(settings: &vmux_setting::AppSettings) -> vmux_core::KeymapKind {
    settings.editor.as_ref().map(|e| e.keymap).unwrap_or_default()
}
```

Keep `FileBuffer` for the `__error__` paths only. Update `UnloadedFileView` /
queries that gate "already loaded" to also treat `EditState` as loaded:
change `type UnloadedFileView = (Without<FileBuffer>, Without<FileDir>, Without<FileImage>);`
to also include `Without<EditState>`.

- [ ] **Step 4: Rewrite `emit_window` to use EditState**

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
    let (first, end) = crate::viewport::window_range(total, vp.top_line, vp.rows);
    let lines = edit.hl.line_window(&edit.core.buffer.rope, first as usize, end as usize);
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_VIEWPORT_EVENT,
        &FileViewportPatch { first_line: first, total_lines: total, lines },
    ));
}
```

Update callers (`send_initial_meta`, `on_file_scroll`, `on_file_resize`,
`reload_changed_files`) to query `&mut EditState` instead of `&FileBuffer`, set
`edit.core.rows = vp.rows` in resize, and use
`edit.core.buffer.len_lines()`/`display_path` for `FileMetaEvent.total_lines`.
For `on_file_scroll`, clamp with `clamp_top_line(evt.top_line,
edit.core.buffer.len_lines() as u32, vp.rows)`.

- [ ] **Step 5: Add a `emit_cursor` helper**

```rust
fn emit_cursor(
    entity: Entity,
    core: &EditCore,
    keymap: &dyn Keymap,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_CURSOR_EVENT,
        &FileCursorEvent {
            mode: keymap.mode(),
            mode_label: keymap.mode_label(),
            primary: core.cursor_pos(),
            selections: core.sel_spans(vp.top_line, vp.rows),
        },
    ));
}
```

- [ ] **Step 6: Build**

Run: `cargo build -p vmux_editor`
Expected: clean (no wasm). Fix query/borrow mismatches until it compiles.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs crates/vmux_editor/src/keymap.rs
git commit -m "feat(editor): EditState/keymap components, window+cursor emit on rope"
```

### Task 11: Edit observers (key / text / pointer) + integration test

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1: Register inbound events + observers in `EditorPlugin::build`**

Extend the existing `BinEventEmitterPlugin::<(...)>` tuple to include
`FileTextInput, FileKeyEvent, FilePointerEvent`, and add the observers:

```rust
            .add_observer(on_file_key)
            .add_observer(on_file_text_input)
            .add_observer(on_file_pointer)
```

- [ ] **Step 2: Write the shared apply path + observers**

```rust
fn run_commands(
    entity: Entity,
    cmds: Vec<crate::edit::EditCommand>,
    edit: &mut EditState,
    keymap: &mut dyn Keymap,
    vp: &mut FileViewport,
    clipboard: &mut ClipboardHandle,
    browsers: &Browsers,
    commands: &mut Commands,
) -> bool {
    use crate::edit::EditCommand;
    let mut text_changed = false;
    let mut sel_or_mode = false;
    let mut dirty_changed = false;
    for cmd in cmds {
        // Refresh register from the system clipboard before a paste.
        if matches!(cmd, EditCommand::Paste | EditCommand::PasteBefore) {
            if let Some(cb) = clipboard.0.as_mut() {
                if let Ok(s) = cb.get_text() {
                    edit.core.register = Some((s, false));
                }
            }
        }
        let out = edit.core.apply(cmd);
        if out.text_changed {
            text_changed = true;
            let (l, _) = edit.core.buffer.char_to_coords(edit.core.primary().head);
            edit.hl.invalidate_from(l.saturating_sub(1));
        }
        sel_or_mode |= out.sel_changed || out.mode_changed;
        dirty_changed |= out.dirty_changed;
        if let Some((s, _)) = out.yank {
            if let Some(cb) = clipboard.0.as_mut() {
                let _ = cb.set_text(s);
            }
        }
    }
    if let Some(top) = edit.core.autoscroll(vp.top_line, vp.rows) {
        vp.top_line = top;
        text_changed = true;
    }
    if text_changed {
        emit_window(entity, edit, vp, browsers, commands);
    }
    if text_changed || sel_or_mode {
        emit_cursor(entity, &edit.core, keymap, vp, browsers, commands);
    }
    if dirty_changed {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_DIRTY_EVENT,
            &FileDirtyEvent { dirty: edit.core.dirty },
        ));
    }
    text_changed
}

fn on_file_key(
    trigger: On<BinReceive<FileKeyEvent>>,
    mut q: Query<(&mut EditState, &mut EditorKeymap, &mut FileViewport)>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    let Ok((mut edit, mut keymap, mut vp)) = q.get_mut(entity) else { return };
    let input = crate::keymap::KeyInput {
        key: evt.key.clone(),
        mods: crate::keymap::Mods {
            ctrl: evt.mods.ctrl, alt: evt.mods.alt, shift: evt.mods.shift, meta: evt.mods.meta,
        },
        repeat: evt.repeat,
    };
    let cmds = keymap.0.handle(&input);
    let changed = run_commands(entity, cmds, &mut edit, keymap.0.as_mut(), &mut vp, &mut clipboard, &browsers, &mut commands);
    if changed {
        commands.entity(entity).insert(LspEditDirty(true)).remove::<crate::lsp::manager::LintRan>();
    }
}

fn on_file_text_input(
    trigger: On<BinReceive<FileTextInput>>,
    mut q: Query<(&mut EditState, &mut EditorKeymap, &mut FileViewport)>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let text = trigger.event().payload.text.clone();
    let Ok((mut edit, mut keymap, mut vp)) = q.get_mut(entity) else { return };
    let cmds = vec![crate::edit::EditCommand::InsertText(text)];
    let changed = run_commands(entity, cmds, &mut edit, keymap.0.as_mut(), &mut vp, &mut clipboard, &browsers, &mut commands);
    if changed {
        commands.entity(entity).insert(LspEditDirty(true)).remove::<crate::lsp::manager::LintRan>();
    }
}

fn on_file_pointer(
    trigger: On<BinReceive<FilePointerEvent>>,
    mut q: Query<(&mut EditState, &mut EditorKeymap, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let p = trigger.event().payload.clone();
    let Ok((mut edit, keymap, vp)) = q.get_mut(entity) else { return };
    let at = edit.core.buffer.coords_to_char(p.line as usize, p.col as usize);
    if p.extend {
        edit.core.apply(crate::edit::EditCommand::Select(crate::edit::Motion::Right)); // ensure visual anchor kept
        edit.core.selections = vec![crate::edit::Selection { anchor: edit.core.primary().anchor, head: at }];
    } else {
        edit.core.selections = vec![crate::edit::Selection::caret(at)];
    }
    emit_cursor(entity, &edit.core, keymap.0.as_ref(), vp, &browsers, &mut commands);
}
```

Add the `LspEditDirty` component near the other markers:

```rust
#[derive(Component)]
struct LspEditDirty(bool);
```

(Note: `Selection`, `Motion`, `EditCommand` are `pub` in `edit::command`;
re-export `Selection`/`Motion` from `edit.rs` so `crate::edit::Selection`
resolves. Add `pub use command::{Motion, Selection};` to `edit.rs`.)

- [ ] **Step 3: Write a Bevy integration test (headless, no CEF)**

Tests can't exercise `Browsers`/CEF emit, so test the core-through-keymap path
directly at the ECS layer by calling `run_commands` is impractical without
`Browsers`. Instead assert on `EditState` after feeding a key through the keymap
in a minimal harness:

```rust
#[cfg(test)]
mod edit_flow_tests {
    use super::*;
    use crate::keymap::{KeyInput, Mods, KeymapKindExt};

    #[test]
    fn vim_dd_deletes_line_via_keymap_and_core() {
        let mut km = vmux_core::KeymapKind::Vim.make();
        let mut core = EditCore::new(
            std::path::PathBuf::from("a.txt"), "Plain Text".into(),
            "one\ntwo\nthree\n", EditMode::Normal,
        );
        for key in ["d", "d"] {
            for cmd in km.handle(&KeyInput { key: key.into(), mods: Mods::default(), repeat: false }) {
                core.apply(cmd);
            }
        }
        assert_eq!(core.buffer.to_string(), "two\nthree\n");
    }
}
```

- [ ] **Step 4: Build + test**

Run: `cargo test -p vmux_editor edit_flow_tests && cargo build -p vmux_editor`
Expected: PASS + clean build.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs crates/vmux_editor/src/edit.rs
git commit -m "feat(editor): key/text/pointer edit observers"
```

### Task 12: Save, external-change guard, LSP didChange debounce

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1: Handle `Save` in `run_commands`**

In `run_commands`, before `edit.core.apply(cmd)`, intercept Save:

```rust
        if matches!(cmd, EditCommand::Save) {
            let body = edit.core.buffer.to_string();
            if std::fs::write(&edit.core.buffer.path, &body).is_ok() {
                edit.core.dirty = false;
                dirty_changed = true;
                commands.insert_resource_if_changed_marker(); // placeholder removed below
            }
            continue;
        }
```

Replace that placeholder line: instead, record a self-write guard. Add a
`NonSend` set and skip it in the watcher. Add near `FileWatch`:

```rust
#[derive(Default)]
struct SelfWrites(std::collections::HashMap<std::path::PathBuf, std::time::Instant>);
```

Insert `app.insert_non_send(SelfWrites::default());` in build. Pass
`self_writes: &mut SelfWrites` into `run_commands` and on save do:

```rust
        if matches!(cmd, EditCommand::Save) {
            let body = edit.core.buffer.to_string();
            if std::fs::write(&edit.core.buffer.path, &body).is_ok() {
                self_writes.0.insert(canon(&edit.core.buffer.path), std::time::Instant::now());
                edit.core.dirty = false;
                dirty_changed = true;
                commands.entity(entity).insert(LspEditDirty(true)); // triggers didSave via flush
            }
            continue;
        }
```

(Thread `self_writes: NonSendMut<SelfWrites>` into `on_file_key`/`on_file_text_input`.)

- [ ] **Step 2: Guard `reload_changed_files` against self-writes + dirty buffers**

In `drain_file_changes`/`reload_changed_files` (`plugin.rs:579-700`), before
inserting `FileReloadRequested` / before reloading a text file:

```rust
    // skip our own writes (within 1s)
    if let Some(t) = self_writes.0.get(&canon(&fv.path)) {
        if t.elapsed() < std::time::Duration::from_secs(1) { continue; }
    }
```

And in the text reload arm, if the entity has `EditState` and `edit.core.dirty`:

```rust
        if edit.core.dirty {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity, FILE_EXTERNAL_CHANGE_EVENT,
                &FileExternalChange { path: display_path(&fv.path) },
            ));
            continue; // do not clobber unsaved edits
        }
```

Otherwise rebuild `EditState` (new `EditCore` + fresh `HighlightCache`) from
disk, mirroring the load path, and `manager.change(&fv.path)`.

- [ ] **Step 3: LSP didChange debounce system**

```rust
#[derive(Resource, Default)]
struct LspDebounce(Option<std::time::Instant>);

fn flush_lsp_changes(
    time: Res<Time>,
    mut last: Local<f32>,
    q: Query<(Entity, &FileView, &EditState), With<LspEditDirty>>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    *last += time.delta_secs();
    if *last < 0.15 {
        return;
    }
    *last = 0.0;
    for (entity, fv, edit) in &q {
        manager.change(&fv.path); // existing API; re-runs diagnostics
        let _ = edit;
        commands.entity(entity).remove::<LspEditDirty>();
    }
}
```

Register `flush_lsp_changes` in the `Update` systems tuple. (If
`LspManager::change` needs the new text, extend it to read from `EditState`/disk
as the existing reload does — match the signature already used in
`reload_changed_files`.)

- [ ] **Step 4: Build**

Run: `cargo build -p vmux_editor`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/plugin.rs
git commit -m "feat(editor): save + self-write guard + external-change + LSP debounce"
```

---

## Milestone M5 — Page input controller + rendering (WASM)

> The page logic is exercised manually (no wasm unit harness here). Each task
> ends by building the wasm page and running the app to verify behavior.

### Task 13: Hidden textarea input controller + key routing

**Files:**
- Modify: `crates/vmux_editor/Cargo.toml` (web-sys features)
- Modify: `crates/vmux_editor/src/page.rs`

- [ ] **Step 1: Add web-sys features**

In the wasm `web-sys` features list (`crates/vmux_editor/Cargo.toml`), add:

```toml
    "HtmlTextAreaElement", "InputEvent", "CompositionEvent", "MouseEvent",
```

- [ ] **Step 2: Add edit-state signals + listeners in `Page()`**

Near the other `use_signal`s:

```rust
    let mut ed_mode = use_signal(|| vmux_core::editor::EditMode::Insert);
    let mut ed_label = use_signal(String::new);
    let mut cursor = use_signal(vmux_core::editor::CursorPos::default);
    let mut sel = use_signal(Vec::<vmux_core::editor::SelSpan>::new);
    let mut dirty = use_signal(|| false);
```

Add listeners (beside `_vp`):

```rust
    let _cur = use_bin_event_listener::<FileCursorEvent, _>(FILE_CURSOR_EVENT, move |c| {
        ed_mode.set(c.mode);
        ed_label.set(c.mode_label);
        cursor.set(c.primary);
        sel.set(c.selections);
    });
    let _dirty = use_bin_event_listener::<FileDirtyEvent, _>(FILE_DIRTY_EVENT, move |d| {
        dirty.set(d.dirty);
    });
```

- [ ] **Step 3: Helper to read modifiers + send a key event**

```rust
fn key_mods(raw: &web_sys::KeyboardEvent) -> KeyMods {
    KeyMods { ctrl: raw.ctrl_key(), alt: raw.alt_key(), shift: raw.shift_key(), meta: raw.meta_key() }
}

fn is_text_key(key: &str) -> bool {
    // single Unicode scalar with no name (printable) => text
    key.chars().count() == 1
}
```

- [ ] **Step 4: Render the hidden textarea overlay (inside the Text-mode branch, above the lines div)**

```rust
    textarea {
        id: "file-input",
        class: "absolute z-10 m-0 resize-none border-0 bg-transparent p-0 text-transparent caret-transparent outline-none",
        style: "left:{cursor().col as f64 * cell_dims().0}px; top:{(cursor().line.saturating_sub(first_line()) ) as f64 * cell_dims().1}px; width:1ch; height:{cell_dims().1}px;",
        autocomplete: "off",
        autocapitalize: "off",
        spellcheck: "false",
        oncompositionend: move |_| {
            send_committed_text();
        },
        oninput: move |e: Event<FormData>| {
            let data = e.data();
            if let Some(raw) = data.downcast::<web_sys::InputEvent>() {
                if raw.is_composing() { return; }
            }
            send_committed_text();
        },
        onkeydown: move |e: Event<KeyboardData>| {
            let data = e.data();
            let Some(raw) = data.downcast::<web_sys::KeyboardEvent>() else { return; };
            if raw.is_composing() { return; }
            let key = raw.key();
            let mods = key_mods(&raw);
            let chord = mods.ctrl || mods.alt || mods.meta;
            let text_mode = ed_mode().accepts_text();
            // In Insert/vscode a plain printable becomes text via `input`; don't send a key.
            if text_mode && !chord && is_text_key(&key) {
                return;
            }
            e.prevent_default();
            let _ = try_cef_bin_emit_rkyv(&FileKeyEvent {
                key, code: raw.code(), mods, repeat: raw.repeat(),
            });
        },
    }
```

`send_committed_text` reads the textarea value, emits `FileTextInput`, clears it:

```rust
fn send_committed_text() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("file-input"))
        .and_then(|e| e.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
    {
        let v = el.value();
        if !v.is_empty() {
            let _ = try_cef_bin_emit_rkyv(&FileTextInput { text: v });
            el.set_value("");
        }
    }
}
```

- [ ] **Step 5: Focus the textarea instead of the container in Text mode**

Change `focus_container()` usage so that, in Text mode, `#file-input` is focused
(keep `#file-container` focus for Dir/Image modes). Update `use_effect` and the
container `onmousedown` accordingly: focus `#file-input` when `mode()==Text`.

- [ ] **Step 6: Remove the old text-scroll keys from the container `onkeydown`**

In the container `onkeydown` `Mode::Text` arm (`page.rs:652-677`), keep only the
dir back-nav (`Escape`/`h` with `back_dir`); delete the ArrowUp/Down/PageUp/Down/
Home → `FileScrollEvent` block (editing/caret now drives movement; scrolling is
wheel + autoscroll + the caret motions handled natively). Keep `onwheel`.

- [ ] **Step 7: Build wasm page + run**

Run: `cargo build -p vmux_editor --target wasm32-unknown-unknown` (or the
project's page-build via `make`/`vmux_server` build). Then run the app, open a
text file, type ASCII + Japanese (IME), confirm text appears and commits.
Expected: typing inserts; IME composition commits on enter; arrows/backspace
work.

- [ ] **Step 8: Commit**

```bash
git add crates/vmux_editor/Cargo.toml crates/vmux_editor/src/page.rs
git commit -m "feat(editor): hidden textarea IME + key routing on the page"
```

### Task 14: Caret + selection rendering, mode badge, dirty dot, pointer

**Files:**
- Modify: `crates/vmux_editor/src/page.rs`

- [ ] **Step 1: Render caret + selection overlay inside the lines container**

Add, as a sibling of the `for line in lines()` block (positioned relative to the
scrolling content), driven by `cursor()`/`sel()` and `cell_dims()`/`first_line()`:

```rust
    // caret
    {
        let (cw, ch) = cell_dims();
        let top = (cursor().line.saturating_sub(first_line())) as f64 * ch;
        let left = cursor().col as f64 * cw;
        rsx! {
            div {
                class: "pointer-events-none absolute z-20 w-[2px] bg-cyan-300 animate-pulse",
                style: "left:{left}px; top:{top}px; height:{ch}px;",
            }
        }
    }
    // selection rects
    for s in sel().iter() {
        {
            let (cw, ch) = cell_dims();
            let top = (s.line.saturating_sub(first_line())) as f64 * ch;
            let left = s.start as f64 * cw;
            let width = if s.end == u32::MAX { 100.0 } else { (s.end.saturating_sub(s.start)) as f64 * cw };
            let wcss = if s.end == u32::MAX { "calc(100% - ".to_string() + &format!("{left}px)") } else { format!("{width}px") };
            rsx! {
                div {
                    key: "sel{s.line}",
                    class: "pointer-events-none absolute z-0 bg-cyan-400/20",
                    style: "left:{left}px; top:{top}px; width:{wcss}; height:{ch}px;",
                }
            }
        }
    }
```

(Ensure the lines content wrapper is `position: relative` so absolute children
anchor to it; the existing `div.min-w-max.py-2` can take a `relative` class.)

- [ ] **Step 2: Mode badge + dirty dot in the header**

In the header row (`page.rs:680-702`), before the `lsp_status` block:

```rust
    {
        let lbl = ed_label();
        (!lbl.is_empty()).then(|| rsx! {
            span { class: "rounded bg-cyan-400/15 px-1.5 py-0.5 text-[10px] font-semibold text-cyan-200", "{lbl}" }
        })
    }
    if dirty() {
        span { class: "ml-1 h-1.5 w-1.5 rounded-full bg-cyan-300", title: "unsaved" }
    }
```

- [ ] **Step 3: Pointer → place/extend cursor**

On each rendered line's content span (the `span.relative.whitespace-pre` in the
`Mode::Text` lines loop), add a mousedown that maps click x to a column:

```rust
                                onmousedown: move |e: Event<MouseData>| {
                                    let (cw, _) = cell_dims();
                                    let data = e.data();
                                    if let Some(raw) = data.downcast::<web_sys::MouseEvent>() {
                                        let target = raw.target()
                                            .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
                                        if let Some(el) = target {
                                            let rect = el.get_bounding_client_rect();
                                            let x = raw.client_x() as f64 - rect.left();
                                            let col = (x / cw).round().max(0.0) as u32;
                                            let _ = try_cef_bin_emit_rkyv(&FilePointerEvent {
                                                line: ln, col, extend: raw.shift_key(),
                                            });
                                        }
                                    }
                                    focus_file_input();
                                },
```

(`ln` is the line's absolute `line_no` already in scope. `focus_file_input()`
focuses `#file-input`.)

- [ ] **Step 4: Build + run**

Run the app; verify: blinking caret tracks typing/motions; selection highlights
for vim visual + vscode shift-arrow; mode badge shows NORMAL/INSERT/VISUAL in
vim; dirty dot appears on edit and clears on save (`:w` / Cmd-S); click places
the caret; shift-click extends.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_editor/src/page.rs
git commit -m "feat(editor): caret/selection render, mode badge, dirty dot, pointer"
```

---

## Final verification

- [ ] `cargo fmt --all` (then `git checkout -- patches/` if it touched vendored crates — `cargo fmt` reformats `patches/` too; commit only `crates/` formatting).
- [ ] `cargo clippy -p vmux_editor -p vmux_core -p vmux_setting --all-targets`
- [ ] `cargo test -p vmux_editor -p vmux_core -p vmux_setting`
- [ ] Manual matrix (user): vscode keymap (arrows/shift-select/Cmd-CXVZ/S, word-jump, backspace-word), vim keymap (hjkl/w/b/e, i/a/o/O, x/dd/dw/cc/yy/p, v/V select, u/Ctrl-r, :w), keymap switch via `settings.ron`, Japanese IME, external-change banner, save round-trips to disk, scroll + caret autoscroll on a long file.
- [ ] Delete this plan file once fully implemented (per AGENTS.md), in the final commit.
