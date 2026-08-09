use super::*;

fn rope(s: &str) -> Rope {
    Rope::from_str(s)
}

#[test]
fn window_line_numbers_and_text() {
    let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
    let r = rope("fn a() {}\nlet x = 1;\nstruct S;\n");
    let w = c.line_window(&r, 1, 3);
    assert_eq!(w.len(), 2);
    assert_eq!(w[0].line_no, 1);
    let joined: String = w[0].spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(joined.trim_end(), "let x = 1;");
}

#[test]
fn invalidate_recomputes_tail() {
    let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
    let mut r = rope("let a = 1;\nlet b = 2;\nlet c = 3;\n");
    let _ = c.line_window(&r, 0, 3);
    r.insert(0, "// ");
    c.invalidate_from(0);
    let w = c.line_window(&r, 2, 3);
    let joined: String = w[0].spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(joined.trim_end(), "let c = 3;");
}

#[test]
fn multicolor_for_code() {
    let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
    let r = rope("fn main() {}\n");
    let w = c.line_window(&r, 0, 1);
    let colors: std::collections::HashSet<_> = w[0].spans.iter().map(|s| s.fg).collect();
    assert!(colors.len() > 1);
}
