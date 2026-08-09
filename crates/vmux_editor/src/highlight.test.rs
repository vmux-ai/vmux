use super::*;

#[test]
fn highlights_rust_keywords_distinctly() {
    let hl = Highlighter::new();
    let out = hl.highlight("fn main() {}\n", std::path::Path::new("a.rs"));
    assert_eq!(out.language, "Rust");
    assert_eq!(out.lines.len(), 1);
    assert_eq!(out.lines[0].line_no, 0);
    let joined: String = out.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(joined.trim_end(), "fn main() {}");
    let distinct: std::collections::HashSet<_> = out.lines[0].spans.iter().map(|s| s.fg).collect();
    assert!(
        distinct.len() > 1,
        "expected multiple colors, got {distinct:?}"
    );
}

#[test]
fn recognizes_toml() {
    let hl = Highlighter::new();
    let out = hl.highlight(
        "[package]\nname = \"x\"\n",
        std::path::Path::new("Cargo.toml"),
    );
    assert_eq!(out.language, "TOML");
    let colors: std::collections::HashSet<_> = out
        .lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.fg))
        .collect();
    assert!(colors.len() > 1, "expected highlighting, got {colors:?}");
}

#[test]
fn recognizes_languages_beyond_syntect_defaults() {
    let hl = Highlighter::new();
    for (file, sample) in [
        ("a.ts", "const x = 1;\n"),
        ("a.tsx", "const x = <div/>;\n"),
        ("a.go", "package main\n"),
        ("a.py", "import os\n"),
        ("a.kt", "fun main() {}\n"),
        ("a.swift", "let x = 1\n"),
        ("a.zig", "const x = 1;\n"),
    ] {
        let out = hl.highlight(sample, std::path::Path::new(file));
        assert_ne!(out.language, "Plain Text", "{file} not recognized");
    }
}

#[test]
fn unknown_extension_is_plaintext_single_span() {
    let hl = Highlighter::new();
    let out = hl.highlight("just text\n", std::path::Path::new("notes.xyzzy"));
    assert_eq!(out.language, "Plain Text");
    assert_eq!(out.lines.len(), 1);
}

#[test]
fn line_count_matches_input() {
    let hl = Highlighter::new();
    let out = hl.highlight("a\nb\nc\n", std::path::Path::new("a.txt"));
    assert_eq!(out.lines.len(), 3);
    assert_eq!(out.lines[2].line_no, 2);
}

#[test]
fn load_rejects_missing_file() {
    let hl = Highlighter::new();
    let err = hl
        .load_file(std::path::Path::new("/no/such/file.rs"))
        .unwrap_err();
    assert!(err.contains("/no/such/file.rs"), "got: {err}");
}

#[test]
fn load_rejects_directory() {
    let hl = Highlighter::new();
    let dir = std::env::temp_dir();
    let err = hl.load_file(&dir).unwrap_err();
    assert!(err.to_lowercase().contains("not a file"), "got: {err}");
}

#[test]
fn load_reads_and_highlights() {
    let hl = Highlighter::new();
    let mut p = std::env::temp_dir();
    p.push(format!("vmux-editor-{}.rs", std::process::id()));
    std::fs::write(&p, "fn x() {}\n").unwrap();
    let out = hl.load_file(&p).unwrap();
    let _ = std::fs::remove_file(&p);
    assert_eq!(out.language, "Rust");
    assert_eq!(out.lines.len(), 1);
}
