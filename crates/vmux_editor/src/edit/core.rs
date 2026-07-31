use std::path::PathBuf;

use unicode_width::UnicodeWidthStr;

use crate::edit::buffer::TextBuffer;
use crate::edit::command::{
    CursorPos, EditCommand, EditMode, Motion, MotionKind, Operator, SelSpan, Selection, Target,
};
use crate::edit::register::{RegisterKind, RegisterValue, Registers};
use crate::edit::text_object::char_class;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Insert,
    Delete,
    Other,
}

fn translated_caret_after_replace(old: &str, new: &str, caret: usize) -> usize {
    let old = old.chars().collect::<Vec<_>>();
    let new = new.chars().collect::<Vec<_>>();
    let prefix = old
        .iter()
        .zip(&new)
        .take_while(|(left, right)| left == right)
        .count();
    if caret <= prefix {
        return caret.min(new.len());
    }
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let old_suffix = old.len().saturating_sub(suffix);
    let new_suffix = new.len().saturating_sub(suffix);
    if caret >= old_suffix {
        return (new_suffix + caret.saturating_sub(old_suffix)).min(new.len());
    }
    (prefix
        + caret
            .saturating_sub(prefix)
            .min(new_suffix.saturating_sub(prefix)))
    .min(new.len())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditOutcome {
    pub text_changed: bool,
    pub sel_changed: bool,
    pub mode_changed: bool,
    pub dirty_changed: bool,
    pub scroll_to: Option<u32>,
    pub yank: Option<RegisterValue>,
}

pub struct EditCore {
    pub buffer: TextBuffer,
    pub selections: Vec<Selection>,
    pub mode: EditMode,
    pub rows: u16,
    pub top_row: u32,
    pub dirty: bool,
    pub registers: Registers,
    rev: u64,
    saved_rev: Option<u64>,
    undo: Vec<(ropey::Rope, Vec<Selection>, u64)>,
    redo: Vec<(ropey::Rope, Vec<Selection>, u64)>,
    last_group: Option<Group>,
    preferred_vertical_col: Option<usize>,
    pub fold_view: crate::fold::FoldView,
    pub search: Option<crate::edit::search::Search>,
    replaced: Vec<Option<char>>,
    pub search_highlight: bool,
    marks: std::collections::HashMap<char, usize>,
    jumps: Vec<usize>,
    jump_index: usize,
    changes: Vec<usize>,
    change_index: usize,
}

impl EditCore {
    pub fn new(path: PathBuf, language: String, text: &str, default_mode: EditMode) -> Self {
        let buffer = TextBuffer::from_text(path, language, text);
        let fold_view = crate::fold::FoldState::default().view(buffer.len_lines() as u32);
        Self {
            buffer,
            selections: vec![Selection::caret(0)],
            mode: default_mode,
            rows: 0,
            top_row: 0,
            dirty: false,
            registers: Registers::default(),
            rev: 0,
            saved_rev: Some(0),
            undo: Vec::new(),
            redo: Vec::new(),
            last_group: None,
            preferred_vertical_col: None,
            fold_view,
            search: None,
            replaced: Vec::new(),
            search_highlight: false,
            marks: std::collections::HashMap::new(),
            jumps: Vec::new(),
            jump_index: 0,
            changes: Vec::new(),
            change_index: 0,
        }
    }

    /// Insert text and slide any marks sitting after the insertion point.
    fn buf_insert(&mut self, at: usize, text: &str) {
        self.buffer.insert(at, text);
        let n = text.chars().count();
        for mark in self.marks.values_mut() {
            if *mark > at {
                *mark += n;
            }
        }
    }

    /// Remove a range, pulling marks inside it back to its start.
    fn buf_remove(&mut self, range: std::ops::Range<usize>) {
        self.buffer.remove(range.clone());
        let n = range.end - range.start;
        for mark in self.marks.values_mut() {
            if *mark >= range.end {
                *mark -= n;
            } else if *mark > range.start {
                *mark = range.start;
            }
        }
    }

    /// Record the current position on the jump list so `Ctrl-o` can come back to it.
    fn push_jump(&mut self) {
        let at = self.primary().head;
        self.jumps.truncate(self.jump_index);
        if self.jumps.last() == Some(&at) {
            return;
        }
        self.jumps.push(at);
        self.jump_index = self.jumps.len();
    }

    fn jump(&mut self, back: bool, count: usize) {
        let here = self.primary().head;
        for _ in 0..count.max(1) {
            if back {
                if self.jump_index == 0 {
                    break;
                }
                if self.jump_index == self.jumps.len() {
                    self.jumps.push(here);
                }
                self.jump_index -= 1;
            } else {
                if self.jump_index + 1 >= self.jumps.len() {
                    break;
                }
                self.jump_index += 1;
            }
        }
        if let Some(&at) = self.jumps.get(self.jump_index) {
            self.set_caret(at.min(self.buffer.len_chars()));
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
        self.preferred_vertical_col = None;
        self.place_caret(at);
    }
    fn place_caret(&mut self, at: usize) {
        let at = if self.mode == EditMode::Normal {
            self.normal_cursor_target(at)
        } else {
            at.min(self.buffer.len_chars())
        };
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
            row: line as u32,
            col: self.vis_col(line_start, col),
        }
    }

    /// The primary selection as an operator would see it.
    ///
    /// Vim's charwise visual selection covers the character under the cursor, and linewise visual
    /// covers whole lines; the underlying anchor/head pair is exclusive, so widen it here and keep
    /// rendering and effect in agreement.
    pub fn visual_range(&self) -> std::ops::Range<usize> {
        let sel = self.primary();
        let r = sel.range();
        match self.mode {
            EditMode::Visual => {
                r.start
                    ..self
                        .buffer
                        .next_grapheme(r.end)
                        .min(self.buffer.len_chars())
            }
            EditMode::VisualLine => self.expand_linewise(r),
            _ => r,
        }
    }

    pub fn sel_spans(&self, first: u32, rows: u16) -> Vec<SelSpan> {
        let sel = self.primary();
        if sel.is_empty() && !self.mode.is_visual() {
            return Vec::new();
        }
        if rows == 0 {
            return Vec::new();
        }
        let r = self.visual_range();
        if r.start >= r.end {
            return Vec::new();
        }
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
                row: line as u32,
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
        let at = self.primary().head;
        if self.changes.last() != Some(&at) {
            self.changes.push(at);
        }
        self.change_index = self.changes.len().saturating_sub(1);
    }

    fn resolve_motion(&self, from: usize, motion: Motion) -> usize {
        let len = self.buffer.len_chars();
        match motion {
            Motion::Left => self.buffer.prev_grapheme(from),
            Motion::Right => self.buffer.next_grapheme(from).min(len),
            Motion::LeftBounded => self.line_left(from),
            Motion::RightBounded => self.line_right(from),
            Motion::Up => self.vertical(from, -1),
            Motion::Down => self.vertical(from, 1),
            Motion::PageUp => self.vertical(from, -(self.rows.max(1) as i64)),
            Motion::PageDown => self.vertical(from, self.rows.max(1) as i64),
            Motion::ParagraphPrev => self.paragraph_prev(from),
            Motion::ParagraphNext => self.paragraph_next(from),
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
            Motion::WordNext => self.word_next(from, false),
            Motion::WordPrev => self.word_prev(from, false),
            Motion::WordEnd => self.word_end(from, false),
            Motion::BigWordNext => self.word_next(from, true),
            Motion::BigWordPrev => self.word_prev(from, true),
            Motion::BigWordEnd => self.word_end(from, true),
            Motion::WordEndPrev => self.word_end_prev(from, false),
            Motion::BigWordEndPrev => self.word_end_prev(from, true),
            Motion::LastNonBlank => self.last_non_blank(from),
            Motion::Column(n) => {
                let (l, _) = self.buffer.char_to_coords(from);
                let start = self.buffer.line_to_char(l);
                (start + n.saturating_sub(1)).min(start + self.buffer.line_len_chars(l))
            }
            Motion::HalfPageUp => self.vertical(from, -((self.rows.max(2) / 2) as i64)),
            Motion::HalfPageDown => self.vertical(from, (self.rows.max(2) / 2) as i64),
            Motion::ScreenTop => self.screen_line(0),
            Motion::ScreenMiddle => self.screen_line(self.rows.saturating_sub(1) / 2),
            Motion::ScreenBottom => self.screen_line(self.rows.saturating_sub(1)),
            Motion::NextLineStart => self.first_non_blank(self.vertical(from, 1)),
            Motion::PrevLineStart => self.first_non_blank(self.vertical(from, -1)),
            Motion::MatchPair => self.match_pair(from).unwrap_or(from),
            Motion::FindChar { ch, forward, till } => {
                self.find_char(from, ch, forward, till).unwrap_or(from)
            }
            Motion::SearchNext { reverse } => self.search_step(from, reverse).unwrap_or(from),
        }
    }

    /// Char positions of every match of the active search, in buffer order.
    pub fn search_matches(&self) -> Vec<std::ops::Range<usize>> {
        let Some(search) = self.search.as_ref() else {
            return Vec::new();
        };
        let text = self.buffer.text();
        search
            .matches(&text)
            .into_iter()
            .map(|r| {
                let start = text[..r.start].chars().count();
                let len = text[r.clone()].chars().count();
                start..start + len
            })
            .collect()
    }

    fn search_step(&self, from: usize, reverse: bool) -> Option<usize> {
        let search = self.search.as_ref()?;
        let forward = search.forward != reverse;
        let matches = self.search_matches();
        crate::edit::search::step(&matches, from, forward)
    }

    /// Highlight spans for the active search, in the same shape the selection uses.
    pub fn search_spans(&self, first: u32, rows: u16) -> Vec<SelSpan> {
        if !self.search_highlight || rows == 0 {
            return Vec::new();
        }
        let last_line = first as usize + rows as usize;
        let mut out = Vec::new();
        for m in self.search_matches() {
            let (l0, _) = self.buffer.char_to_coords(m.start);
            let (l1, _) = self.buffer.char_to_coords(m.end);
            if l1 < first as usize || l0 >= last_line {
                continue;
            }
            for line in l0..=l1 {
                if line < first as usize || line >= last_line {
                    continue;
                }
                let ls = self.buffer.line_to_char(line);
                let llen = self.buffer.line_len_chars(line);
                let sc = if line == l0 { m.start - ls } else { 0 };
                let ec = if line == l1 { m.end - ls } else { llen };
                out.push(SelSpan {
                    line: line as u32,
                    row: line as u32,
                    start: self.vis_col(ls, sc),
                    end: self.vis_col(ls, ec),
                });
            }
        }
        out
    }

    /// The keyword under the cursor, used by `*` and `#`.
    fn word_under_cursor(&self) -> Option<String> {
        let len = self.buffer.len_chars();
        if len == 0 {
            return None;
        }
        let head = self.primary().head.min(len - 1);
        let mut start = head;
        while start < len && char_class(self.buffer.rope.char(start)) != 1 {
            start += 1;
        }
        if start >= len || self.buffer.char_to_line(start) != self.buffer.char_to_line(head) {
            return None;
        }
        let mut end = start;
        while start > 0 && char_class(self.buffer.rope.char(start - 1)) == 1 {
            start -= 1;
        }
        while end < len && char_class(self.buffer.rope.char(end)) == 1 {
            end += 1;
        }
        Some(self.buffer.rope.slice(start..end).chars().collect())
    }

    /// The first non-blank of the buffer line displayed `offset` rows below the viewport top.
    fn screen_line(&self, offset: u16) -> usize {
        let line = self.fold_view.step_rows(self.top_row, offset as i64);
        self.first_non_blank(self.buffer.line_to_char(line as usize))
    }

    fn last_non_blank(&self, from: usize) -> usize {
        let (l, _) = self.buffer.char_to_coords(from);
        let base = self.buffer.line_to_char(l);
        let llen = self.buffer.line_len_chars(l);
        let mut i = llen;
        while i > 0 {
            let ch = self.buffer.rope.char(base + i - 1);
            if ch != ' ' && ch != '\t' {
                return base + i - 1;
            }
            i -= 1;
        }
        base
    }

    /// Vim's `%`: from the first bracket at or after the cursor on this line, jump to its partner.
    fn match_pair(&self, from: usize) -> Option<usize> {
        const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
        let (line, _) = self.buffer.char_to_coords(from);
        let base = self.buffer.line_to_char(line);
        let llen = self.buffer.line_len_chars(line);
        let col = from - base;
        for i in col..llen {
            let at = base + i;
            let c = self.buffer.rope.char(at);
            if let Some((open, close)) = PAIRS.iter().find(|(o, _)| *o == c) {
                return self.scan_pair(at, *open, *close, true);
            }
            if let Some((open, close)) = PAIRS.iter().find(|(_, c2)| *c2 == c) {
                return self.scan_pair(at, *open, *close, false);
            }
        }
        None
    }

    fn scan_pair(&self, at: usize, open: char, close: char, forward: bool) -> Option<usize> {
        let len = self.buffer.len_chars();
        let mut depth = 0i32;
        let mut i = at as i64;
        loop {
            let c = self.buffer.rope.char(i as usize);
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
            }
            if depth == 0 && (c == open || c == close) && i as usize != at {
                return Some(i as usize);
            }
            i += if forward { 1 } else { -1 };
            if i < 0 || i as usize >= len {
                return None;
            }
        }
    }

    /// Vim's `f`/`F`/`t`/`T`: search the cursor's line only.
    fn find_char(&self, from: usize, ch: char, forward: bool, till: bool) -> Option<usize> {
        let (line, _) = self.buffer.char_to_coords(from);
        let base = self.buffer.line_to_char(line);
        let llen = self.buffer.line_len_chars(line);
        let col = from - base;
        if forward {
            let start = if till { col + 2 } else { col + 1 };
            for i in start..llen {
                if self.buffer.rope.char(base + i) == ch {
                    return Some(base + if till { i - 1 } else { i });
                }
            }
        } else {
            let end = if till { col.checked_sub(1)? } else { col };
            for i in (0..end).rev() {
                if self.buffer.rope.char(base + i) == ch {
                    return Some(base + if till { i + 1 } else { i });
                }
            }
        }
        None
    }

    fn resolve_navigation_motion(&mut self, from: usize, motion: Motion) -> usize {
        let delta = match motion {
            Motion::Up => Some(-1),
            Motion::Down => Some(1),
            Motion::PageUp => Some(-(self.rows.max(1) as i64)),
            Motion::PageDown => Some(self.rows.max(1) as i64),
            _ => None,
        };
        let Some(delta) = delta else {
            self.preferred_vertical_col = None;
            return self.resolve_motion(from, motion);
        };
        let (_, current_col) = self.buffer.char_to_coords(from);
        let preferred_col = *self.preferred_vertical_col.get_or_insert(current_col);
        let (line, _) = self.buffer.char_to_coords(from);
        let target = self.fold_view.step_rows(line as u32, delta) as usize;
        self.buffer.coords_to_char(target, preferred_col)
    }

    fn vertical(&self, from: usize, delta: i64) -> usize {
        let (l, c) = self.buffer.char_to_coords(from);
        let target = self.fold_view.step_rows(l as u32, delta) as usize;
        self.buffer.coords_to_char(target, c)
    }
    fn line_left(&self, from: usize) -> usize {
        let (line, col) = self.buffer.char_to_coords(from);
        let start = self.buffer.line_to_char(line);
        if col == 0 {
            start
        } else {
            self.buffer.prev_grapheme(from).max(start)
        }
    }
    fn line_right(&self, from: usize) -> usize {
        let (line, _) = self.buffer.char_to_coords(from);
        let start = self.buffer.line_to_char(line);
        let end = start + self.buffer.line_len_chars(line);
        if end == start {
            return start;
        }
        let last = self.buffer.prev_grapheme(end);
        if from >= last {
            last
        } else {
            self.buffer.next_grapheme(from).min(last)
        }
    }
    fn normal_cursor_target(&self, at: usize) -> usize {
        let at = at.min(self.buffer.len_chars());
        let (line, _) = self.buffer.char_to_coords(at);
        let start = self.buffer.line_to_char(line);
        let end = start + self.buffer.line_len_chars(line);
        if start == end {
            start
        } else {
            at.clamp(start, self.buffer.prev_grapheme(end))
        }
    }
    fn paragraph_prev(&self, from: usize) -> usize {
        let (current, _) = self.buffer.char_to_coords(from);
        if current == 0 {
            return 0;
        }
        let mut line = current - 1;
        while line > 0 && self.buffer.line_len_chars(line) == 0 {
            line -= 1;
        }
        while line > 0 && self.buffer.line_len_chars(line - 1) > 0 {
            line -= 1;
        }
        self.buffer.line_to_char(line)
    }
    fn paragraph_next(&self, from: usize) -> usize {
        let (current, _) = self.buffer.char_to_coords(from);
        let total = self.buffer.len_lines();
        let mut line = current.saturating_add(1);
        while line < total && self.buffer.line_len_chars(line) > 0 {
            line += 1;
        }
        while line < total && self.buffer.line_len_chars(line) == 0 {
            line += 1;
        }
        if line < total {
            return self.buffer.line_to_char(line);
        }
        from
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

    /// Word classes, or the coarser WORD split that only separates on whitespace.
    fn cls(&self, i: usize, big: bool) -> u8 {
        let c = self.buffer.rope.char(i);
        if big {
            if c.is_whitespace() { 0 } else { 1 }
        } else {
            char_class(c)
        }
    }
    fn word_next(&self, from: usize, big: bool) -> usize {
        let len = self.buffer.len_chars();
        let mut i = from;
        if i >= len {
            return len;
        }
        let start_class = self.cls(i, big);
        while i < len && self.cls(i, big) == start_class && start_class != 0 {
            i += 1;
        }
        while i < len && self.cls(i, big) == 0 {
            i += 1;
        }
        i
    }
    fn word_prev(&self, from: usize, big: bool) -> usize {
        let mut i = from;
        while i > 0 && self.cls(i - 1, big) == 0 {
            i -= 1;
        }
        if i == 0 {
            return 0;
        }
        let cls = self.cls(i - 1, big);
        while i > 0 && self.cls(i - 1, big) == cls {
            i -= 1;
        }
        i
    }
    fn word_end(&self, from: usize, big: bool) -> usize {
        let len = self.buffer.len_chars();
        let mut i = (from + 1).min(len);
        while i < len && self.cls(i, big) == 0 {
            i += 1;
        }
        if i >= len {
            return len;
        }
        let cls = self.cls(i, big);
        while i + 1 < len && self.cls(i + 1, big) == cls {
            i += 1;
        }
        i
    }
    /// Vim's `ge`: the end of the word before the cursor.
    fn word_end_prev(&self, from: usize, big: bool) -> usize {
        let len = self.buffer.len_chars();
        if from == 0 || len == 0 {
            return 0;
        }
        let mut i = from.min(len - 1);
        let start = self.cls(i, big);
        if start != 0 {
            while i > 0 && self.cls(i - 1, big) == start {
                i -= 1;
            }
        }
        if i == 0 {
            return 0;
        }
        i -= 1;
        while i > 0 && self.cls(i, big) == 0 {
            i -= 1;
        }
        i
    }

    fn insert_text(&mut self, text: &str) -> bool {
        self.checkpoint(Group::Insert);
        if !self.primary().is_empty() {
            let r = self.primary().range();
            self.buf_remove(r.clone());
            self.set_caret(r.start);
        }
        let at = self.primary().head;
        self.buf_insert(at, text);
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
        self.buf_remove(r.clone());
        self.set_caret(r.start);
        true
    }

    fn line_indent(&self, line: usize) -> String {
        let base = self.buffer.line_to_char(line);
        let llen = self.buffer.line_len_chars(line);
        let mut out = String::new();
        for i in 0..llen {
            let ch = self.buffer.rope.char(base + i);
            if ch == ' ' || ch == '\t' {
                out.push(ch);
            } else {
                break;
            }
        }
        out
    }

    fn outdent_width(&self, line: usize) -> usize {
        let base = self.buffer.line_to_char(line);
        let llen = self.buffer.line_len_chars(line);
        if llen == 0 {
            return 0;
        }
        if self.buffer.rope.char(base) == '\t' {
            return 1;
        }
        let mut n = 0;
        while n < llen.min(4) && self.buffer.rope.char(base + n) == ' ' {
            n += 1;
        }
        n
    }

    fn expand_linewise(&self, r: std::ops::Range<usize>) -> std::ops::Range<usize> {
        let (l0, _) = self.buffer.char_to_coords(r.start);
        let (l1, _) = self.buffer.char_to_coords(r.end.max(r.start));
        let start = self.buffer.line_to_char(l0);
        let end = if l1 + 1 < self.buffer.len_lines() {
            self.buffer.line_to_char(l1 + 1)
        } else {
            self.buffer.len_chars()
        };
        start..end
    }

    fn line_span(&self, from: usize, count: usize) -> std::ops::Range<usize> {
        let (l0, _) = self.buffer.char_to_coords(from);
        let last = (l0 + count.max(1) - 1).min(self.buffer.len_lines().saturating_sub(1));
        let start = self.buffer.line_to_char(l0);
        let end = if last + 1 < self.buffer.len_lines() {
            self.buffer.line_to_char(last + 1)
        } else {
            self.buffer.len_chars()
        };
        start..end
    }

    fn resolve_motion_n(&self, from: usize, m: Motion, n: usize) -> usize {
        let mut at = from;
        for _ in 0..n.max(1) {
            let next = self.resolve_motion(at, m);
            if next == at {
                break;
            }
            at = next;
        }
        at
    }

    fn operator_range(&self, target: Target) -> Option<(std::ops::Range<usize>, RegisterKind)> {
        match target {
            Target::Line(count) => Some((
                self.line_span(self.primary().head, count),
                RegisterKind::Linewise,
            )),
            Target::TextObject(obj) => {
                let r = crate::edit::text_object::resolve(&self.buffer, self.primary().head, obj)?;
                if r.start >= r.end {
                    return None;
                }
                let kind = if obj.kind.is_linewise() {
                    RegisterKind::Linewise
                } else {
                    RegisterKind::Charwise
                };
                Some((r, kind))
            }
            Target::Selection => {
                let r = self.visual_range();
                if r.start >= r.end {
                    return None;
                }
                let kind = if self.mode == EditMode::VisualLine {
                    RegisterKind::Linewise
                } else {
                    RegisterKind::Charwise
                };
                Some((r, kind))
            }
            Target::Motion(m, count) => {
                let from = self.primary().head;
                let to = self.resolve_motion_n(from, m, count);
                let start = from.min(to);
                let mut end = from.max(to);
                match m.kind() {
                    MotionKind::Linewise => {
                        return Some((self.expand_linewise(start..end), RegisterKind::Linewise));
                    }
                    MotionKind::Inclusive => {
                        end = self.buffer.next_grapheme(end).min(self.buffer.len_chars());
                    }
                    MotionKind::Exclusive => {
                        let (end_line, end_col) = self.buffer.char_to_coords(end);
                        if end_col == 0 && end > start && end_line > 0 {
                            let prev = end_line - 1;
                            let prev_end =
                                self.buffer.line_to_char(prev) + self.buffer.line_len_chars(prev);
                            if prev_end >= start {
                                end = prev_end;
                                if start <= self.first_non_blank(start) {
                                    return Some((
                                        self.expand_linewise(start..end),
                                        RegisterKind::Linewise,
                                    ));
                                }
                            }
                        }
                    }
                }
                if start >= end {
                    return None;
                }
                Some((start..end, RegisterKind::Charwise))
            }
        }
    }

    /// Resolve an ex range to whole lines, including each line's newline.
    fn ex_range(&self, range: crate::edit::ex::ExRange) -> std::ops::Range<usize> {
        use crate::edit::ex::ExRange;
        match range {
            ExRange::CurrentLine => self.line_span(self.primary().head, 1),
            ExRange::WholeFile => 0..self.buffer.len_chars(),
            ExRange::Selection => self.expand_linewise(self.primary().range()),
            ExRange::Lines(first, last) => {
                let total = self.buffer.len_lines().saturating_sub(1);
                let (first, last) = (first.min(total), last.min(total));
                let (first, last) = (first.min(last), first.max(last));
                let start = self.buffer.line_to_char(first);
                let end = if last + 1 < self.buffer.len_lines() {
                    self.buffer.line_to_char(last + 1)
                } else {
                    self.buffer.len_chars()
                };
                start..end
            }
        }
    }

    fn substitute(
        &mut self,
        range: crate::edit::ex::ExRange,
        pattern: &str,
        replacement: &str,
        all: bool,
    ) -> bool {
        let span = self.ex_range(range);
        if span.start >= span.end {
            return false;
        }
        let Ok(re) = regex::Regex::new(&crate::edit::search::translate(pattern)) else {
            return false;
        };
        let source: String = self.buffer.rope.slice(span.clone()).chars().collect();
        let replacement = replacement.replace('&', "${0}");
        let out = if all {
            re.replace_all(&source, replacement.as_str()).into_owned()
        } else {
            // Vim substitutes the first match on each line unless the `g` flag is given.
            source
                .split_inclusive('\n')
                .map(|line| re.replace(line, replacement.as_str()).into_owned())
                .collect()
        };
        if out == source {
            return false;
        }
        self.checkpoint(Group::Other);
        self.buf_remove(span.clone());
        self.buf_insert(span.start, &out);
        let at = (span.start + out.chars().count()).min(self.buffer.len_chars());
        self.set_caret(self.first_non_blank(at.saturating_sub(1)));
        true
    }

    fn apply_operator(
        &mut self,
        operator: Operator,
        target: Target,
        register: Option<char>,
    ) -> (bool, Option<RegisterValue>) {
        let Some((range, kind)) = self.operator_range(target) else {
            return (false, None);
        };
        let was_visual = self.mode.is_visual();
        match operator {
            Operator::Yank => {
                let text: String = self.buffer.rope.slice(range.clone()).chars().collect();
                let value = RegisterValue { text, kind };
                self.registers.write_yank(register, value.clone());
                if was_visual {
                    self.mode = EditMode::Normal;
                }
                self.set_caret(range.start);
                (false, Some(value))
            }
            Operator::Delete | Operator::Change => {
                let text: String = self.buffer.rope.slice(range.clone()).chars().collect();
                let had_newline = text.ends_with('\n');
                let value = RegisterValue { text, kind };
                self.registers.write_delete(register, value.clone());
                self.checkpoint(Group::Other);
                if operator == Operator::Change && kind == RegisterKind::Linewise {
                    let (line, _) = self.buffer.char_to_coords(range.start);
                    let indent = self.line_indent(line);
                    self.buf_remove(range.clone());
                    self.mode = EditMode::Insert;
                    let tail = if had_newline { "\n" } else { "" };
                    self.buf_insert(range.start, &format!("{indent}{tail}"));
                    self.set_caret(range.start + indent.chars().count());
                } else {
                    self.buf_remove(range.clone());
                    if operator == Operator::Change {
                        self.mode = EditMode::Insert;
                    } else if was_visual {
                        self.mode = EditMode::Normal;
                    }
                    let at = range.start.min(self.buffer.len_chars());
                    let caret = if kind == RegisterKind::Linewise {
                        self.first_non_blank(at)
                    } else {
                        at
                    };
                    self.set_caret(caret);
                }
                (true, Some(value))
            }
            Operator::Indent | Operator::Outdent => {
                let range = self.expand_linewise(range);
                let (l0, _) = self.buffer.char_to_coords(range.start);
                let (l1, _) = self
                    .buffer
                    .char_to_coords(range.end.saturating_sub(1).max(range.start));
                self.checkpoint(Group::Other);
                for line in (l0..=l1).rev() {
                    let base = self.buffer.line_to_char(line);
                    if operator == Operator::Indent {
                        if self.buffer.line_len_chars(line) == 0 {
                            continue;
                        }
                        self.buf_insert(base, "\t");
                    } else {
                        let width = self.outdent_width(line);
                        if width > 0 {
                            self.buf_remove(base..base + width);
                        }
                    }
                }
                if was_visual {
                    self.mode = EditMode::Normal;
                }
                let caret = self.first_non_blank(self.buffer.line_to_char(l0));
                self.set_caret(caret);
                (true, None)
            }
            Operator::Upper | Operator::Lower | Operator::ToggleCase => {
                let src: String = self.buffer.rope.slice(range.clone()).chars().collect();
                let out: String = src
                    .chars()
                    .map(|c| match operator {
                        Operator::Upper => c.to_uppercase().next().unwrap_or(c),
                        Operator::Lower => c.to_lowercase().next().unwrap_or(c),
                        _ if c.is_uppercase() => c.to_lowercase().next().unwrap_or(c),
                        _ => c.to_uppercase().next().unwrap_or(c),
                    })
                    .collect();
                if was_visual {
                    self.mode = EditMode::Normal;
                }
                if out == src {
                    self.set_caret(range.start);
                    return (false, None);
                }
                self.checkpoint(Group::Other);
                self.buf_remove(range.clone());
                self.buf_insert(range.start, &out);
                self.set_caret(range.start);
                (true, None)
            }
        }
    }

    fn apply_put(&mut self, before: bool, count: usize, register: Option<char>) -> bool {
        let Some(value) = self.registers.read(register).cloned() else {
            return false;
        };
        let was_visual = self.mode.is_visual();
        if was_visual {
            let range = self.visual_range();
            let kind = if self.mode == EditMode::VisualLine {
                RegisterKind::Linewise
            } else {
                RegisterKind::Charwise
            };
            if range.start < range.end {
                let text: String = self.buffer.rope.slice(range.clone()).chars().collect();
                self.registers
                    .write_delete(None, RegisterValue { text, kind });
                self.checkpoint(Group::Other);
                self.buf_remove(range.clone());
            }
            self.mode = EditMode::Normal;
            self.set_caret(range.start.min(self.buffer.len_chars()));
        }
        let count = count.max(1);
        match value.kind {
            RegisterKind::Charwise => {
                let head = self.primary().head;
                let line_end = self.resolve_motion(head, Motion::LineEnd);
                let at = if before || was_visual {
                    head
                } else {
                    self.buffer.next_grapheme(head).min(line_end)
                };
                let text = value.text.repeat(count);
                self.checkpoint(Group::Other);
                self.buf_insert(at, &text);
                let end = at + text.chars().count();
                self.set_caret(end.saturating_sub(1).max(at));
            }
            RegisterKind::Linewise => {
                let head = self.primary().head;
                let (line, _) = self.buffer.char_to_coords(head);
                let mut text = value.text.repeat(count);
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                let mut at = if before {
                    self.buffer.line_to_char(line)
                } else if line + 1 < self.buffer.len_lines() {
                    self.buffer.line_to_char(line + 1)
                } else {
                    self.buffer.len_chars()
                };
                self.checkpoint(Group::Other);
                if at == self.buffer.len_chars() && at > 0 && self.buffer.rope.char(at - 1) != '\n'
                {
                    self.buf_insert(at, "\n");
                    at += 1;
                }
                self.buf_insert(at, &text);
                self.set_caret(self.first_non_blank(at));
            }
        }
        true
    }

    pub fn apply(&mut self, cmd: EditCommand) -> EditOutcome {
        let before_sel = self.primary();
        let before_mode = self.mode;
        let before_dirty = self.dirty;
        let mut text_changed = false;
        let mut yank: Option<RegisterValue> = None;

        if !matches!(
            &cmd,
            EditCommand::Move(Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown)
                | EditCommand::Select(
                    Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown
                )
                | EditCommand::ScrollViewport(_)
        ) {
            self.preferred_vertical_col = None;
        }

        match cmd {
            EditCommand::Move(m) => {
                self.break_group();
                if m.is_jump() {
                    self.push_jump();
                }
                let selection = self.primary();
                let collapse_selection = !self.mode.is_visual() && !selection.is_empty();
                let h = if collapse_selection {
                    self.preferred_vertical_col = None;
                    match m.collapse_to_start() {
                        Some(true) => selection.range().start,
                        Some(false) => selection.range().end,
                        None => self.resolve_motion(selection.head, m),
                    }
                } else {
                    self.resolve_navigation_motion(selection.head, m)
                };
                if self.mode.is_visual() {
                    self.set_head(h);
                } else {
                    self.place_caret(h);
                }
            }
            EditCommand::Select(m) => {
                self.break_group();
                let h = self.resolve_navigation_motion(self.primary().head, m);
                self.set_head(h);
            }
            EditCommand::InsertText(t) => text_changed = self.insert_text(&t),
            EditCommand::OvertypeText(t) => {
                self.checkpoint(Group::Insert);
                for ch in t.chars() {
                    let at = self.primary().head;
                    let line_end = self.resolve_motion(at, Motion::LineEnd);
                    if at < line_end {
                        self.replaced.push(Some(self.buffer.rope.char(at)));
                        self.buf_remove(at..at + 1);
                    } else {
                        self.replaced.push(None);
                    }
                    self.buf_insert(at, &ch.to_string());
                    self.set_caret(at + 1);
                }
                text_changed = true;
            }
            EditCommand::ReplaceText(t) => {
                let caret = self.primary().head;
                let next_caret = translated_caret_after_replace(&self.buffer.text(), &t, caret);
                self.checkpoint(Group::Other);
                self.buffer.remove(0..self.buffer.len_chars());
                self.buffer.insert(0, &t);
                self.marks.clear();
                self.set_caret(next_caret);
                text_changed = true;
            }
            EditCommand::InsertTab => text_changed = self.insert_text("\t"),
            EditCommand::InsertNewline => text_changed = self.insert_text("\n"),
            EditCommand::DeleteBack if self.mode == EditMode::Replace => {
                let head = self.primary().head;
                if let Some(original) = self.replaced.pop()
                    && head > 0
                {
                    self.checkpoint(Group::Delete);
                    self.buf_remove(head - 1..head);
                    if let Some(ch) = original {
                        self.buf_insert(head - 1, &ch.to_string());
                    }
                    self.set_caret(head - 1);
                    text_changed = true;
                }
            }
            EditCommand::DeleteBack => {
                if self.primary().is_empty() {
                    let head = self.primary().head;
                    if head > 0 {
                        self.checkpoint(Group::Delete);
                        let prev = self.buffer.prev_grapheme(head);
                        self.buf_remove(prev..head);
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
                        self.buf_remove(head..next);
                        text_changed = true;
                    }
                }
            }
            EditCommand::DeleteWordBack => {
                let head = self.primary().head;
                let target = self.word_prev(head, false);
                if target < head {
                    self.checkpoint(Group::Delete);
                    self.buf_remove(target..head);
                    self.set_caret(target);
                    text_changed = true;
                }
            }
            EditCommand::Op {
                operator,
                target,
                register,
            } => {
                let (changed, value) = self.apply_operator(operator, target, register);
                text_changed = changed;
                yank = value;
            }
            EditCommand::Put {
                before,
                count,
                register,
            } => text_changed = self.apply_put(before, count, register),
            EditCommand::ReplaceChar { ch, count } => {
                let head = self.primary().head;
                let line_end = self.resolve_motion(head, Motion::LineEnd);
                let mut end = head;
                let mut fits = true;
                for _ in 0..count.max(1) {
                    end = self.buffer.next_grapheme(end);
                    if end > line_end {
                        fits = false;
                        break;
                    }
                }
                if fits && end > head {
                    let n = self.buffer.rope.slice(head..end).chars().count();
                    self.checkpoint(Group::Other);
                    self.buf_remove(head..end);
                    self.buf_insert(head, &ch.to_string().repeat(n));
                    self.set_caret(head + n - 1);
                    text_changed = true;
                }
            }
            EditCommand::JoinLines { count, spaces } => {
                for _ in 0..count.max(2) - 1 {
                    let (line, _) = self.buffer.char_to_coords(self.primary().head);
                    if line + 1 >= self.buffer.len_lines() {
                        break;
                    }
                    let start = self.buffer.line_to_char(line);
                    let end = start + self.buffer.line_len_chars(line);
                    let next = self.buffer.line_to_char(line + 1);
                    let next_len = self.buffer.line_len_chars(line + 1);
                    let mut skip = 0;
                    while skip < next_len {
                        let c = self.buffer.rope.char(next + skip);
                        if c == ' ' || c == '\t' {
                            skip += 1;
                        } else {
                            break;
                        }
                    }
                    if !text_changed {
                        self.checkpoint(Group::Other);
                    }
                    self.buf_remove(end..next + skip);
                    if spaces
                        && end > start
                        && end < self.buffer.len_chars()
                        && self.buffer.rope.char(end) != '\n'
                        && self.buffer.rope.char(end) != ')'
                        && self.buffer.rope.char(end - 1) != ' '
                    {
                        self.buf_insert(end, " ");
                    }
                    self.set_caret(end);
                    text_changed = true;
                }
            }
            EditCommand::OpenLine { above } => {
                let head = self.primary().head;
                let (line, _) = self.buffer.char_to_coords(head);
                let indent = self.line_indent(line);
                self.checkpoint(Group::Other);
                self.mode = EditMode::Insert;
                if above {
                    let at = self.buffer.line_to_char(line);
                    self.buf_insert(at, &format!("{indent}\n"));
                    self.set_caret(at + indent.chars().count());
                } else {
                    let at = self.buffer.line_to_char(line) + self.buffer.line_len_chars(line);
                    self.buf_insert(at, &format!("\n{indent}"));
                    self.set_caret(at + 1 + indent.chars().count());
                }
                text_changed = true;
            }
            EditCommand::SetMode(m) => {
                self.break_group();
                self.mode = m;
                if m == EditMode::Normal {
                    self.set_caret(self.primary().head);
                }
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
            EditCommand::SetSearch { pattern, forward } => {
                if let Some(search) = crate::edit::search::Search::new(&pattern, forward) {
                    self.search = Some(search);
                    self.search_highlight = true;
                    self.push_jump();
                    if let Some(at) = self.search_step(self.primary().head, false) {
                        self.set_caret(at);
                    }
                }
            }
            EditCommand::SearchWord { forward } => {
                if let Some(word) = self.word_under_cursor() {
                    let pattern = format!("\\<{}\\>", regex::escape(&word));
                    if let Some(search) = crate::edit::search::Search::new(&pattern, forward) {
                        self.search = Some(search);
                        self.search_highlight = true;
                        self.push_jump();
                        if let Some(at) = self.search_step(self.primary().head, false) {
                            self.set_caret(at);
                        }
                    }
                }
            }
            EditCommand::ClearSearchHighlight => self.search_highlight = false,
            EditCommand::Substitute {
                range,
                pattern,
                replacement,
                all,
            } => {
                text_changed = self.substitute(range, &pattern, &replacement, all);
            }
            EditCommand::ExDelete(range) => {
                let span = self.ex_range(range);
                if span.start < span.end {
                    let text: String = self.buffer.rope.slice(span.clone()).chars().collect();
                    self.registers
                        .write_delete(None, RegisterValue::linewise(text));
                    self.checkpoint(Group::Other);
                    self.buf_remove(span.clone());
                    let at = span.start.min(self.buffer.len_chars());
                    self.set_caret(self.first_non_blank(at));
                    text_changed = true;
                }
            }
            EditCommand::ExYank(range) => {
                let span = self.ex_range(range);
                if span.start < span.end {
                    let text: String = self.buffer.rope.slice(span).chars().collect();
                    let value = RegisterValue::linewise(text);
                    self.registers.write_yank(None, value.clone());
                    yank = Some(value);
                }
            }
            EditCommand::SetMark(name) => {
                self.marks.insert(name, self.primary().head);
            }
            EditCommand::GotoMark { name, linewise } => {
                if let Some(&at) = self.marks.get(&name) {
                    let at = at.min(self.buffer.len_chars());
                    self.push_jump();
                    let target = if linewise {
                        self.first_non_blank(at)
                    } else {
                        at
                    };
                    if self.mode.is_visual() {
                        self.set_head(target);
                    } else {
                        self.set_caret(target);
                    }
                }
            }
            EditCommand::JumpList { back, count } => self.jump(back, count),
            EditCommand::ChangeList { back, count } => {
                if !self.changes.is_empty() {
                    let last = self.changes.len() - 1;
                    self.change_index = if back {
                        self.change_index.saturating_sub(count.max(1))
                    } else {
                        (self.change_index + count.max(1)).min(last)
                    };
                    let at = self.changes[self.change_index.min(last)];
                    self.set_caret(at.min(self.buffer.len_chars()));
                }
            }
            EditCommand::SwapSelectionEnds => {
                let sel = self.primary();
                self.selections = vec![Selection {
                    anchor: sel.head,
                    head: sel.anchor,
                }];
            }
            EditCommand::SelectTextObject(obj) => {
                if let Some(r) =
                    crate::edit::text_object::resolve(&self.buffer, self.primary().head, obj)
                    && r.start < r.end
                {
                    self.selections = vec![Selection {
                        anchor: r.start,
                        head: self.buffer.prev_grapheme(r.end),
                    }];
                }
            }
            EditCommand::Save
            | EditCommand::ScrollViewport(_)
            | EditCommand::ScrollCursorTo(_)
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
    fn op(operator: Operator, target: Target) -> EditCommand {
        EditCommand::Op {
            operator,
            target,
            register: None,
        }
    }
    fn put(before: bool) -> EditCommand {
        EditCommand::Put {
            before,
            count: 1,
            register: None,
        }
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
    fn bounded_horizontal_motion_stays_on_the_current_line() {
        let mut c = core("ab\ncd");
        c.set_caret(c.buffer.coords_to_char(1, 0));
        c.apply(EditCommand::Move(Motion::LeftBounded));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 0));

        c.set_caret(c.buffer.coords_to_char(0, 1));
        c.apply(EditCommand::Move(Motion::RightBounded));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 1));
    }

    #[test]
    fn normal_mode_clamps_every_cursor_target_to_a_line_cell() {
        let mut c = core("ab\ncd");
        c.mode = EditMode::Normal;

        c.set_caret(c.buffer.coords_to_char(0, 0));
        c.apply(EditCommand::Move(Motion::LineEnd));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 1));

        c.apply(EditCommand::Move(Motion::DocEnd));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 1));

        c.apply(EditCommand::Move(Motion::WordNext));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 1));

        c.mode = EditMode::Insert;
        c.set_caret(c.buffer.coords_to_char(0, 2));
        c.apply(EditCommand::SetMode(EditMode::Normal));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 1));
    }

    #[test]
    fn paragraph_motion_moves_between_visible_paragraph_starts() {
        let mut c = core("one\ntwo\n\nthree\nfour\n\nfive\n");
        c.set_caret(c.buffer.coords_to_char(1, 2));
        c.apply(EditCommand::Move(Motion::ParagraphNext));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (3, 0));
        c.apply(EditCommand::Move(Motion::ParagraphPrev));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 0));

        c.apply(EditCommand::Move(Motion::ParagraphPrev));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 0));

        c.set_caret(c.buffer.coords_to_char(6, 0));
        c.apply(EditCommand::Move(Motion::ParagraphNext));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (6, 0));
    }

    #[test]
    fn visual_delete_covers_the_character_under_the_cursor() {
        let mut c = core("abcdef");
        c.set_caret(1);
        c.mode = EditMode::Visual;
        c.apply(EditCommand::Select(Motion::Right));
        c.apply(EditCommand::Select(Motion::Right));
        c.apply(op(Operator::Delete, Target::Selection));
        assert_eq!(text_of(&c), "aef");
    }

    #[test]
    fn delete_range_word() {
        let mut c = core("foo bar");
        c.set_caret(0);
        c.apply(op(Operator::Delete, Target::Motion(Motion::WordNext, 1)));
        assert_eq!(text_of(&c), "bar");
    }

    #[test]
    fn find_char_lands_on_or_before_the_target() {
        let mut c = core("alpha, beta, gamma");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Move(Motion::FindChar {
            ch: ',',
            forward: true,
            till: false,
        }));
        assert_eq!(c.primary().head, 5);
        c.set_caret(0);
        c.apply(EditCommand::Move(Motion::FindChar {
            ch: ',',
            forward: true,
            till: true,
        }));
        assert_eq!(c.primary().head, 4);
    }

    #[test]
    fn find_char_stays_on_the_cursor_line() {
        let mut c = core("abc\nx,y\n");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Move(Motion::FindChar {
            ch: ',',
            forward: true,
            till: false,
        }));
        assert_eq!(c.primary().head, 0);
    }

    #[test]
    fn forward_find_is_inclusive_as_an_operator_target() {
        let mut c = core("alpha, beta");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(op(
            Operator::Delete,
            Target::Motion(
                Motion::FindChar {
                    ch: ',',
                    forward: true,
                    till: true,
                },
                1,
            ),
        ));
        assert_eq!(text_of(&c), ", beta");
    }

    #[test]
    fn counted_find_reaches_the_nth_match() {
        let mut c = core("a,b,c,d");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(op(
            Operator::Delete,
            Target::Motion(
                Motion::FindChar {
                    ch: ',',
                    forward: true,
                    till: false,
                },
                2,
            ),
        ));
        assert_eq!(text_of(&c), "c,d");
    }

    #[test]
    fn match_pair_jumps_both_directions() {
        let mut c = core("foo(bar[baz])");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Move(Motion::MatchPair));
        assert_eq!(c.primary().head, 12);
        c.apply(EditCommand::Move(Motion::MatchPair));
        assert_eq!(c.primary().head, 3);
    }

    #[test]
    fn big_word_motions_span_punctuation() {
        let mut c = core("foo.bar baz");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Move(Motion::BigWordNext));
        assert_eq!(c.primary().head, 8);
        c.apply(EditCommand::Move(Motion::BigWordPrev));
        assert_eq!(c.primary().head, 0);
        c.apply(EditCommand::Move(Motion::BigWordEnd));
        assert_eq!(c.primary().head, 6);
    }

    #[test]
    fn word_end_prev_walks_back_to_the_previous_word() {
        let mut c = core("one two three");
        c.mode = EditMode::Normal;
        c.set_caret(8);
        c.apply(EditCommand::Move(Motion::WordEndPrev));
        assert_eq!(c.primary().head, 6);
    }

    #[test]
    fn screen_motions_use_the_viewport_top() {
        let mut c = core("a\nb\nc\nd\ne\nf\ng\n");
        c.mode = EditMode::Normal;
        c.rows = 4;
        c.top_row = 2;
        c.apply(EditCommand::Move(Motion::ScreenTop));
        assert_eq!(c.buffer.char_to_coords(c.primary().head).0, 2);
        c.apply(EditCommand::Move(Motion::ScreenBottom));
        assert_eq!(c.buffer.char_to_coords(c.primary().head).0, 5);
        c.apply(EditCommand::Move(Motion::ScreenMiddle));
        assert_eq!(c.buffer.char_to_coords(c.primary().head).0, 3);
    }

    #[test]
    fn column_motion_is_one_based() {
        let mut c = core("abcdef");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Move(Motion::Column(4)));
        assert_eq!(c.primary().head, 3);
    }

    #[test]
    fn overtype_replaces_characters_and_backspace_restores_them() {
        let mut c = core("abcd");
        c.mode = EditMode::Replace;
        c.set_caret(0);
        c.apply(EditCommand::OvertypeText("XY".into()));
        assert_eq!(text_of(&c), "XYcd");
        c.apply(EditCommand::DeleteBack);
        assert_eq!(text_of(&c), "Xbcd");
        c.apply(EditCommand::DeleteBack);
        assert_eq!(text_of(&c), "abcd");
    }

    #[test]
    fn overtype_past_the_line_end_appends() {
        let mut c = core("ab\ncd\n");
        c.mode = EditMode::Replace;
        c.set_caret(2);
        c.apply(EditCommand::OvertypeText("XY".into()));
        assert_eq!(text_of(&c), "abXY\ncd\n");
    }

    #[test]
    fn substitute_replaces_the_first_match_per_line_without_g() {
        let mut c = core("aa bb aa\naa cc\n");
        c.mode = EditMode::Normal;
        c.apply(EditCommand::Substitute {
            range: crate::edit::ex::ExRange::WholeFile,
            pattern: "aa".into(),
            replacement: "X".into(),
            all: false,
        });
        assert_eq!(text_of(&c), "X bb aa\nX cc\n");
    }

    #[test]
    fn substitute_with_g_replaces_every_match() {
        let mut c = core("aa bb aa\n");
        c.mode = EditMode::Normal;
        c.apply(EditCommand::Substitute {
            range: crate::edit::ex::ExRange::WholeFile,
            pattern: "aa".into(),
            replacement: "X".into(),
            all: true,
        });
        assert_eq!(text_of(&c), "X bb X\n");
    }

    #[test]
    fn substitute_honours_a_line_range() {
        let mut c = core("a\na\na\n");
        c.mode = EditMode::Normal;
        c.apply(EditCommand::Substitute {
            range: crate::edit::ex::ExRange::Lines(1, 1),
            pattern: "a".into(),
            replacement: "b".into(),
            all: false,
        });
        assert_eq!(text_of(&c), "a\nb\na\n");
    }

    #[test]
    fn substitute_expands_ampersand_to_the_match() {
        let mut c = core("cat\n");
        c.mode = EditMode::Normal;
        c.apply(EditCommand::Substitute {
            range: crate::edit::ex::ExRange::WholeFile,
            pattern: "cat".into(),
            replacement: "[&]".into(),
            all: false,
        });
        assert_eq!(text_of(&c), "[cat]\n");
    }

    #[test]
    fn ex_delete_removes_lines_and_yanks_them_linewise() {
        let mut c = core("one\ntwo\nthree\n");
        c.mode = EditMode::Normal;
        c.apply(EditCommand::ExDelete(crate::edit::ex::ExRange::Lines(0, 1)));
        assert_eq!(text_of(&c), "three\n");
        assert_eq!(
            c.registers.read(None),
            Some(&RegisterValue::linewise("one\ntwo\n"))
        );
    }

    #[test]
    fn search_jumps_to_the_next_match_and_wraps() {
        let mut c = core("alpha beta alpha gamma");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::SetSearch {
            pattern: "alpha".into(),
            forward: true,
        });
        assert_eq!(c.primary().head, 11);
        c.apply(EditCommand::Move(Motion::SearchNext { reverse: false }));
        assert_eq!(c.primary().head, 0);
        c.apply(EditCommand::Move(Motion::SearchNext { reverse: true }));
        assert_eq!(c.primary().head, 11);
    }

    #[test]
    fn a_backward_search_reverses_what_n_means() {
        let mut c = core("x a x a x");
        c.mode = EditMode::Normal;
        c.set_caret(4);
        c.apply(EditCommand::SetSearch {
            pattern: "x".into(),
            forward: false,
        });
        assert_eq!(c.primary().head, 0);
        c.apply(EditCommand::Move(Motion::SearchNext { reverse: false }));
        assert_eq!(c.primary().head, 8);
    }

    #[test]
    fn star_searches_the_whole_word_under_the_cursor() {
        let mut c = core("cat category cat");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::SearchWord { forward: true });
        assert_eq!(c.primary().head, 13);
    }

    #[test]
    fn search_composes_with_an_operator() {
        let mut c = core("one two three");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::SetSearch {
            pattern: "three".into(),
            forward: true,
        });
        c.set_caret(0);
        c.apply(op(
            Operator::Delete,
            Target::Motion(Motion::SearchNext { reverse: false }, 1),
        ));
        assert_eq!(text_of(&c), "three");
    }

    #[test]
    fn an_invalid_pattern_leaves_the_caret_alone() {
        let mut c = core("abc");
        c.mode = EditMode::Normal;
        c.set_caret(1);
        c.apply(EditCommand::SetSearch {
            pattern: "\\v(".into(),
            forward: true,
        });
        assert_eq!(c.primary().head, 1);
        assert!(c.search.is_none());
    }

    #[test]
    fn highlight_spans_appear_only_while_highlighting_is_on() {
        let mut c = core("foo bar foo\n");
        c.mode = EditMode::Normal;
        c.apply(EditCommand::SetSearch {
            pattern: "foo".into(),
            forward: true,
        });
        assert_eq!(c.search_spans(0, 4).len(), 2);
        c.apply(EditCommand::ClearSearchHighlight);
        assert!(c.search_spans(0, 4).is_empty());
    }

    #[test]
    fn marks_return_to_a_saved_position() {
        let mut c = core("one\ntwo\nthree\n");
        c.mode = EditMode::Normal;
        c.set_caret(c.buffer.coords_to_char(1, 1));
        c.apply(EditCommand::SetMark('a'));
        c.set_caret(0);
        c.apply(EditCommand::GotoMark {
            name: 'a',
            linewise: false,
        });
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 1));
    }

    #[test]
    fn a_linewise_mark_lands_on_the_first_non_blank() {
        let mut c = core("one\n    two\n");
        c.mode = EditMode::Normal;
        c.set_caret(c.buffer.coords_to_char(1, 7));
        c.apply(EditCommand::SetMark('b'));
        c.set_caret(0);
        c.apply(EditCommand::GotoMark {
            name: 'b',
            linewise: true,
        });
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 4));
    }

    #[test]
    fn marks_slide_when_text_is_inserted_before_them() {
        let mut c = core("one\ntwo\n");
        c.mode = EditMode::Normal;
        c.set_caret(c.buffer.coords_to_char(1, 0));
        c.apply(EditCommand::SetMark('a'));
        c.set_caret(0);
        c.apply(EditCommand::OpenLine { above: true });
        c.apply(EditCommand::InsertText("zero".into()));
        c.apply(EditCommand::SetMode(EditMode::Normal));
        c.apply(EditCommand::GotoMark {
            name: 'a',
            linewise: false,
        });
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (2, 0));
    }

    #[test]
    fn an_unset_mark_does_not_move_the_caret() {
        let mut c = core("one\ntwo\n");
        c.mode = EditMode::Normal;
        c.set_caret(1);
        c.apply(EditCommand::GotoMark {
            name: 'z',
            linewise: false,
        });
        assert_eq!(c.primary().head, 1);
    }

    #[test]
    fn the_jump_list_walks_back_and_forward() {
        let mut c = core("a\nb\nc\nd\ne\n");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Move(Motion::DocEnd));
        let end = c.primary().head;
        c.apply(EditCommand::JumpList {
            back: true,
            count: 1,
        });
        assert_eq!(c.primary().head, 0);
        c.apply(EditCommand::JumpList {
            back: false,
            count: 1,
        });
        assert_eq!(c.primary().head, end);
    }

    #[test]
    fn plain_motions_do_not_record_jumps() {
        let mut c = core("a\nb\nc\n");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Move(Motion::Down));
        c.apply(EditCommand::JumpList {
            back: true,
            count: 1,
        });
        assert_eq!(c.buffer.char_to_coords(c.primary().head).0, 1);
    }

    #[test]
    fn the_change_list_revisits_edit_positions() {
        let mut c = core("one\ntwo\nthree\n");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::ReplaceChar { ch: 'X', count: 1 });
        c.set_caret(c.buffer.coords_to_char(2, 0));
        c.apply(EditCommand::ReplaceChar { ch: 'Y', count: 1 });
        c.set_caret(c.buffer.coords_to_char(1, 0));
        c.apply(EditCommand::ChangeList {
            back: true,
            count: 1,
        });
        assert_eq!(c.buffer.char_to_coords(c.primary().head).0, 0);
    }

    #[test]
    fn operator_counts_multiply_the_motion() {
        let mut c = core("one two three four");
        c.set_caret(0);
        c.apply(op(Operator::Delete, Target::Motion(Motion::WordNext, 3)));
        assert_eq!(text_of(&c), "four");
    }

    #[test]
    fn inclusive_motion_covers_its_last_character() {
        let mut c = core("foo bar");
        c.set_caret(0);
        c.apply(op(Operator::Delete, Target::Motion(Motion::WordEnd, 1)));
        assert_eq!(text_of(&c), " bar");
    }

    #[test]
    fn linewise_motion_deletes_whole_lines() {
        let mut c = core("one\ntwo\nthree\n");
        c.set_caret(c.buffer.coords_to_char(0, 1));
        c.apply(op(Operator::Delete, Target::Motion(Motion::Down, 1)));
        assert_eq!(text_of(&c), "three\n");
    }

    #[test]
    fn exclusive_motion_ending_in_column_one_stops_at_the_previous_line() {
        let mut c = core("foo\nbar\n");
        c.set_caret(1);
        c.apply(op(Operator::Delete, Target::Motion(Motion::WordNext, 1)));
        assert_eq!(text_of(&c), "f\nbar\n");
    }

    #[test]
    fn delete_line_yanks_linewise_and_put_opens_a_new_line() {
        let mut c = core("one\ntwo\nthree\n");
        c.mode = EditMode::Normal;
        c.set_caret(c.buffer.coords_to_char(1, 0));
        c.apply(op(Operator::Delete, Target::Line(1)));
        assert_eq!(text_of(&c), "one\nthree\n");
        assert_eq!(
            c.registers.read(None),
            Some(&RegisterValue::linewise("two\n"))
        );
        c.apply(put(false));
        assert_eq!(text_of(&c), "one\nthree\ntwo\n");
    }

    #[test]
    fn count_deletes_multiple_lines() {
        let mut c = core("a\nb\nc\nd\n");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(op(Operator::Delete, Target::Line(3)));
        assert_eq!(text_of(&c), "d\n");
    }

    #[test]
    fn change_line_keeps_indentation() {
        let mut c = core("    foo\nbar\n");
        c.mode = EditMode::Normal;
        c.set_caret(c.buffer.coords_to_char(0, 4));
        c.apply(op(Operator::Change, Target::Line(1)));
        assert_eq!(text_of(&c), "    \nbar\n");
        assert_eq!(c.primary().head, 4);
        assert_eq!(c.mode, EditMode::Insert);
    }

    #[test]
    fn indent_and_outdent_shift_whole_lines() {
        let mut c = core("a\nb\n");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(op(Operator::Indent, Target::Line(2)));
        assert_eq!(text_of(&c), "\ta\n\tb\n");
        c.apply(op(Operator::Outdent, Target::Line(2)));
        assert_eq!(text_of(&c), "a\nb\n");
    }

    #[test]
    fn case_operators_transform_a_range() {
        let mut c = core("hello");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(op(Operator::Upper, Target::Motion(Motion::LineEnd, 1)));
        assert_eq!(text_of(&c), "HELLO");
        c.apply(op(Operator::ToggleCase, Target::Motion(Motion::LineEnd, 1)));
        assert_eq!(text_of(&c), "hello");
    }

    #[test]
    fn replace_char_overwrites_without_entering_insert() {
        let mut c = core("abcd");
        c.mode = EditMode::Normal;
        c.set_caret(1);
        c.apply(EditCommand::ReplaceChar { ch: 'X', count: 2 });
        assert_eq!(text_of(&c), "aXXd");
        assert_eq!(c.primary().head, 2);
    }

    #[test]
    fn replace_char_past_the_line_end_is_rejected() {
        let mut c = core("ab\ncd\n");
        c.mode = EditMode::Normal;
        c.set_caret(1);
        c.apply(EditCommand::ReplaceChar { ch: 'X', count: 4 });
        assert_eq!(text_of(&c), "ab\ncd\n");
    }

    #[test]
    fn join_lines_collapses_indentation_to_one_space() {
        let mut c = core("foo\n    bar\nbaz\n");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::JoinLines {
            count: 2,
            spaces: true,
        });
        assert_eq!(text_of(&c), "foo bar\nbaz\n");
    }

    #[test]
    fn join_without_spaces_keeps_text_adjacent() {
        let mut c = core("foo\n  bar\n");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::JoinLines {
            count: 2,
            spaces: false,
        });
        assert_eq!(text_of(&c), "foobar\n");
    }

    #[test]
    fn open_line_inherits_indentation() {
        let mut c = core("    foo\n");
        c.mode = EditMode::Normal;
        c.set_caret(4);
        c.apply(EditCommand::OpenLine { above: false });
        assert_eq!(text_of(&c), "    foo\n    \n");
        assert_eq!(c.mode, EditMode::Insert);

        let mut c = core("    foo\n");
        c.mode = EditMode::Normal;
        c.set_caret(4);
        c.apply(EditCommand::OpenLine { above: true });
        assert_eq!(text_of(&c), "    \n    foo\n");
    }

    #[test]
    fn linewise_put_before_inserts_above_the_current_line() {
        let mut c = core("one\ntwo\n");
        c.mode = EditMode::Normal;
        c.registers.set_unnamed(RegisterValue::linewise("new\n"));
        c.set_caret(c.buffer.coords_to_char(1, 0));
        c.apply(put(true));
        assert_eq!(text_of(&c), "one\nnew\ntwo\n");
    }

    #[test]
    fn put_repeats_with_a_count() {
        let mut c = core("a");
        c.mode = EditMode::Normal;
        c.registers.set_unnamed(RegisterValue::charwise("X"));
        c.set_caret(0);
        c.apply(EditCommand::Put {
            before: false,
            count: 3,
            register: None,
        });
        assert_eq!(text_of(&c), "aXXX");
    }

    #[test]
    fn named_register_round_trips_through_an_operator() {
        let mut c = core("one\ntwo\n");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Op {
            operator: Operator::Yank,
            target: Target::Line(1),
            register: Some('a'),
        });
        c.set_caret(c.buffer.coords_to_char(1, 0));
        c.apply(EditCommand::Put {
            before: false,
            count: 1,
            register: Some('a'),
        });
        assert_eq!(text_of(&c), "one\ntwo\none\n");
    }

    #[test]
    fn cursor_pos_visual_col_for_wide_chars() {
        let mut c = core("あb");
        c.set_caret(1);
        assert_eq!(
            c.cursor_pos(),
            CursorPos {
                line: 0,
                row: 0,
                col: 2
            }
        );
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
    fn replace_text_is_one_undoable_edit() {
        let mut c = core("old");
        c.set_caret(2);
        c.apply(EditCommand::ReplaceText("new text".into()));
        assert_eq!(text_of(&c), "new text");
        assert_eq!(c.primary().head, 2);
        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "old");
    }

    #[test]
    fn replace_text_preserves_caret_in_an_unchanged_body() {
        let old = "---\ntitle: Old\n---\nBody text";
        let new = "---\ntitle: Old\ntags:\n  - note\n---\nBody text";
        let mut c = core(old);
        c.set_caret(old.find("text").unwrap());
        c.apply(EditCommand::ReplaceText(new.into()));
        assert_eq!(c.primary().head, new.find("text").unwrap());
    }

    #[test]
    fn yank_and_paste() {
        let mut c = core("abcdef");
        c.set_caret(0);
        c.mode = EditMode::Visual;
        c.apply(EditCommand::Select(Motion::Right));
        c.apply(EditCommand::Select(Motion::Right));
        let out = c.apply(op(Operator::Yank, Target::Selection));
        assert_eq!(out.yank, Some(RegisterValue::charwise("abc")));
        c.mode = EditMode::Insert;
        c.set_caret(6);
        c.apply(put(true));
        assert_eq!(text_of(&c), "abcdefabc");
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
        c.registers.set_unnamed(RegisterValue::charwise("X"));
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(put(false));
        assert_eq!(text_of(&c), "aXc");
        let mut c2 = core("ac");
        c2.registers.set_unnamed(RegisterValue::charwise("X"));
        c2.mode = EditMode::Normal;
        c2.set_caret(0);
        c2.apply(put(true));
        assert_eq!(text_of(&c2), "Xac");
    }

    #[test]
    fn repeated_pastes_undo_one_at_a_time() {
        let mut c = core("a");
        c.registers.set_unnamed(RegisterValue::charwise("X"));
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(put(false));
        c.apply(put(false));
        assert_eq!(text_of(&c), "aXX");
        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "aX");
        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "a");
    }

    #[test]
    fn autoscroll_follows_caret_down() {
        let mut c = core("a\nb\nc\nd\ne\nf\n");
        c.rows = 3;
        c.set_caret(c.buffer.coords_to_char(5, 0));
        assert_eq!(c.autoscroll(0, 3), Some(3));
    }

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

    #[test]
    fn vertical_motion_collapses_selection_before_moving() {
        let mut c = core("first\nsecond\nthird\n");
        let start = c.buffer.coords_to_char(1, 1);
        let end = c.buffer.coords_to_char(2, 3);
        c.selections = vec![Selection {
            anchor: start,
            head: end,
        }];

        c.apply(EditCommand::Move(Motion::Up));

        assert_eq!(c.primary(), Selection::caret(start));
    }

    #[test]
    fn vertical_motion_preserves_column_across_empty_lines() {
        for mode in [EditMode::Insert, EditMode::Normal] {
            let mut c = core("abcdefghij\n\nabcdefghij\n");
            c.fold_view = crate::fold::FoldState::default().view(c.buffer.len_lines() as u32);
            c.mode = mode;
            c.set_caret(c.buffer.coords_to_char(0, 5));

            c.apply(EditCommand::Move(Motion::Down));
            assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 0));

            c.apply(EditCommand::Move(Motion::Down));
            assert_eq!(c.buffer.char_to_coords(c.primary().head), (2, 5));
        }
    }
}
