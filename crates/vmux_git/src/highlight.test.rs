use super::*;

#[test]
fn highlights_per_line_with_colors() {
    let lines = highlight_file("fn main() {}\n", Path::new("a.rs"));
    assert_eq!(lines.len(), 1);
    let colors: std::collections::HashSet<_> = lines[0].iter().map(|s| s.fg).collect();
    assert!(colors.len() > 1, "expected multiple colors");
}

#[test]
fn single_line_independent() {
    let spans = highlight_line("let x = 1;", Path::new("a.rs"));
    let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(joined, "let x = 1;");
}
