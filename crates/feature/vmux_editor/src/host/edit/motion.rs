use crate::edit::buffer::TextBuffer;
use crate::edit::command::Motion;
use crate::edit::text_object::TextObjectKind;
use crate::fold::FoldView;

pub(super) struct MotionResolver<'a> {
    buffer: &'a TextBuffer,
    fold_view: &'a FoldView,
    rows: u16,
    top_row: u32,
}

impl<'a> MotionResolver<'a> {
    pub(super) fn new(
        buffer: &'a TextBuffer,
        fold_view: &'a FoldView,
        rows: u16,
        top_row: u32,
    ) -> Self {
        Self {
            buffer,
            fold_view,
            rows,
            top_row,
        }
    }

    pub(super) fn resolve(&self, from: usize, motion: Motion) -> Option<usize> {
        let len = self.buffer.len_chars();
        Some(match motion {
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
                let (line, _) = self.buffer.char_to_coords(from);
                self.buffer.line_to_char(line)
            }
            Motion::FirstNonBlank => self.first_non_blank(from),
            Motion::LineEnd => {
                let (line, _) = self.buffer.char_to_coords(from);
                self.buffer.line_to_char(line) + self.buffer.line_len_chars(line)
            }
            Motion::DocStart => 0,
            Motion::DocEnd => len,
            Motion::GotoLine(line) => self.buffer.line_to_char(line as usize),
            Motion::WordNext => self.word_next(from, false),
            Motion::WordPrev => self.word_prev(from, false),
            Motion::WordEnd => self.word_end(from, false),
            Motion::BigWordNext => self.word_next(from, true),
            Motion::BigWordPrev => self.word_prev(from, true),
            Motion::BigWordEnd => self.word_end(from, true),
            Motion::WordEndPrev => self.word_end_prev(from, false),
            Motion::BigWordEndPrev => self.word_end_prev(from, true),
            Motion::LastNonBlank => self.last_non_blank(from),
            Motion::Column(column) => {
                let (line, _) = self.buffer.char_to_coords(from);
                let start = self.buffer.line_to_char(line);
                (start + column.saturating_sub(1)).min(start + self.buffer.line_len_chars(line))
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
            Motion::SearchNext { .. } => return None,
        })
    }

    pub(super) fn normal_cursor_target(&self, at: usize) -> usize {
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

    pub(super) fn first_non_blank(&self, from: usize) -> usize {
        let (line, _) = self.buffer.char_to_coords(from);
        let base = self.buffer.line_to_char(line);
        let len = self.buffer.line_len_chars(line);
        for index in 0..len {
            let ch = self.buffer.rope.char(base + index);
            if ch != ' ' && ch != '\t' {
                return base + index;
            }
        }
        base
    }

    pub(super) fn word_prev(&self, from: usize, big: bool) -> usize {
        let mut index = from;
        while index > 0 && self.char_class(index - 1, big) == 0 {
            index -= 1;
        }
        if index == 0 {
            return 0;
        }
        let class = self.char_class(index - 1, big);
        while index > 0 && self.char_class(index - 1, big) == class {
            index -= 1;
        }
        index
    }

    fn screen_line(&self, offset: u16) -> usize {
        let line = self.fold_view.step_rows(self.top_row, offset as i64);
        self.first_non_blank(self.buffer.line_to_char(line as usize))
    }

    fn last_non_blank(&self, from: usize) -> usize {
        let (line, _) = self.buffer.char_to_coords(from);
        let base = self.buffer.line_to_char(line);
        let len = self.buffer.line_len_chars(line);
        let mut index = len;
        while index > 0 {
            let ch = self.buffer.rope.char(base + index - 1);
            if ch != ' ' && ch != '\t' {
                return base + index - 1;
            }
            index -= 1;
        }
        base
    }

    fn match_pair(&self, from: usize) -> Option<usize> {
        const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
        let (line, _) = self.buffer.char_to_coords(from);
        let base = self.buffer.line_to_char(line);
        let len = self.buffer.line_len_chars(line);
        let column = from - base;
        for index in column..len {
            let at = base + index;
            let ch = self.buffer.rope.char(at);
            if let Some((open, close)) = PAIRS.iter().find(|(open, _)| *open == ch) {
                return self.scan_pair(at, *open, *close, true);
            }
            if let Some((open, close)) = PAIRS.iter().find(|(_, close)| *close == ch) {
                return self.scan_pair(at, *open, *close, false);
            }
        }
        None
    }

    fn scan_pair(&self, at: usize, open: char, close: char, forward: bool) -> Option<usize> {
        let len = self.buffer.len_chars();
        let mut depth = 0i32;
        let mut index = at as i64;
        loop {
            let ch = self.buffer.rope.char(index as usize);
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
            }
            if depth == 0 && (ch == open || ch == close) && index as usize != at {
                return Some(index as usize);
            }
            index += if forward { 1 } else { -1 };
            if index < 0 || index as usize >= len {
                return None;
            }
        }
    }

