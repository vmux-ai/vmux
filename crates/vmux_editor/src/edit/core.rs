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
    pub yank: Option<(String, bool)>,
}

pub struct EditCore {
    pub buffer: TextBuffer,
    pub selections: Vec<Selection>,
    pub mode: EditMode,
    pub rows: u16,
    pub dirty: bool,
    pub register: Option<(String, bool)>,
    rev: u64,
    saved_rev: Option<u64>,
    undo: Vec<(ropey::Rope, Vec<Selection>, u64)>,
    redo: Vec<(ropey::Rope, Vec<Selection>, u64)>,
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
            rev: 0,
            saved_rev: Some(0),
            undo: Vec::new(),
            redo: Vec::new(),
            last_group: None,
        }
    }

    pub fn mark_saved(&mut self) {
        self.saved_rev = Some(self.rev);
        self.dirty = false;
        self.last_group = None;
    }

    pub fn primary(&self) -> Selection {
        self.selections[0]
    }
    pub fn set_caret(&mut self, at: usize) {
        self.selections = vec![Selection::caret(at)];
    }
    fn set_head(&mut self, head: usize) {
        let anchor = self.selections[0].anchor;
        self.selections = vec![Selection { anchor, head }];
    }

    fn vis_col(&self, line_start: usize, col: usize) -> u32 {
        let s: String = self
            .buffer
            .rope
            .slice(line_start..line_start + col)
            .chars()
            .collect();
        UnicodeWidthStr::width(s.as_str()) as u32
    }

    pub fn cursor_pos(&self) -> CursorPos {
        let head = self.primary().head;
        let (line, col) = self.buffer.char_to_coords(head);
        let line_start = self.buffer.line_to_char(line);
        CursorPos {
            line: line as u32,
            col: self.vis_col(line_start, col),
        }
    }

    pub fn sel_spans(&self, first: u32, rows: u16) -> Vec<SelSpan> {
        let sel = self.primary();
        if sel.is_empty() || rows == 0 {
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
            let end = if line < l1 {
                u32::MAX
            } else {
                self.vis_col(ls, ec)
            };
            out.push(SelSpan {
                line: line as u32,
                start: self.vis_col(ls, sc),
                end,
            });
        }
        out
    }

    fn break_group(&mut self) {
        self.last_group = None;
    }
    fn snapshot(&mut self) {
        self.undo
            .push((self.buffer.rope.clone(), self.selections.clone(), self.rev));
        self.redo.clear();
    }
    fn checkpoint(&mut self, group: Group) {
        if self.last_group != Some(group) || group == Group::Other {
            self.snapshot();
        }
        self.last_group = Some(group);
        self.rev += 1;
        self.dirty = self.saved_rev != Some(self.rev);
    }

    fn resolve_motion(&self, from: usize, motion: Motion) -> usize {
        let len = self.buffer.len_chars();
        match motion {
            Motion::Left => self.buffer.prev_grapheme(from),
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
        self.checkpoint(Group::Insert);
        if !self.primary().is_empty() {
            let r = self.primary().range();
            self.buffer.remove(r.clone());
            self.set_caret(r.start);
        }
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
        let before_sel = self.primary();
        let before_mode = self.mode;
        let before_dirty = self.dirty;
        let mut text_changed = false;
        let mut yank: Option<(String, bool)> = None;

        match cmd {
            EditCommand::Move(m) => {
                self.break_group();
                let h = self.resolve_motion(self.primary().head, m);
                if self.mode.is_visual() {
                    self.set_head(h);
                } else {
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
                        let prev = self.buffer.prev_grapheme(head);
                        self.buffer.remove(prev..head);
                        self.set_caret(prev);
                        text_changed = true;
                    }
                } else {
                    text_changed = self.delete_selection();
                }
            }
            EditCommand::DeleteForward => {
                if !self.primary().is_empty() {
                    text_changed = self.delete_selection();
                } else {
                    let head = self.primary().head;
                    if head < self.buffer.len_chars() {
                        self.checkpoint(Group::Delete);
                        let next = self.buffer.next_grapheme(head);
                        self.buffer.remove(head..next);
                        text_changed = true;
                    }
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
            EditCommand::YankRange(m) => {
                let head = self.primary().head;
                let target = self.resolve_motion(head, m);
                let (a, b) = (head.min(target), head.max(target));
                if b > a {
                    let s: String = self.buffer.rope.slice(a..b).chars().collect();
                    self.register = Some((s.clone(), false));
                    yank = Some((s, false));
                    self.set_caret(a);
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
            EditCommand::Undo => {
                if let Some((rope, sel, rev)) = self.undo.pop() {
                    self.redo
                        .push((self.buffer.rope.clone(), self.selections.clone(), self.rev));
                    self.buffer.rope = rope;
                    self.selections = sel;
                    self.rev = rev;
                    self.dirty = self.saved_rev != Some(self.rev);
                    self.break_group();
                    text_changed = true;
                }
            }
            EditCommand::Redo => {
                if let Some((rope, sel, rev)) = self.redo.pop() {
                    self.undo
                        .push((self.buffer.rope.clone(), self.selections.clone(), self.rev));
                    self.buffer.rope = rope;
                    self.selections = sel;
                    self.rev = rev;
                    self.dirty = self.saved_rev != Some(self.rev);
                    self.break_group();
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
            EditCommand::Paste => {
                if let Some((s, _linewise)) = self.register.clone() {
                    let was_visual = self.mode.is_visual();
                    if !self.primary().is_empty() {
                        self.delete_selection();
                    }
                    self.mode = if was_visual {
                        EditMode::Normal
                    } else {
                        self.mode
                    };
                    self.checkpoint(Group::Other);
                    let at = if !was_visual && self.mode == EditMode::Normal {
                        self.buffer.next_grapheme(self.primary().head)
                    } else {
                        self.primary().head
                    };
                    self.buffer.insert(at, &s);
                    self.set_caret(at + s.chars().count());
                    text_changed = true;
                }
            }
            EditCommand::PasteBefore => {
                if let Some((s, _linewise)) = self.register.clone() {
                    if !self.primary().is_empty() {
                        self.delete_selection();
                    }
                    if self.mode.is_visual() {
                        self.mode = EditMode::Normal;
                    }
                    self.checkpoint(Group::Other);
                    let at = self.primary().head;
                    self.buffer.insert(at, &s);
                    self.set_caret(at + s.chars().count());
                    text_changed = true;
                }
            }
            EditCommand::Save
            | EditCommand::GotoDefinition
            | EditCommand::FindReferences
            | EditCommand::Hover
            | EditCommand::TriggerCompletion => {}
        }

        EditOutcome {
            text_changed,
            sel_changed: self.primary() != before_sel,
            mode_changed: self.mode != before_mode,
            dirty_changed: self.dirty != before_dirty,
            scroll_to: None,
            yank,
        }
    }

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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(text: &str) -> EditCore {
        EditCore::new(
            PathBuf::from("a.txt"),
            "Plain Text".into(),
            text,
            EditMode::Insert,
        )
    }
    fn text_of(c: &EditCore) -> String {
        c.buffer.text()
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
        c.set_caret(1);
        assert_eq!(c.cursor_pos(), CursorPos { line: 0, col: 2 });
    }

    #[test]
    fn typing_over_selection_replaces() {
        let mut c = core("abcdef");
        c.selections = vec![Selection { anchor: 1, head: 4 }];
        c.apply(EditCommand::InsertText("X".into()));
        assert_eq!(text_of(&c), "aXef");
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut c = core("");
        c.apply(EditCommand::InsertText("abc".into()));
        c.apply(EditCommand::SetMode(EditMode::Normal));
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
    fn delete_forward_removes_selection() {
        let mut c = core("abcdef");
        c.selections = vec![Selection { anchor: 1, head: 4 }];
        c.apply(EditCommand::DeleteForward);
        assert_eq!(text_of(&c), "aef");
    }

    #[test]
    fn undo_back_to_saved_is_clean() {
        let mut c = core("");
        c.apply(EditCommand::InsertText("ab".into()));
        c.mark_saved();
        assert!(!c.dirty);
        c.apply(EditCommand::InsertText("c".into()));
        assert!(c.dirty);
        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "ab");
        assert!(!c.dirty, "undo to saved revision clears dirty");
    }

    #[test]
    fn delete_back_is_grapheme_aware() {
        let mut c = core("ae\u{0301}");
        c.set_caret(c.buffer.len_chars());
        c.apply(EditCommand::DeleteBack);
        assert_eq!(text_of(&c), "a");
    }

    #[test]
    fn paste_after_vs_before_in_normal() {
        let mut c = core("ac");
        c.register = Some(("X".into(), false));
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Paste);
        assert_eq!(text_of(&c), "aXc");
        let mut c2 = core("ac");
        c2.register = Some(("X".into(), false));
        c2.mode = EditMode::Normal;
        c2.set_caret(0);
        c2.apply(EditCommand::PasteBefore);
        assert_eq!(text_of(&c2), "Xac");
    }

    #[test]
    fn autoscroll_follows_caret_down() {
        let mut c = core("a\nb\nc\nd\ne\nf\n");
        c.rows = 3;
        c.set_caret(c.buffer.coords_to_char(5, 0));
        assert_eq!(c.autoscroll(0, 3), Some(3));
    }
}
