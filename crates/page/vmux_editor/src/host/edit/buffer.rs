use std::path::PathBuf;

use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

pub struct TextBuffer {
    pub rope: Rope,
    pub path: PathBuf,
    pub language: String,
}

/// Why a server's [`lsp_types::WorkspaceEdit`] could not be applied.
///
/// Both cases are the server violating the protocol, so the answer is a refusal rather than a
/// best-effort partial application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspEditError {
    Inverted,
    Overlapping,
}

impl std::fmt::Display for LspEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inverted => f.write_str("edit range ends before it starts"),
            Self::Overlapping => f.write_str("edit ranges overlap"),
        }
    }
}

impl TextBuffer {
    pub fn from_text(path: PathBuf, language: String, text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            path,
            language,
        }
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines().max(1)
    }

    pub fn char_to_line(&self, char_idx: usize) -> usize {
        self.rope.char_to_line(char_idx.min(self.len_chars()))
    }

    pub fn line_to_char(&self, line: usize) -> usize {
        let line = line.min(self.len_lines().saturating_sub(1));
        self.rope.line_to_char(line)
    }

    pub fn line_len_chars(&self, line: usize) -> usize {
        if line >= self.len_lines() {
            return 0;
        }
        let slice = self.rope.line(line);
        let mut n = slice.len_chars();
        if n > 0 && slice.char(n - 1) == '\n' {
            n -= 1;
            if n > 0 && slice.char(n - 1) == '\r' {
                n -= 1;
            }
        }
        n
    }

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

    /// Convert an LSP `Position` to a char index.
    ///
    /// LSP columns count UTF-16 code units; every other column in this crate is a char index,
    /// so a line holding anything outside the BMP disagrees with [`Self::coords_to_char`].
    pub fn lsp_position_to_char(&self, position: lsp_types::Position) -> usize {
        let line = (position.line as usize).min(self.len_lines().saturating_sub(1));
        let base = self.rope.line_to_char(line);
        let len = self.line_len_chars(line);
        let mut utf16 = 0u32;
        let mut chars = 0usize;
        for ch in self.rope.line(line).chars().take(len) {
            if utf16 >= position.character {
                break;
            }
            utf16 += ch.len_utf16() as u32;
            chars += 1;
        }
        base + chars.min(len)
    }

    /// The text this buffer would hold with `edits` applied, leaving the buffer untouched.
    ///
    /// Sorting and rejecting overlap is the client's job: LSP guarantees a server's edits do not
    /// overlap but says nothing about their order, and applying them back-to-front is what keeps
    /// earlier offsets valid.
    pub fn with_lsp_edits(&self, edits: &[lsp_types::TextEdit]) -> Result<String, LspEditError> {
        let mut ranges = Vec::with_capacity(edits.len());
        for edit in edits {
            let start = self.lsp_position_to_char(edit.range.start);
            let end = self.lsp_position_to_char(edit.range.end);
            if end < start {
                return Err(LspEditError::Inverted);
            }
            ranges.push((start, end, edit.new_text.as_str()));
        }
        ranges.sort_by_key(|(start, _, _)| *start);
        for pair in ranges.windows(2) {
            if pair[0].1 > pair[1].0 {
                return Err(LspEditError::Overlapping);
            }
        }
        let mut rope = self.rope.clone();
        for (start, end, text) in ranges.into_iter().rev() {
            rope.remove(start..end);
            rope.insert(start, text);
        }
        Ok(rope.to_string())
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.rope.insert(char_idx.min(self.len_chars()), text);
    }

    pub fn remove(&mut self, range: std::ops::Range<usize>) {
        let end = range.end.min(self.len_chars());
        let start = range.start.min(end);
        self.rope.remove(start..end);
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn next_grapheme(&self, char_idx: usize) -> usize {
        let len = self.len_chars();
        if char_idx >= len {
            return len;
        }
        let line = self.char_to_line(char_idx);
        let line_start = self.rope.line_to_char(line);
        let line_str: String = self.rope.line(line).chars().collect();
        let off = char_idx - line_start;
        let mut acc = 0usize;
        for g in line_str.graphemes(true) {
            let glen = g.chars().count();
            if acc <= off && off < acc + glen {
                return (line_start + acc + glen).min(len);
            }
            acc += glen;
        }
        (char_idx + 1).min(len)
    }

    pub fn prev_grapheme(&self, char_idx: usize) -> usize {
        if char_idx == 0 {
            return 0;
        }
        let char_idx = char_idx.min(self.len_chars());
        let line = self.char_to_line(char_idx);
        let line_start = self.rope.line_to_char(line);
        if char_idx == line_start {
            return char_idx - 1;
        }
        let line_str: String = self.rope.line(line).chars().collect();
        let off = char_idx - line_start;
        let mut acc = 0usize;
        let mut prev = 0usize;
        for g in line_str.graphemes(true) {
            let glen = g.chars().count();
            if acc + glen >= off {
                return line_start + prev;
            }
            prev = acc + glen;
            acc += glen;
        }
        line_start + prev
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
        assert_eq!(b.text(), "abc");
        b.remove(1..2);
        assert_eq!(b.text(), "ac");
    }

    #[test]
    fn next_grapheme_advances_one() {
        let b = buf("あb");
        assert_eq!(b.next_grapheme(0), 1);
        assert_eq!(b.next_grapheme(1), 2);
        assert_eq!(b.next_grapheme(2), 2);
    }

    fn edit(start: (u32, u32), end: (u32, u32), new_text: &str) -> lsp_types::TextEdit {
        lsp_types::TextEdit {
            range: lsp_types::Range {
                start: lsp_types::Position {
                    line: start.0,
                    character: start.1,
                },
                end: lsp_types::Position {
                    line: end.0,
                    character: end.1,
                },
            },
            new_text: new_text.to_string(),
        }
    }

    #[test]
    fn later_edits_do_not_shift_earlier_ones() {
        let b = buf("one two three\n");
        let out = b
            .with_lsp_edits(&[
                edit((0, 0), (0, 3), "1"),
                edit((0, 8), (0, 13), "3"),
                edit((0, 4), (0, 7), "2"),
            ])
            .unwrap();
        assert_eq!(out, "1 2 3\n");
        assert_eq!(b.text(), "one two three\n", "source buffer is untouched");
    }

    /// A rename lands on a line holding an emoji whenever the file has one above the caret.
    /// Reading the LSP column as a char index puts the edit in the wrong place.
    #[test]
    fn columns_are_utf16_not_chars() {
        let b = buf("let 😀 = old;\n");
        let out = b.with_lsp_edits(&[edit((0, 9), (0, 12), "new")]).unwrap();
        assert_eq!(out, "let 😀 = new;\n");
    }

    #[test]
    fn overlapping_edits_are_refused() {
        let b = buf("abcdef\n");
        let err = b
            .with_lsp_edits(&[edit((0, 0), (0, 4), "x"), edit((0, 2), (0, 6), "y")])
            .unwrap_err();
        assert_eq!(err, LspEditError::Overlapping);
    }

    #[test]
    fn touching_edits_are_allowed() {
        let b = buf("abcdef\n");
        let out = b
            .with_lsp_edits(&[edit((0, 0), (0, 3), "x"), edit((0, 3), (0, 6), "y")])
            .unwrap();
        assert_eq!(out, "xy\n");
    }

    #[test]
    fn insertion_past_the_last_line_appends() {
        let b = buf("a\n");
        let out = b.with_lsp_edits(&[edit((9, 0), (9, 0), "b\n")]).unwrap();
        assert_eq!(out, "a\nb\n");
    }

    #[test]
    fn crlf_line_endings_survive_an_edit() {
        let b = buf("one\r\ntwo\r\n");
        let out = b.with_lsp_edits(&[edit((1, 0), (1, 3), "2")]).unwrap();
        assert_eq!(out, "one\r\n2\r\n");
    }

    #[test]
    fn two_insertions_at_one_offset_keep_their_given_order() {
        let b = buf("ac\n");
        let out = b
            .with_lsp_edits(&[edit((0, 1), (0, 1), "b1"), edit((0, 1), (0, 1), "b2")])
            .unwrap();
        assert_eq!(out, "ab1b2c\n");
    }
}
