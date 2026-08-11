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
}
