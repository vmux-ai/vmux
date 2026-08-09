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
