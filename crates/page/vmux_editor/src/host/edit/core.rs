use std::path::PathBuf;

use unicode_width::UnicodeWidthStr;

use crate::edit::buffer::TextBuffer;
use crate::edit::command::{
    CursorPos, EditCommand, EditMode, Motion, MotionKind, Operator, SelSpan, Selection, Target,
    VerticalDirection,
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

struct OperatorSpan {
    ranges: Vec<std::ops::Range<usize>>,
    kind: RegisterKind,
}

fn transform_case(src: &str, operator: Operator) -> String {
    src.chars()
        .map(|c| match operator {
            Operator::Upper => c.to_uppercase().next().unwrap_or(c),
            Operator::Lower => c.to_lowercase().next().unwrap_or(c),
            _ if c.is_uppercase() => c.to_lowercase().next().unwrap_or(c),
            _ => c.to_uppercase().next().unwrap_or(c),
        })
        .collect()
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

#[derive(Default, Clone)]
struct CaretMemory {
    preferred_vertical_col: Option<usize>,
    replaced: Vec<Option<char>>,
}

pub struct EditCore {
    pub buffer: TextBuffer,
    pub selections: Vec<Selection>,
    active: usize,
    in_sweep: bool,
    sweep_snapshotted: bool,
    pub mode: EditMode,
    pub rows: u16,
    pub top_row: u32,
    pub dirty: bool,
    pub registers: Registers,
    rev: u64,
    saved_rev: Option<u64>,
    undo: crate::edit::undo::UndoTree,
    last_group: Option<Group>,
    memory: Vec<CaretMemory>,
    pub fold_view: crate::fold::FoldView,
    pub search: Option<crate::edit::search::Search>,
    search_cache: Option<(u64, String, Vec<std::ops::Range<usize>>)>,
    block_insert: Option<(Vec<usize>, usize)>,
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
        let undo =
            crate::edit::undo::UndoTree::new(buffer.rope.clone(), vec![Selection::caret(0)], 0);
        Self {
            buffer,
            selections: vec![Selection::caret(0)],
            active: 0,
            in_sweep: false,
            sweep_snapshotted: false,
            mode: default_mode,
            rows: 0,
            top_row: 0,
            dirty: false,
            registers: Registers::default(),
            rev: 0,
            saved_rev: Some(0),
            undo,
            last_group: None,
            memory: vec![CaretMemory::default()],
            fold_view,
            search: None,
            search_cache: None,
            block_insert: None,
            search_highlight: false,
            marks: std::collections::HashMap::new(),
            jumps: Vec::new(),
            jump_index: 0,
            changes: Vec::new(),
            change_index: 0,
        }
    }

    fn buf_insert(&mut self, at: usize, text: &str) {
        self.buffer.insert(at, text);
        let n = text.chars().count();
        for mark in self.marks.values_mut() {
            if *mark > at {
                *mark += n;
            }
        }
        self.slide_inactive(|pos| if pos > at { pos + n } else { pos });
    }

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
        self.slide_inactive(|pos| {
            if pos >= range.end {
                pos - n
            } else if pos > range.start {
                range.start
            } else {
                pos
            }
        });
    }

    fn slide_inactive(&mut self, shift: impl Fn(usize) -> usize) {
        if self.selections.len() == 1 {
            return;
        }
        let active = self.active;
        for (index, sel) in self.selections.iter_mut().enumerate() {
            if index == active {
                continue;
            }
            sel.anchor = shift(sel.anchor);
            sel.head = shift(sel.head);
        }
    }

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
        self.selections[self.active.min(self.selections.len() - 1)]
    }

    pub fn selected_lines(&self) -> (u32, u32) {
        let sel = self.primary();
        let (from, _) = self.buffer.char_to_coords(sel.anchor.min(sel.head));
        let (to, _) = self.buffer.char_to_coords(sel.anchor.max(sel.head));
        (from as u32, to as u32)
    }

    fn set_active(&mut self, sel: Selection) {
        let at = self.active.min(self.selections.len() - 1);
        self.selections[at] = sel;
    }

    pub fn toggle_caret(&mut self, at: usize) {
        let at = at.min(self.buffer.len_chars());
        if let Some(index) = self.selections.iter().position(|s| s.head == at) {
            if self.selections.len() > 1 {
                self.selections.remove(index);
                self.active = self.active.saturating_sub((self.active >= index) as usize);
            }
            return;
        }
        self.selections.push(Selection::caret(at));
        self.active = self.selections.len() - 1;
        self.merge_overlapping_carets();
    }

    pub fn add_caret_vertically(&mut self, direction: VerticalDirection) {
        let delta = match direction {
            VerticalDirection::Up => -1,
            VerticalDirection::Down => 1,
        };
        let head = self.primary().head;
        let (line, col) = self.buffer.char_to_coords(head);
        let row = self.fold_view.step_rows(line as u32, delta) as usize;
        if row == line {
            return;
        }
        self.toggle_caret(self.buffer.coords_to_char(row, col));
    }

    pub fn collapse_carets(&mut self) {
        let keep = self.primary();
        self.selections = vec![keep];
        self.active = 0;
    }

    pub fn select_all_occurrences(&mut self) {
        let Some((under_caret, found)) = self.word_occurrences() else {
            return;
        };
        self.active = found
            .iter()
            .position(|range| range.start == under_caret.start)
            .unwrap_or(0);
        self.selections = found
            .into_iter()
            .map(|range| Selection {
                anchor: range.start,
                head: range.end,
            })
            .collect();
    }

    pub fn word_occurrences(
        &self,
    ) -> Option<(std::ops::Range<usize>, Vec<std::ops::Range<usize>>)> {
        let text = self.buffer.text();
        let chars: Vec<char> = text.chars().collect();
        let caret = self.primary().head.min(chars.len());
        let word = |i: usize| chars[i].is_alphanumeric() || chars[i] == '_';

        let mut start = caret;
        while start > 0 && word(start - 1) {
            start -= 1;
        }
        let mut end = caret;
        while end < chars.len() && word(end) {
            end += 1;
        }
        if start == end {
            return None;
        }
        let needle = &chars[start..end];

        let mut found = Vec::new();
        let mut at = 0;
        while at + needle.len() <= chars.len() {
            let matches = chars[at..at + needle.len()] == *needle;
            let bounded = (at == 0 || !word(at - 1))
                && (at + needle.len() == chars.len() || !word(at + needle.len()));
            if matches && bounded {
                found.push(at..at + needle.len());
                at += needle.len();
                continue;
            }
            at += 1;
        }
        if found.is_empty() {
            return None;
        }
        Some((start..end, found))
    }

    pub fn caret_count(&self) -> usize {
        self.selections.len()
    }

    fn memory(&mut self) -> &mut CaretMemory {
        if self.memory.len() != self.selections.len() {
            self.memory = vec![CaretMemory::default(); self.selections.len()];
        }
        let at = self.active.min(self.memory.len().saturating_sub(1));
        &mut self.memory[at]
    }

    pub fn set_caret(&mut self, at: usize) {
        self.memory().preferred_vertical_col = None;
        self.place_caret(at);
    }

    fn place_caret(&mut self, at: usize) {
        let at = if self.mode == EditMode::Normal {
            self.normal_cursor_target(at)
        } else {
            at.min(self.buffer.len_chars())
        };
        self.set_active(Selection::caret(at));
    }

    fn set_head(&mut self, head: usize) {
        let anchor = self.primary().anchor;
        self.set_active(Selection { anchor, head });
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
        self.cursor_pos_of(self.primary())
    }

    pub fn cursor_positions(&self) -> Vec<CursorPos> {
        let mut sorted: Vec<Selection> = self.selections.clone();
        sorted.sort_by_key(|s| s.head);
        sorted.into_iter().map(|s| self.cursor_pos_of(s)).collect()
    }

    fn cursor_pos_of(&self, sel: Selection) -> CursorPos {
        let (line, col) = self.buffer.char_to_coords(sel.head);
        let line_start = self.buffer.line_to_char(line);
        CursorPos {
            line: line as u32,
            row: line as u32,
            col: self.vis_col(line_start, col),
        }
    }

    pub fn visual_range(&self) -> std::ops::Range<usize> {
        self.visual_range_of(self.primary())
    }

    fn visual_range_of(&self, sel: Selection) -> std::ops::Range<usize> {
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
        if self.selections.len() == 1 {
            return self.sel_spans_of(self.primary(), first, rows);
        }
        let mut out = Vec::new();
        for sel in &self.selections {
            out.extend(self.sel_spans_of(*sel, first, rows));
        }
        out
    }

    fn sel_spans_of(&self, sel: Selection, first: u32, rows: u16) -> Vec<SelSpan> {
        if sel.is_empty() && !self.mode.is_visual() {
            return Vec::new();
        }
        if rows == 0 {
            return Vec::new();
        }
        if self.mode == EditMode::VisualBlock {
            return self
                .block_rows()
                .into_iter()
                .filter_map(|row| {
                    let (line, _) = self.buffer.char_to_coords(row.start);
                    if line < first as usize || line >= first as usize + rows as usize {
                        return None;
                    }
                    let ls = self.buffer.line_to_char(line);
                    Some(SelSpan {
                        line: line as u32,
                        row: line as u32,
                        start: self.vis_col(ls, row.start - ls),
                        end: self.vis_col(ls, row.end - ls),
                    })
                })
                .filter(|span| span.end > span.start)
                .collect();
        }
        let r = self.visual_range_of(sel);
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
            .push(&self.buffer.rope, &self.selections, self.rev);
    }

    fn restore(&mut self, state: crate::edit::undo::Restored) {
        self.buffer.rope = state.rope;
        self.selections = state.selections;
        self.active = 0;
        self.rev = state.rev;
        self.search_cache = None;
        self.dirty = self.saved_rev != Some(self.rev);
        self.break_group();
        let len = self.buffer.len_chars();
        for mark in self.marks.values_mut() {
            *mark = (*mark).min(len);
        }
    }

    fn checkpoint(&mut self, group: Group) {
        let wants_snapshot = self.last_group != Some(group) || group == Group::Other;
        if wants_snapshot && !(self.in_sweep && self.sweep_snapshotted) {
            self.snapshot();
            self.sweep_snapshotted = true;
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

    pub fn search_matches(&self) -> Vec<std::ops::Range<usize>> {
        let Some(search) = self.search.as_ref() else {
            return Vec::new();
        };
        let text = self.buffer.text();
        let mut out = Vec::new();
        let mut byte = 0usize;
        let mut chars = 0usize;
        for r in search.matches(&text) {
            chars += text[byte..r.start].chars().count();
            byte = r.start;
            let len = text[r.clone()].chars().count();
            out.push(chars..chars + len);
        }
        out
    }

    pub fn refresh_search_matches(&mut self) {
        let Some(search) = self.search.as_ref() else {
            self.search_cache = None;
            return;
        };
        if self
            .search_cache
            .as_ref()
            .is_some_and(|(rev, pattern, _)| *rev == self.rev && *pattern == search.pattern)
        {
            return;
        }
        let rev = self.rev;
        let pattern = search.pattern.clone();
        let matches = self.search_matches();
        self.search_cache = Some((rev, pattern, matches));
    }

    pub fn cached_search_matches(&self) -> &[std::ops::Range<usize>] {
        match self.search_cache.as_ref() {
            Some((_, _, matches)) => matches,
            None => &[],
        }
    }

    fn search_step(&self, from: usize, reverse: bool) -> Option<usize> {
        let search = self.search.as_ref()?;
        let forward = search.forward != reverse;
        if let Some((rev, pattern, cached)) = self.search_cache.as_ref()
            && *rev == self.rev
            && *pattern == search.pattern
        {
            return crate::edit::search::step(cached, from, forward);
        }
        let matches = self.search_matches();
        crate::edit::search::step(&matches, from, forward)
    }

    pub fn word_highlight_spans(&self, first: u32, rows: u16) -> Vec<SelSpan> {
        if rows == 0 {
            return Vec::new();
        }
        let word = self.primary().range();
        if word.is_empty() {
            return Vec::new();
        }
        let first_line = first as usize;
        let last_line = (first_line + rows as usize).min(self.buffer.len_lines());
        if first_line >= last_line {
            return Vec::new();
        }
        let band_start = self.buffer.line_to_char(first_line);
        let band_end = if last_line >= self.buffer.len_lines() {
            self.buffer.len_chars()
        } else {
            self.buffer.line_to_char(last_line)
        };
        let needle: Vec<char> = self.buffer.rope.slice(word).chars().collect();
        let band: Vec<char> = self
            .buffer
            .rope
            .slice(band_start..band_end)
            .chars()
            .collect();

        let mut out = Vec::new();
        let mut at = 0;
        while at + needle.len() <= band.len() {
            let found = band[at..at + needle.len()] == needle[..];
            let absolute = band_start + at;
            if found {
                let (line, column) = self.buffer.char_to_coords(absolute);
                let line_start = self.buffer.line_to_char(line);
                out.push(SelSpan {
                    line: line as u32,
                    row: line as u32,
                    start: self.vis_col(line_start, column),
                    end: self.vis_col(line_start, absolute + needle.len() - line_start),
                });
                at += needle.len();
                continue;
            }
            at += 1;
        }
        out
    }
    pub fn search_spans(
        &self,
        matches: &[std::ops::Range<usize>],
        first: u32,
        rows: u16,
    ) -> Vec<SelSpan> {
        if !self.search_highlight || rows == 0 {
            return Vec::new();
        }
        let last_line = first as usize + rows as usize;
        let above =
            matches.partition_point(|m| self.buffer.char_to_coords(m.end).0 < first as usize);
        let mut out = Vec::new();
        for m in matches[above..].iter().cloned() {
            let (l0, _) = self.buffer.char_to_coords(m.start);
            if l0 >= last_line {
                break;
            }
            let (l1, _) = self.buffer.char_to_coords(m.end);
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

    fn number_at_caret(&self) -> Option<(std::ops::Range<usize>, i64)> {
        let (line, col) = self.buffer.char_to_coords(self.primary().head);
        let base = self.buffer.line_to_char(line);
        let len = self.buffer.line_len_chars(line);
        let digit = |i: usize| self.buffer.rope.char(base + i).is_ascii_digit();
        let mut start = (col..len).find(|i| digit(*i))?;
        while start > 0 && digit(start - 1) {
            start -= 1;
        }
        let mut end = start;
        while end < len && digit(end) {
            end += 1;
        }
        let negative = start > 0 && self.buffer.rope.char(base + start - 1) == '-';
        let text: String = self
            .buffer
            .rope
            .slice(base + start..base + end)
            .chars()
            .collect();
        let magnitude = text.parse::<i64>().ok()?;
        let span_start = if negative { start - 1 } else { start };
        let value = if negative { -magnitude } else { magnitude };
        Some((base + span_start..base + end, value))
    }

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
            self.memory().preferred_vertical_col = None;
            return self.resolve_motion(from, motion);
        };
        let (_, current_col) = self.buffer.char_to_coords(from);
        let preferred_col = *self
            .memory()
            .preferred_vertical_col
            .get_or_insert(current_col);
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

    pub fn paste(&mut self, text: &str) -> bool {
        self.break_group();
        let changed = self.insert_text(text);
        self.break_group();
        changed
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

    pub fn block_rows(&self) -> Vec<std::ops::Range<usize>> {
        let sel = self.primary();
        let (l0, c0) = self.buffer.char_to_coords(sel.anchor);
        let (l1, c1) = self.buffer.char_to_coords(sel.head);
        let (first, last) = (l0.min(l1), l0.max(l1));
        let (left, right) = (c0.min(c1), c0.max(c1) + 1);
        (first..=last)
            .map(|line| {
                let base = self.buffer.line_to_char(line);
                let len = self.buffer.line_len_chars(line);
                let start = base + left.min(len);
                let end = base + right.min(len);
                start..end.max(start)
            })
            .collect()
    }

    fn operator_span(&self, target: Target) -> Option<OperatorSpan> {
        if target == Target::Selection && self.mode == EditMode::VisualBlock {
            let rows = self.block_rows();
            if rows.iter().all(|r| r.is_empty()) {
                return None;
            }
            return Some(OperatorSpan {
                ranges: rows,
                kind: RegisterKind::Blockwise,
            });
        }
        let (range, kind) = self.operator_range(target)?;
        Some(OperatorSpan {
            ranges: vec![range],
            kind,
        })
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

    fn reshape(&mut self, to: crate::shape::BufferShape) -> bool {
        let source: String = self.buffer.rope.chars().collect();
        let from = crate::shape::BufferShape::of(&self.buffer.rope).indent;
        let out = crate::shape::Reindent { from, to }.applied(&source);
        if out == source {
            return false;
        }
        let line = self.cursor_pos().line as usize;
        self.checkpoint(Group::Other);
        self.buf_remove(0..self.buffer.len_chars());
        self.buf_insert(0, &out);
        let line = line.min(self.buffer.len_lines().saturating_sub(1));
        let at = self.buffer.line_to_char(line);
        self.set_caret(self.first_non_blank(at));
        true
    }

    fn apply_line_shift(
        &mut self,
        operator: Operator,
        range: std::ops::Range<usize>,
    ) -> (bool, Option<RegisterValue>) {
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
        let caret = self.first_non_blank(self.buffer.line_to_char(l0));
        self.set_caret(caret);
        (true, None)
    }

    fn apply_block_operator(
        &mut self,
        operator: Operator,
        rows: &[std::ops::Range<usize>],
        register: Option<char>,
    ) -> (bool, Option<RegisterValue>) {
        let text = rows
            .iter()
            .map(|r| {
                self.buffer
                    .rope
                    .slice(r.clone())
                    .chars()
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let value = RegisterValue {
            text,
            kind: RegisterKind::Blockwise,
        };
        let caret = rows.first().map(|r| r.start).unwrap_or_default();
        match operator {
            Operator::Yank => {
                self.registers.write_yank(register, value.clone());
                self.mode = EditMode::Normal;
                self.set_caret(caret);
                (false, Some(value))
            }
            Operator::Delete | Operator::Change => {
                self.registers.write_delete(register, value.clone());
                self.checkpoint(Group::Other);
                for row in rows.iter().rev() {
                    if !row.is_empty() {
                        self.buf_remove(row.clone());
                    }
                }
                self.mode = if operator == Operator::Change {
                    EditMode::Insert
                } else {
                    EditMode::Normal
                };
                self.set_caret(caret.min(self.buffer.len_chars()));
                (true, Some(value))
            }
            Operator::Upper | Operator::Lower | Operator::ToggleCase => {
                self.checkpoint(Group::Other);
                for row in rows.iter().rev() {
                    if row.is_empty() {
                        continue;
                    }
                    let src: String = self.buffer.rope.slice(row.clone()).chars().collect();
                    let out = transform_case(&src, operator);
                    self.buf_remove(row.clone());
                    self.buf_insert(row.start, &out);
                }
                self.mode = EditMode::Normal;
                self.set_caret(caret);
                (true, None)
            }
            Operator::Indent | Operator::Outdent => {
                let span = rows.first().map(|r| r.start).unwrap_or_default()
                    ..rows.last().map(|r| r.end).unwrap_or_default();
                self.mode = EditMode::Normal;
                self.apply_line_shift(operator, span)
            }
        }
    }

    fn apply_operator(
        &mut self,
        operator: Operator,
        target: Target,
        register: Option<char>,
    ) -> (bool, Option<RegisterValue>) {
        let Some(span) = self.operator_span(target) else {
            return (false, None);
        };
        if span.kind == RegisterKind::Blockwise {
            let rows = span.ranges.clone();
            return self.apply_block_operator(operator, &rows, register);
        }
        let kind = span.kind;
        let range = span.ranges[0].clone();
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
                if was_visual {
                    self.mode = EditMode::Normal;
                }
                self.apply_line_shift(operator, range)
            }
            Operator::Upper | Operator::Lower | Operator::ToggleCase => {
                let src: String = self.buffer.rope.slice(range.clone()).chars().collect();
                let out = transform_case(&src, operator);
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
        if self.mode == EditMode::VisualBlock {
            let rows = self.block_rows();
            self.apply_block_operator(Operator::Delete, &rows, None);
        } else if was_visual {
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
            RegisterKind::Blockwise => {
                let head = self.primary().head;
                let (line, col) = self.buffer.char_to_coords(head);
                let col = if before { col } else { col + 1 };
                self.checkpoint(Group::Other);
                for (offset, row) in value.text.split('\n').enumerate() {
                    let target = line + offset;
                    if target >= self.buffer.len_lines() {
                        let end = self.buffer.len_chars();
                        self.buf_insert(end, "\n");
                    }
                    let base = self.buffer.line_to_char(target);
                    let len = self.buffer.line_len_chars(target);
                    let pad = col.saturating_sub(len);
                    if pad > 0 {
                        self.buf_insert(base + len, &" ".repeat(pad));
                    }
                    let at = base + col.min(len + pad);
                    self.buf_insert(at, &row.repeat(count));
                }
                self.set_caret(self.buffer.coords_to_char(line, col));
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
        if self.selections.len() == 1 {
            self.active = 0;
            return self.apply_at_active(cmd);
        }
        if !cmd.is_per_caret() {
            return self.apply_at_active(cmd);
        }

        let mut order: Vec<usize> = (0..self.selections.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(self.selections[i].range().start));

        let was_active = self.active;
        let mut outcome = EditOutcome::default();
        self.in_sweep = true;
        self.sweep_snapshotted = false;
        for index in order {
            self.active = index;
            let out = self.apply_at_active(cmd.clone());
            outcome.text_changed |= out.text_changed;
            outcome.sel_changed |= out.sel_changed;
            outcome.mode_changed |= out.mode_changed;
            outcome.dirty_changed |= out.dirty_changed;
            outcome.scroll_to = outcome.scroll_to.or(out.scroll_to);
            outcome.yank = outcome.yank.or(out.yank);
        }
        self.in_sweep = false;
        self.sweep_snapshotted = false;
        self.active = was_active.min(self.selections.len().saturating_sub(1));
        self.merge_overlapping_carets();
        outcome
    }

    fn merge_overlapping_carets(&mut self) {
        if self.selections.len() == 1 {
            return;
        }
        let active = self.primary();
        self.selections.sort_by_key(|s| (s.range().start, s.head));
        let mut merged: Vec<Selection> = Vec::with_capacity(self.selections.len());
        for sel in self.selections.drain(..) {
            match merged.last() {
                Some(prev) if prev.range().end >= sel.range().start && prev.head == sel.head => {}
                Some(prev) if prev.head == sel.head && prev.anchor == sel.anchor => {}
                _ => merged.push(sel),
            }
        }
        self.selections = merged;
        self.active = self
            .selections
            .iter()
            .position(|s| *s == active)
            .unwrap_or(0);
    }

    fn apply_at_active(&mut self, cmd: EditCommand) -> EditOutcome {
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
            self.memory().preferred_vertical_col = None;
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
                    self.memory().preferred_vertical_col = None;
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
                        let covered = self.buffer.rope.char(at);
                        self.memory().replaced.push(Some(covered));
                        self.buf_remove(at..at + 1);
                    } else {
                        self.memory().replaced.push(None);
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
                if let Some(original) = self.memory().replaced.pop()
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
                if let Some(state) = self
                    .undo
                    .undo(&self.buffer.rope, &self.selections, self.rev)
                {
                    self.restore(state);
                    text_changed = true;
                }
            }
            EditCommand::Redo => {
                if let Some(state) = self
                    .undo
                    .redo(&self.buffer.rope, &self.selections, self.rev)
                {
                    self.restore(state);
                    text_changed = true;
                }
            }
            EditCommand::UndoTime { forward, count } => {
                if let Some(state) = self.undo.step_time(
                    &self.buffer.rope,
                    &self.selections,
                    self.rev,
                    forward,
                    count,
                ) {
                    self.restore(state);
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
            EditCommand::OpenFind { .. } | EditCommand::OpenCommandLine => {}
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
            EditCommand::Reshape(shape) => {
                text_changed = self.reshape(shape);
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
            EditCommand::BeginBlockInsert { after } => {
                let rows = self.block_rows();
                let Some(first) = rows.first().cloned() else {
                    return EditOutcome::default();
                };
                let (line, _) = self.buffer.char_to_coords(first.start);
                let col = if after {
                    self.buffer.char_to_coords(first.end).1
                } else {
                    self.buffer.char_to_coords(first.start).1
                };
                let lines = (0..rows.len()).map(|i| line + i).collect();
                self.block_insert = Some((lines, col));
                self.mode = EditMode::Insert;
                self.set_caret(self.buffer.coords_to_char(line, col));
            }
            EditCommand::FinishBlockInsert { text } => {
                if let Some((lines, col)) = self.block_insert.take()
                    && !text.is_empty()
                {
                    self.checkpoint(Group::Other);
                    for line in lines.into_iter().skip(1).rev() {
                        if line >= self.buffer.len_lines() {
                            continue;
                        }
                        let base = self.buffer.line_to_char(line);
                        let len = self.buffer.line_len_chars(line);
                        if col > len {
                            continue;
                        }
                        self.buf_insert(base + col, &text);
                    }
                    text_changed = true;
                }
            }
            EditCommand::Increment(delta) => {
                if let Some((span, value)) = self.number_at_caret() {
                    let width = span.end - span.start;
                    let digits: String = self.buffer.rope.slice(span.clone()).chars().collect();
                    let padded = digits.starts_with('0') && digits.len() > 1;
                    let next = value.saturating_add(delta);
                    let text = if padded {
                        format!("{:0>width$}", next, width = width)
                    } else {
                        next.to_string()
                    };
                    self.checkpoint(Group::Other);
                    self.buf_remove(span.clone());
                    self.buf_insert(span.start, &text);
                    self.set_caret(span.start + text.chars().count() - 1);
                    text_changed = true;
                }
            }
            EditCommand::SwapSelectionEnds => {
                let sel = self.primary();
                self.set_active(Selection {
                    anchor: sel.head,
                    head: sel.anchor,
                });
            }
            EditCommand::SelectTextObject(obj) => {
                if let Some(r) =
                    crate::edit::text_object::resolve(&self.buffer, self.primary().head, obj)
                    && r.start < r.end
                {
                    self.set_active(Selection {
                        anchor: r.start,
                        head: self.buffer.prev_grapheme(r.end),
                    });
                }
            }
            EditCommand::Save
            | EditCommand::Paste
            | EditCommand::ScrollViewport(_)
            | EditCommand::ScrollCursorTo(_)
            | EditCommand::GotoDefinition
            | EditCommand::FindReferences
            | EditCommand::BeginRename
            | EditCommand::Hover
            | EditCommand::TriggerCompletion
            | EditCommand::FoldToggle
            | EditCommand::FoldOpen
            | EditCommand::FoldClose
            | EditCommand::FoldToggleRecursive
            | EditCommand::FoldAll
            | EditCommand::UnfoldAll => {}
            EditCommand::SelectAllOccurrences => self.select_all_occurrences(),
            EditCommand::CollapseCarets => self.collapse_carets(),
            EditCommand::AddCaretVertically(direction) => self.add_caret_vertically(direction),
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

    fn typed_after_vim_keys(text: &str, caret: usize, keys: &[&str], typed: &str) -> String {
        use crate::host::keymap::vim::VimKeymap;
        use crate::keymap::{KeyInput, Keymap, Mods};

        let mut c = core(text);
        c.mode = EditMode::Normal;
        c.set_caret(caret);
        let mut keymap = VimKeymap::default();
        for key in keys {
            let stroke = KeyInput {
                key: (*key).into(),
                mods: Mods::default(),
                repeat: false,
            };
            for cmd in keymap.handle(&stroke) {
                c.apply(cmd);
            }
        }
        c.apply(EditCommand::InsertText(typed.into()));
        text_of(&c)
    }

    #[test]
    fn append_at_line_end_types_past_the_last_character() {
        assert_eq!(typed_after_vim_keys("ab\n", 0, &["A"], "X"), "abX\n");
    }

    #[test]
    fn append_on_the_last_character_types_past_it() {
        assert_eq!(typed_after_vim_keys("ab\n", 1, &["a"], "X"), "abX\n");
    }

    #[test]
    fn cached_search_matches_follow_an_edit_and_a_new_pattern() {
        let mut c = core("foo bar foo\n");
        c.mode = EditMode::Normal;
        let count = |c: &mut EditCore| {
            c.refresh_search_matches();
            c.cached_search_matches().len()
        };

        c.apply(EditCommand::SetSearch {
            pattern: "foo".into(),
            forward: true,
        });
        assert_eq!(count(&mut c), 2);

        c.mode = EditMode::Insert;
        c.set_caret(0);
        c.apply(EditCommand::InsertText("foo ".into()));
        assert_eq!(count(&mut c), 3, "an edit must not serve a stale match set");

        c.apply(EditCommand::SetSearch {
            pattern: "bar".into(),
            forward: true,
        });
        assert_eq!(
            count(&mut c),
            1,
            "a new pattern must not serve the old one's matches"
        );
    }

    #[test]
    fn cached_search_matches_survive_an_undo_onto_a_different_edit() {
        let mut c = core("foo\n");
        c.mode = EditMode::Insert;
        c.apply(EditCommand::SetSearch {
            pattern: "foo".into(),
            forward: true,
        });
        c.set_caret(3);
        c.apply(EditCommand::InsertText(" foo".into()));
        c.refresh_search_matches();
        assert_eq!(c.cached_search_matches().len(), 2);

        c.apply(EditCommand::Undo);
        c.set_caret(3);
        c.apply(EditCommand::InsertText(" foo foo".into()));
        c.refresh_search_matches();

        assert_eq!(
            c.cached_search_matches().len(),
            3,
            "a revision number is reused across undo branches, so it cannot key the cache alone"
        );
    }

    #[test]
    fn only_a_selection_highlights_its_other_occurrences() {
        let mut c = core("id\nid_x\nother\nid\nid\n");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        assert!(
            c.word_highlight_spans(0, 5).is_empty(),
            "a bare caret sitting on a word must not light up the whole file"
        );

        c.set_active(Selection { anchor: 0, head: 2 });
        let in_band = |first, rows| {
            c.word_highlight_spans(first, rows)
                .iter()
                .map(|s| (s.line, s.start, s.end))
                .collect::<Vec<_>>()
        };

        assert_eq!(in_band(0, 3), vec![(0, 0, 2), (1, 0, 2)]);
        assert_eq!(
            in_band(0, 5),
            vec![(0, 0, 2), (1, 0, 2), (3, 0, 2), (4, 0, 2)]
        );
        assert_eq!(in_band(3, 2), vec![(3, 0, 2), (4, 0, 2)]);
    }

    #[test]
    fn pasting_types_after_the_pasted_text() {
        let mut c = core("ab\n");
        c.mode = EditMode::Insert;
        c.set_caret(2);
        c.paste("XY");
        c.apply(EditCommand::InsertText("Z".into()));

        assert_eq!(text_of(&c), "abXYZ\n");
    }

    #[test]
    fn pasting_replaces_the_selection() {
        let mut c = core("abcd\n");
        c.mode = EditMode::Insert;
        c.selections = vec![Selection { anchor: 1, head: 3 }];
        c.paste("X");

        assert_eq!(text_of(&c), "aXd\n");
    }

    #[test]
    fn undo_takes_a_paste_back_on_its_own() {
        let mut c = core("");
        c.mode = EditMode::Insert;
        c.apply(EditCommand::InsertText("a".into()));
        c.paste("XY");
        c.apply(EditCommand::InsertText("b".into()));

        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "aXY");
        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "a");
    }

    #[test]
    fn putting_in_normal_mode_lands_on_the_last_pasted_character() {
        let mut c = core("ab\n");
        c.mode = EditMode::Normal;
        c.registers.set_unnamed(RegisterValue::charwise("XY"));
        c.set_caret(0);
        c.apply(put(false));

        assert_eq!(text_of(&c), "aXYb\n");
        assert_eq!(c.primary().head, 2);
    }

    #[test]
    fn selecting_all_occurrences_skips_the_ones_inside_longer_words() {
        let mut c = core("id width id_x\nid\n");
        c.apply(EditCommand::SelectAllOccurrences);

        let hits: Vec<&str> = c
            .selections
            .iter()
            .map(|s| {
                let text = c.buffer.text();
                let chars: Vec<char> = text.chars().collect();
                let r = s.range();
                chars[r.start..r.end].iter().collect::<String>()
            })
            .map(|s| Box::leak(s.into_boxed_str()) as &str)
            .collect();
        assert_eq!(hits, vec!["id", "id"], "`width` and `id_x` are other words");
    }

    #[test]
    fn selecting_all_occurrences_off_a_word_leaves_the_carets_alone() {
        let mut c = core("id  id\n");
        c.selections = vec![Selection { anchor: 3, head: 3 }];
        c.apply(EditCommand::SelectAllOccurrences);

        assert_eq!(c.selections.len(), 1);
        assert_eq!(c.primary().head, 3);
    }

    fn multi_caret(text: &str, at: &[usize]) -> EditCore {
        let mut c = core(text);
        c.set_caret(at[0]);
        for &offset in &at[1..] {
            c.toggle_caret(offset);
        }
        assert_eq!(c.caret_count(), at.len(), "fixture placed every caret");
        c
    }

    fn heads(c: &EditCore) -> Vec<usize> {
        let mut heads: Vec<usize> = c.selections.iter().map(|s| s.head).collect();
        heads.sort_unstable();
        heads
    }

    #[test]
    fn each_caret_keeps_its_own_column_through_a_short_line() {
        let mut c = multi_caret("abcdefgh\nij\nklmnopqr\nABCDEFGH\nkl\nMNOPQRST\n", &[6, 25]);
        c.apply(EditCommand::Move(Motion::Down));
        c.apply(EditCommand::Move(Motion::Down));
        let cols: Vec<usize> = c
            .selections
            .iter()
            .map(|s| c.buffer.char_to_coords(s.head).1)
            .collect();
        assert_eq!(cols, vec![6, 4], "each caret homes on the column it left");
    }

    #[test]
    fn backspace_restores_what_this_caret_overtyped() {
        let mut c = multi_caret("abc\nxyz\n", &[0, 4]);
        c.mode = EditMode::Replace;
        c.apply(EditCommand::OvertypeText("Q".into()));
        assert_eq!(text_of(&c), "Qbc\nQyz\n");
        c.apply(EditCommand::DeleteBack);
        assert_eq!(text_of(&c), "abc\nxyz\n");
    }

    #[test]
    fn collapsing_keeps_the_caret_placed_last_even_when_it_sorts_first() {
        let mut c = multi_caret("abcdef\n", &[4, 1]);
        c.collapse_carets();
        assert_eq!(heads(&c), vec![1]);
    }

    #[test]
    fn adding_a_caret_below_twice_types_on_three_rows() {
        let mut c = core("ab\ncd\nef\ngh\n");
        c.set_caret(0);
        c.apply(EditCommand::AddCaretVertically(VerticalDirection::Down));
        c.apply(EditCommand::AddCaretVertically(VerticalDirection::Down));
        assert_eq!(c.caret_count(), 3, "the second one grew from the new caret");
        c.apply(EditCommand::InsertText(">".into()));
        assert_eq!(text_of(&c), ">ab\n>cd\n>ef\ngh\n");
    }

    #[test]
    fn a_sweep_leaves_the_active_caret_where_it_was() {
        let mut c = core("ab\ncd\nef\n");
        c.set_caret(0);
        c.apply(EditCommand::AddCaretVertically(VerticalDirection::Down));
        c.apply(EditCommand::AddCaretVertically(VerticalDirection::Down));
        c.apply(EditCommand::InsertText(">".into()));
        c.apply(EditCommand::CollapseCarets);
        let (line, _) = c.buffer.char_to_coords(c.primary().head);
        assert_eq!(line, 2, "the caret the gesture ended on");
    }

    #[test]
    fn adding_a_caret_past_the_last_row_adds_nothing() {
        let mut c = core("ab\ncd\n");
        c.set_caret(c.buffer.coords_to_char(2, 0));
        c.apply(EditCommand::AddCaretVertically(VerticalDirection::Down));
        assert_eq!(c.caret_count(), 1);
    }

    #[test]
    fn untoggling_an_earlier_caret_does_not_move_which_one_is_active() {
        let mut c = multi_caret("abcdef\n", &[1, 5, 3]);
        c.toggle_caret(1);
        assert_eq!(heads(&c), vec![3, 5]);
        c.collapse_carets();
        assert_eq!(heads(&c), vec![3], "still the caret the user placed last");
    }

    #[test]
    fn typing_reaches_every_caret() {
        let mut c = multi_caret("aa\nbb\ncc\n", &[0, 3, 6]);
        c.apply(EditCommand::InsertText("X".into()));
        assert_eq!(text_of(&c), "Xaa\nXbb\nXcc\n");
        assert_eq!(
            heads(&c),
            vec![1, 5, 9],
            "each caret sits after its own insert"
        );
    }

    #[test]
    fn a_multi_caret_edit_undoes_in_one_step() {
        let mut c = multi_caret("aa\nbb\ncc\n", &[0, 3, 6]);
        c.apply(EditCommand::InsertText("X".into()));
        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "aa\nbb\ncc\n");
    }

    #[test]
    fn deleting_reaches_every_caret() {
        let mut c = multi_caret("aXa\nbXb\ncXc\n", &[2, 6, 10]);
        c.apply(EditCommand::DeleteBack);
        assert_eq!(text_of(&c), "aa\nbb\ncc\n");
        assert_eq!(heads(&c), vec![1, 4, 7]);
    }

    #[test]
    fn a_caret_that_runs_into_another_is_merged_away() {
        let mut c = multi_caret("ab\n", &[1, 2]);
        c.apply(EditCommand::DeleteBack);
        assert_eq!(text_of(&c), "\n");
        assert_eq!(c.caret_count(), 1, "two carets collapsed onto one offset");
    }

    #[test]
    fn toggling_an_existing_caret_takes_it_away() {
        let mut c = multi_caret("abcdef\n", &[1, 3]);
        c.toggle_caret(3);
        assert_eq!(c.caret_count(), 1);
        assert_eq!(heads(&c), vec![1]);
    }

    #[test]
    fn the_last_caret_cannot_be_removed() {
        let mut c = core("abc\n");
        c.set_caret(1);
        c.toggle_caret(1);
        assert_eq!(c.caret_count(), 1, "a buffer with no caret has no meaning");
    }

    #[test]
    fn collapsing_leaves_the_caret_the_user_last_placed() {
        let mut c = multi_caret("aa\nbb\ncc\n", &[0, 3, 6]);
        c.collapse_carets();
        assert_eq!(c.caret_count(), 1);
    }

    #[test]
    fn a_buffer_wide_command_runs_once_however_many_carets() {
        let mut c = multi_caret("aa\nbb\ncc\n", &[0, 3, 6]);
        c.apply(EditCommand::InsertText("X".into()));
        c.apply(EditCommand::Move(Motion::Right));
        c.apply(EditCommand::InsertText("Y".into()));
        assert_eq!(text_of(&c), "XaYa\nXbYb\nXcYc\n");

        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "Xaa\nXbb\nXcc\n", "one step back, not three");
    }

    #[test]
    fn every_caret_is_drawn_not_just_the_first() {
        let mut c = multi_caret("aaaa\nbbbb\n", &[0, 5]);
        c.apply(EditCommand::Select(Motion::Right));
        let spans = c.sel_spans(0, 10);
        assert_eq!(spans.len(), 2, "both selections render: {spans:?}");
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

    fn block(text: &str, from: (usize, usize), to: (usize, usize)) -> EditCore {
        let mut c = core(text);
        c.mode = EditMode::VisualBlock;
        c.selections = vec![Selection {
            anchor: c.buffer.coords_to_char(from.0, from.1),
            head: c.buffer.coords_to_char(to.0, to.1),
        }];
        c
    }

    #[test]
    fn a_new_edit_after_undo_branches_and_time_travel_reaches_the_old_one() {
        let mut c = core("");
        c.apply(EditCommand::InsertText("first".into()));
        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "");
        c.apply(EditCommand::InsertText("second".into()));
        assert_eq!(text_of(&c), "second");

        c.apply(EditCommand::UndoTime {
            forward: false,
            count: 1,
        });
        assert_eq!(text_of(&c), "first", "g- reaches the abandoned branch");
        c.apply(EditCommand::UndoTime {
            forward: true,
            count: 1,
        });
        assert_eq!(text_of(&c), "second");
    }

    #[test]
    fn bounded_horizontal_motion_stops_at_a_hard_wrap() {
        let mut c = core("alpha\nbeta\n");
        c.mode = EditMode::Normal;
        c.set_caret(c.buffer.coords_to_char(0, 4));
        c.apply(EditCommand::Move(Motion::RightBounded));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (0, 4));
        c.set_caret(c.buffer.coords_to_char(1, 0));
        c.apply(EditCommand::Move(Motion::LeftBounded));
        assert_eq!(c.buffer.char_to_coords(c.primary().head), (1, 0));
    }

    #[test]
    fn increment_finds_the_number_at_or_after_the_caret() {
        let mut c = core("item 41 done");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Increment(1));
        assert_eq!(text_of(&c), "item 42 done");
    }

    #[test]
    fn increment_crosses_zero_on_a_negative_number() {
        let mut c = core("x -1 y");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Increment(2));
        assert_eq!(text_of(&c), "x 1 y");
    }

    #[test]
    fn increment_preserves_zero_padding() {
        let mut c = core("v007");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Increment(1));
        assert_eq!(text_of(&c), "v008");
    }

    #[test]
    fn increment_without_a_number_is_inert() {
        let mut c = core("no digits");
        c.mode = EditMode::Normal;
        c.set_caret(0);
        c.apply(EditCommand::Increment(1));
        assert_eq!(text_of(&c), "no digits");
    }

    #[test]
    fn block_delete_removes_a_rectangle() {
        let mut c = block("abcd\nefgh\nijkl\n", (0, 1), (2, 2));
        c.apply(op(Operator::Delete, Target::Selection));
        assert_eq!(text_of(&c), "ad\neh\nil\n");
        assert_eq!(c.mode, EditMode::Normal);
    }

    #[test]
    fn block_yank_joins_rows_with_newlines() {
        let mut c = block("abcd\nefgh\n", (0, 1), (1, 2));
        let out = c.apply(op(Operator::Yank, Target::Selection));
        assert_eq!(
            out.yank,
            Some(RegisterValue {
                text: "bc\nfg".into(),
                kind: RegisterKind::Blockwise,
            })
        );
    }

    #[test]
    fn block_rows_clamp_to_short_lines() {
        let c = block("abcd\nx\nijkl\n", (0, 1), (2, 2));
        let rows = c.block_rows();
        assert_eq!(rows.len(), 3);
        assert!(rows[1].is_empty(), "the short line contributes no columns");
    }

    #[test]
    fn block_put_inserts_a_column_on_each_line() {
        let mut c = core("ab\ncd\n");
        c.mode = EditMode::Normal;
        c.registers.set_unnamed(RegisterValue {
            text: "X\nY".into(),
            kind: RegisterKind::Blockwise,
        });
        c.set_caret(0);
        c.apply(put(true));
        assert_eq!(text_of(&c), "Xab\nYcd\n");
    }

    #[test]
    fn block_put_pads_short_lines_out_to_the_column() {
        let mut c = core("abcd\nx\n");
        c.mode = EditMode::Normal;
        c.registers.set_unnamed(RegisterValue {
            text: "1\n2".into(),
            kind: RegisterKind::Blockwise,
        });
        c.set_caret(c.buffer.coords_to_char(0, 3));
        c.apply(put(true));
        assert_eq!(text_of(&c), "abc1d\nx  2\n");
    }

    #[test]
    fn block_case_operator_covers_every_row() {
        let mut c = block("abcd\nefgh\n", (0, 0), (1, 1));
        c.apply(op(Operator::Upper, Target::Selection));
        assert_eq!(text_of(&c), "ABcd\nEFgh\n");
    }

    #[test]
    fn block_insert_replicates_typed_text_down_the_column() {
        let mut c = block("abc\ndef\nghi\n", (0, 1), (2, 1));
        c.apply(EditCommand::BeginBlockInsert { after: false });
        assert_eq!(c.mode, EditMode::Insert);
        c.apply(EditCommand::InsertText("-".into()));
        c.apply(EditCommand::FinishBlockInsert { text: "-".into() });
        assert_eq!(text_of(&c), "a-bc\nd-ef\ng-hi\n");
    }

    #[test]
    fn block_append_inserts_after_the_right_edge() {
        let mut c = block("abc\ndef\n", (0, 0), (1, 0));
        c.apply(EditCommand::BeginBlockInsert { after: true });
        c.apply(EditCommand::InsertText("!".into()));
        c.apply(EditCommand::FinishBlockInsert { text: "!".into() });
        assert_eq!(text_of(&c), "a!bc\nd!ef\n");
    }

    #[test]
    fn block_selection_renders_one_span_per_row() {
        let c = block("abcd\nefgh\n", (0, 1), (1, 2));
        let spans = c.sel_spans(0, 4);
        assert_eq!(spans.len(), 2);
        assert!(spans.iter().all(|s| s.start == 1 && s.end == 3));
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
    fn reshape_rewrites_the_indentation_as_one_undoable_edit() {
        let mut c = core("fn a() {\n\tb();\n\t\tc();\n}\n");
        c.mode = EditMode::Normal;

        c.apply(EditCommand::Reshape(crate::shape::BufferShape {
            indent: vmux_core::event::FileIndent {
                spaces: true,
                width: 2,
            },
            line_ending: vmux_core::event::FileLineEnding::Lf,
        }));

        assert_eq!(text_of(&c), "fn a() {\n  b();\n    c();\n}\n");
        assert!(c.dirty);
        c.apply(EditCommand::Undo);
        assert_eq!(text_of(&c), "fn a() {\n\tb();\n\t\tc();\n}\n");
    }

    #[test]
    fn reshaping_to_the_current_shape_leaves_the_buffer_unchanged() {
        let mut c = core("a\n  b\n");
        c.mode = EditMode::Normal;

        c.apply(EditCommand::Reshape(crate::shape::BufferShape {
            indent: vmux_core::event::FileIndent {
                spaces: true,
                width: 2,
            },
            line_ending: vmux_core::event::FileLineEnding::Lf,
        }));

        assert_eq!(text_of(&c), "a\n  b\n");
        assert!(!c.dirty);
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
        assert_eq!(c.search_spans(&c.search_matches(), 0, 4).len(), 2);
        c.apply(EditCommand::ClearSearchHighlight);
        assert!(c.search_spans(&c.search_matches(), 0, 4).is_empty());
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
    fn line_end_operator_stops_before_the_newline() {
        let mut c = core("abc\ndef\n");
        c.mode = EditMode::Normal;
        c.set_caret(2);

        c.apply(op(Operator::Delete, Target::Motion(Motion::LineEnd, 1)));

        assert_eq!(text_of(&c), "ab\ndef\n");
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