    fn find_char(&self, from: usize, ch: char, forward: bool, till: bool) -> Option<usize> {
        let (line, _) = self.buffer.char_to_coords(from);
        let base = self.buffer.line_to_char(line);
        let len = self.buffer.line_len_chars(line);
        let column = from - base;
        if forward {
            let start = if till { column + 2 } else { column + 1 };
            for index in start..len {
                if self.buffer.rope.char(base + index) == ch {
                    return Some(base + if till { index - 1 } else { index });
                }
            }
        } else {
            let end = if till { column.checked_sub(1)? } else { column };
            for index in (0..end).rev() {
                if self.buffer.rope.char(base + index) == ch {
                    return Some(base + if till { index + 1 } else { index });
                }
            }
        }
        None
    }

    fn vertical(&self, from: usize, delta: i64) -> usize {
        let (line, column) = self.buffer.char_to_coords(from);
        let target = self.fold_view.step_rows(line as u32, delta) as usize;
        self.buffer.coords_to_char(target, column)
    }

    fn line_left(&self, from: usize) -> usize {
        let (line, column) = self.buffer.char_to_coords(from);
        let start = self.buffer.line_to_char(line);
        if column == 0 {
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

    fn char_class(&self, index: usize, big: bool) -> u8 {
        let ch = self.buffer.rope.char(index);
        if big {
            if ch.is_whitespace() { 0 } else { 1 }
        } else {
            TextObjectKind::char_class(ch)
        }
    }

    fn word_next(&self, from: usize, big: bool) -> usize {
        let len = self.buffer.len_chars();
        let mut index = from;
        if index >= len {
            return len;
        }
        let start_class = self.char_class(index, big);
        while index < len && self.char_class(index, big) == start_class && start_class != 0 {
            index += 1;
        }
        while index < len && self.char_class(index, big) == 0 {
            index += 1;
        }
        index
    }

    fn word_end(&self, from: usize, big: bool) -> usize {
        let len = self.buffer.len_chars();
        let mut index = (from + 1).min(len);
        while index < len && self.char_class(index, big) == 0 {
            index += 1;
        }
        if index >= len {
            return len;
        }
        let class = self.char_class(index, big);
        while index + 1 < len && self.char_class(index + 1, big) == class {
            index += 1;
        }
        index
    }

    fn word_end_prev(&self, from: usize, big: bool) -> usize {
        let len = self.buffer.len_chars();
        if from == 0 || len == 0 {
            return 0;
        }
        let mut index = from.min(len - 1);
        let start = self.char_class(index, big);
        if start != 0 {
            while index > 0 && self.char_class(index - 1, big) == start {
                index -= 1;
            }
        }
        if index == 0 {
            return 0;
        }
        index -= 1;
        while index > 0 && self.char_class(index, big) == 0 {
            index -= 1;
        }
        index
    }
}
