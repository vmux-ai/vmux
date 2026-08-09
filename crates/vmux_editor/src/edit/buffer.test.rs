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
