use std::ops::Range;

use crate::edit::buffer::TextBuffer;

pub fn char_class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextObjectKind {
    Word,
    BigWord,
    Sentence,
    Paragraph,
    Paren,
    Bracket,
    Brace,
    Angle,
    DoubleQuote,
    SingleQuote,
    BackQuote,
    Tag,
}

impl TextObjectKind {
    fn delimiters(self) -> Option<(char, char)> {
        Some(match self {
            TextObjectKind::Paren => ('(', ')'),
            TextObjectKind::Bracket => ('[', ']'),
            TextObjectKind::Brace => ('{', '}'),
            TextObjectKind::Angle => ('<', '>'),
            _ => return None,
        })
    }

    fn quote(self) -> Option<char> {
        Some(match self {
            TextObjectKind::DoubleQuote => '"',
            TextObjectKind::SingleQuote => '\'',
            TextObjectKind::BackQuote => '`',
            _ => return None,
        })
    }

    pub fn is_linewise(self) -> bool {
        matches!(self, TextObjectKind::Paragraph)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextObject {
    pub kind: TextObjectKind,
    pub around: bool,
    pub count: usize,
}

pub fn resolve(buf: &TextBuffer, head: usize, obj: TextObject) -> Option<Range<usize>> {
    let count = obj.count.max(1);
    match obj.kind {
        TextObjectKind::Word => word(buf, head, false, obj.around),
        TextObjectKind::BigWord => word(buf, head, true, obj.around),
        TextObjectKind::Sentence => sentence(buf, head, obj.around),
        TextObjectKind::Paragraph => paragraph(buf, head, obj.around),
        TextObjectKind::Tag => tag(buf, head, obj.around),
        kind => {
            if let Some((open, close)) = kind.delimiters() {
                let (o, c) = enclosing_pair(buf, head, open, close, count)?;
                return Some(if obj.around { o..c + 1 } else { o + 1..c });
            }
            let q = kind.quote()?;
            let (o, c) = quoted(buf, head, q)?;
            Some(if obj.around { o..c + 1 } else { o + 1..c })
        }
    }
}

fn word(buf: &TextBuffer, head: usize, big: bool, around: bool) -> Option<Range<usize>> {
    let len = buf.len_chars();
    if len == 0 {
        return None;
    }
    let head = head.min(len - 1);
    let class_at = |i: usize| -> u8 {
        let c = buf.rope.char(i);
        if c == '\n' {
            return 0;
        }
        if big {
            if c.is_whitespace() { 0 } else { 1 }
        } else {
            char_class(c)
        }
    };
    let base = class_at(head);
    let mut start = head;
    while start > 0 && class_at(start - 1) == base && buf.rope.char(start - 1) != '\n' {
        start -= 1;
    }
    let mut end = head + 1;
    while end < len && class_at(end) == base && buf.rope.char(end) != '\n' {
        end += 1;
    }
    if !around {
        return Some(start..end);
    }
    let mut trailing = end;
    while trailing < len && buf.rope.char(trailing) != '\n' && class_at(trailing) == 0 {
        trailing += 1;
    }
    if trailing > end {
        return Some(start..trailing);
    }
    let mut leading = start;
    while leading > 0 && buf.rope.char(leading - 1) != '\n' && class_at(leading - 1) == 0 {
        leading -= 1;
    }
    Some(leading..end)
}

fn sentence(buf: &TextBuffer, head: usize, around: bool) -> Option<Range<usize>> {
    let len = buf.len_chars();
    if len == 0 {
        return None;
    }
    let head = head.min(len - 1);
    let terminator = |i: usize| {
        let c = buf.rope.char(i);
        c == '.' || c == '!' || c == '?'
    };
    let mut start = head;
    while start > 0 {
        let prev = start - 1;
        if buf.rope.char(prev).is_whitespace()
            && prev > 0
            && terminator(prev - 1)
            && prev.saturating_sub(1) < head
        {
            break;
        }
        if buf.rope.char(prev) == '\n' && prev > 0 && buf.rope.char(prev - 1) == '\n' {
            break;
        }
        start = prev;
    }
    let mut end = head;
    while end < len && !terminator(end) {
        end += 1;
    }
    if end < len {
        end += 1;
    }
    if around {
        while end < len && buf.rope.char(end) == ' ' {
            end += 1;
        }
    }
    (start < end).then_some(start..end)
}

fn paragraph(buf: &TextBuffer, head: usize, around: bool) -> Option<Range<usize>> {
    let total = buf.len_lines();
    if total == 0 {
        return None;
    }
    let (line, _) = buf.char_to_coords(head);
    let blank = |l: usize| buf.line_len_chars(l) == 0;
    let base = blank(line);
    let mut first = line;
    while first > 0 && blank(first - 1) == base {
        first -= 1;
    }
    let mut last = line;
    while last + 1 < total && blank(last + 1) == base {
        last += 1;
    }
    if around {
        while last + 1 < total && blank(last + 1) != base {
            last += 1;
        }
    }
    let start = buf.line_to_char(first);
    let end = if last + 1 < total {
        buf.line_to_char(last + 1)
    } else {
        buf.len_chars()
    };
    (start < end).then_some(start..end)
}

fn enclosing_pair(
    buf: &TextBuffer,
    head: usize,
    open: char,
    close: char,
    count: usize,
) -> Option<(usize, usize)> {
    let mut found = matching_pair(buf, head, open, close)?;
    for _ in 1..count {
        if found.0 == 0 {
            break;
        }
        found = matching_pair(buf, found.0 - 1, open, close)?;
    }
    Some(found)
}

fn matching_pair(buf: &TextBuffer, head: usize, open: char, close: char) -> Option<(usize, usize)> {
    let len = buf.len_chars();
    if len == 0 {
        return None;
    }
    let head = head.min(len - 1);
    let open_at = if buf.rope.char(head) == open {
        Some(head)
    } else {
        let mut depth = 0i32;
        let mut found = None;
        let mut i = head as i64;
        while i >= 0 {
            let c = buf.rope.char(i as usize);
            if c == close && i as usize != head {
                depth += 1;
            } else if c == open {
                if depth == 0 {
                    found = Some(i as usize);
                    break;
                }
                depth -= 1;
            }
            i -= 1;
        }
        found
    }?;
    let mut depth = 0i32;
    let mut j = open_at + 1;
    while j < len {
        let c = buf.rope.char(j);
        if c == open {
            depth += 1;
        } else if c == close {
            if depth == 0 {
                return Some((open_at, j));
            }
            depth -= 1;
        }
        j += 1;
    }
    None
}

fn quoted(buf: &TextBuffer, head: usize, q: char) -> Option<(usize, usize)> {
    let (line, _) = buf.char_to_coords(head);
    let base = buf.line_to_char(line);
    let len = buf.line_len_chars(line);
    let mut marks = Vec::new();
    let mut i = 0;
    while i < len {
        let c = buf.rope.char(base + i);
        if c == '\\' {
            i += 2;
            continue;
        }
        if c == q {
            marks.push(base + i);
        }
        i += 1;
    }
    marks
        .chunks(2)
        .find(|pair| pair.len() == 2 && head <= pair[1])
        .map(|pair| (pair[0], pair[1]))
}

struct TagSpan {
    outer: Range<usize>,
    inner: Range<usize>,
}

fn tag(buf: &TextBuffer, head: usize, around: bool) -> Option<Range<usize>> {
    let len = buf.len_chars();
    let mut stack: Vec<(String, usize, usize)> = Vec::new();
    let mut spans: Vec<TagSpan> = Vec::new();
    let mut i = 0;
    while i < len {
        if buf.rope.char(i) != '<' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < len && buf.rope.char(j) != '>' {
            j += 1;
        }
        if j >= len {
            break;
        }
        let raw: String = buf.rope.slice(i + 1..j).chars().collect();
        let self_closing = raw.ends_with('/');
        let closing = raw.starts_with('/');
        let name: String = raw
            .trim_start_matches('/')
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '/')
            .collect();
        if !name.is_empty() {
            if closing {
                if let Some(pos) = stack.iter().rposition(|(n, _, _)| *n == name) {
                    let (_, open_start, inner_start) = stack.remove(pos);
                    stack.truncate(pos);
                    spans.push(TagSpan {
                        outer: open_start..j + 1,
                        inner: inner_start..i,
                    });
                }
            } else if !self_closing {
                stack.push((name, i, j + 1));
            }
        }
        i = j + 1;
    }
    spans
        .into_iter()
        .filter(|s| s.outer.contains(&head))
        .min_by_key(|s| s.outer.end - s.outer.start)
        .map(|s| if around { s.outer } else { s.inner })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn buf(text: &str) -> TextBuffer {
        TextBuffer::from_text(PathBuf::from("a.txt"), "Plain Text".into(), text)
    }

    fn slice(text: &str, head: usize, kind: TextObjectKind, around: bool) -> Option<String> {
        let b = buf(text);
        let r = resolve(
            &b,
            head,
            TextObject {
                kind,
                around,
                count: 1,
            },
        )?;
        Some(b.rope.slice(r).chars().collect())
    }

    #[test]
    fn inner_word_stops_at_class_boundaries() {
        assert_eq!(
            slice("foo bar", 1, TextObjectKind::Word, false).as_deref(),
            Some("foo")
        );
        assert_eq!(
            slice("foo.bar", 1, TextObjectKind::Word, false).as_deref(),
            Some("foo")
        );
        assert_eq!(
            slice("foo.bar", 3, TextObjectKind::Word, false).as_deref(),
            Some(".")
        );
    }

    #[test]
    fn around_word_takes_trailing_then_leading_space() {
        assert_eq!(
            slice("foo bar", 1, TextObjectKind::Word, true).as_deref(),
            Some("foo ")
        );
        assert_eq!(
            slice("foo bar", 5, TextObjectKind::Word, true).as_deref(),
            Some(" bar")
        );
    }

    #[test]
    fn big_word_spans_punctuation() {
        assert_eq!(
            slice("foo.bar baz", 1, TextObjectKind::BigWord, false).as_deref(),
            Some("foo.bar")
        );
    }

    #[test]
    fn bracket_objects_respect_nesting() {
        assert_eq!(
            slice("a(b(c)d)e", 4, TextObjectKind::Paren, false).as_deref(),
            Some("c")
        );
        assert_eq!(
            slice("a(b(c)d)e", 2, TextObjectKind::Paren, false).as_deref(),
            Some("b(c)d")
        );
        assert_eq!(
            slice("a(b(c)d)e", 2, TextObjectKind::Paren, true).as_deref(),
            Some("(b(c)d)")
        );
    }

    #[test]
    fn counted_bracket_object_walks_outward() {
        let b = buf("a(b(c)d)e");
        let r = resolve(
            &b,
            4,
            TextObject {
                kind: TextObjectKind::Paren,
                around: false,
                count: 2,
            },
        )
        .unwrap();
        let got: String = b.rope.slice(r).chars().collect();
        assert_eq!(got, "b(c)d");
    }

    #[test]
    fn cursor_on_a_delimiter_selects_that_pair() {
        assert_eq!(
            slice("a(bc)d", 1, TextObjectKind::Paren, false).as_deref(),
            Some("bc")
        );
        assert_eq!(
            slice("a(bc)d", 4, TextObjectKind::Paren, false).as_deref(),
            Some("bc")
        );
    }

    #[test]
    fn quote_objects_pair_left_to_right() {
        assert_eq!(
            slice("say \"hi\" now", 6, TextObjectKind::DoubleQuote, false).as_deref(),
            Some("hi")
        );
        assert_eq!(
            slice("say \"hi\" now", 0, TextObjectKind::DoubleQuote, true).as_deref(),
            Some("\"hi\"")
        );
    }

    #[test]
    fn escaped_quotes_do_not_split_the_span() {
        assert_eq!(
            slice("x \"a\\\"b\" y", 3, TextObjectKind::DoubleQuote, false).as_deref(),
            Some("a\\\"b")
        );
    }

    #[test]
    fn paragraph_objects_use_blank_line_boundaries() {
        let text = "one\ntwo\n\nthree\n";
        assert_eq!(
            slice(text, 0, TextObjectKind::Paragraph, false).as_deref(),
            Some("one\ntwo\n")
        );
        assert_eq!(
            slice(text, 0, TextObjectKind::Paragraph, true).as_deref(),
            Some("one\ntwo\n\n")
        );
    }

    #[test]
    fn tag_objects_pick_the_innermost_match() {
        let text = "<a><b>hi</b></a>";
        assert_eq!(
            slice(text, 7, TextObjectKind::Tag, false).as_deref(),
            Some("hi")
        );
        assert_eq!(
            slice(text, 7, TextObjectKind::Tag, true).as_deref(),
            Some("<b>hi</b>")
        );
    }

    #[test]
    fn self_closing_tags_are_not_opened() {
        let text = "<a>x<br/>y</a>";
        assert_eq!(
            slice(text, 5, TextObjectKind::Tag, false).as_deref(),
            Some("x<br/>y")
        );
    }

    #[test]
    fn sentence_object_covers_one_sentence() {
        let text = "One two. Three four. Five.";
        assert_eq!(
            slice(text, 10, TextObjectKind::Sentence, false).as_deref(),
            Some("Three four.")
        );
        assert_eq!(
            slice(text, 10, TextObjectKind::Sentence, true).as_deref(),
            Some("Three four. ")
        );
    }

    #[test]
    fn unmatched_delimiters_resolve_to_nothing() {
        assert_eq!(slice("a(bc", 2, TextObjectKind::Paren, false), None);
        assert_eq!(slice("abc", 1, TextObjectKind::Paren, false), None);
    }
}
