use super::*;
use vmux_core::event::TermSpan;

fn line_of(text: &str) -> TermLine {
    TermLine {
        spans: vec![TermSpan {
            text: text.to_string(),
            col: 0,
            grid_cols: text.chars().count() as u16,
            ..Default::default()
        }],
        links: Vec::new(),
    }
}

#[test]
fn detects_https_url() {
    let mut l = line_of("see https://vmux.ai/docs now");
    annotate_links(&mut l, None);
    assert_eq!(l.links.len(), 1);
    assert_eq!(l.links[0].url, "https://vmux.ai/docs");
    assert_eq!(l.links[0].start_col, 4);
    assert_eq!(l.links[0].end_col, 23);
}

#[test]
fn trims_trailing_punctuation() {
    let mut l = line_of("docs at https://vmux.ai/docs.");
    annotate_links(&mut l, None);
    assert_eq!(l.links[0].url, "https://vmux.ai/docs");
}

#[test]
fn trims_wrapping_parens() {
    let mut l = line_of("(https://vmux.ai/x)");
    annotate_links(&mut l, None);
    assert_eq!(l.links.len(), 1);
    assert_eq!(l.links[0].url, "https://vmux.ai/x");
}

#[test]
fn detects_absolute_path() {
    let mut l = line_of("edit /Users/me/main.rs please");
    annotate_links(&mut l, None);
    assert_eq!(l.links.len(), 1);
    assert_eq!(l.links[0].url, "file:///Users/me/main.rs");
}

#[test]
fn ignores_ascii_art_path_punctuation() {
    let mut line = line_of(r"/ \/ /\\ \\// |/\\| ////");

    annotate_links(&mut line, None);

    assert!(line.links.is_empty());
}

#[test]
fn resolves_relative_path_against_cwd() {
    let mut l = line_of("see crates/foo.rs");
    annotate_links(&mut l, Some(Path::new("/work")));
    assert_eq!(l.links[0].url, "file:///work/crates/foo.rs");
}

#[test]
fn skips_relative_path_without_cwd() {
    let mut l = line_of("see crates/foo.rs");
    annotate_links(&mut l, None);
    assert!(l.links.is_empty());
}

#[test]
fn does_not_treat_bare_filename_as_url() {
    let mut l = line_of("opened foo.txt and Cargo.toml");
    annotate_links(&mut l, None);
    assert!(l.links.is_empty());
}

#[test]
fn ignores_bare_words() {
    let mut l = line_of("hello world this is prose");
    annotate_links(&mut l, None);
    assert!(l.links.is_empty());
}

#[test]
fn multiple_links_one_line() {
    let mut l = line_of("https://a.com and https://b.com");
    annotate_links(&mut l, None);
    assert_eq!(l.links.len(), 2);
    assert_eq!(l.links[0].url, "https://a.com");
    assert_eq!(l.links[1].url, "https://b.com");
}

#[test]
fn wide_chars_shift_columns() {
    // 'あ' is width 2; the URL starts at col 0 + 2 + 1(space) = 3.
    let mut l = line_of("あ https://x.io");
    annotate_links(&mut l, None);
    assert_eq!(l.links.len(), 1);
    assert_eq!(l.links[0].start_col, 3);
}
