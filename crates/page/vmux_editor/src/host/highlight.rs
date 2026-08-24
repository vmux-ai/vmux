use std::path::Path;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use vmux_core::event::{FileLine, StyledSpan};

/// Where the editor stops opening a file at all, matching the cap VS Code's text model has.
///
/// Well past the point where the features come off: a rope of this size is affordable, and a
/// user who asked for a file would rather read it plainly than be told no.
pub const FILE_VIEW_MAX_BYTES: u64 = 50 * 1024 * 1024;

/// Where syntax highlighting, folding and the language server come off.
///
/// `HighlightCache` keeps a syntect parser state *per line*, so what actually bites on a large
/// file is highlighting rather than the text. Past this the file still opens and still edits, it
/// is just uncoloured — which is what VS Code's `editor.largeFileOptimizations` does.
pub const HIGHLIGHT_MAX_BYTES: u64 = 5 * 1024 * 1024;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

pub fn syntax_set() -> &'static SyntaxSet {
    syntaxes()
}

pub fn select_syntax(path: &Path) -> &'static syntect::parsing::SyntaxReference {
    let ss = syntaxes();
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .unwrap_or_else(|| ss.find_syntax_plain_text())
}

use std::sync::atomic::{AtomicBool, Ordering};

static DARK_THEME: AtomicBool = AtomicBool::new(true);

pub fn set_dark_theme(dark: bool) -> bool {
    DARK_THEME.swap(dark, Ordering::Relaxed) != dark
}

pub fn is_dark_theme() -> bool {
    DARK_THEME.load(Ordering::Relaxed)
}

fn theme_name() -> &'static str {
    if is_dark_theme() {
        "base16-ocean.dark"
    } else {
        "base16-ocean.light"
    }
}

pub fn default_theme() -> syntect::highlighting::Theme {
    ThemeSet::load_defaults().themes[theme_name()].clone()
}

/// The theme's plain text colour, for text served without highlighting.
pub fn theme_foreground(theme: &syntect::highlighting::Theme) -> [u8; 3] {
    theme
        .settings
        .foreground
        .map(|c| [c.r, c.g, c.b])
        .unwrap_or([0xc0, 0xc5, 0xce])
}

pub(crate) fn styled_span(style: Style, text: &str) -> StyledSpan {
    to_styled_span(style, text)
}

pub fn highlight_snippet(code: &str, lang_token: &str) -> Vec<FileLine> {
    let ss = syntaxes();
    let syntax = ss
        .find_syntax_by_token(lang_token)
        .or_else(|| ss.find_syntax_by_extension(lang_token))
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let theme = default_theme();
    let mut h = HighlightLines::new(syntax, &theme);
    LinesWithEndings::from(code)
        .enumerate()
        .map(|(idx, line)| {
            let ranges = h.highlight_line(line, ss).unwrap_or_default();
            FileLine {
                line_no: idx as u32,
                fold: vmux_core::event::FoldGutter::None,
                spans: ranges
                    .into_iter()
                    .map(|(style, text)| to_styled_span(style, text))
                    .filter(|s| !s.text.is_empty())
                    .collect(),
            }
        })
        .collect()
}

#[derive(Debug)]
pub struct HighlightedFile {
    pub language: String,
    pub lines: Vec<FileLine>,
}

pub struct Highlighter {
    themes: ThemeSet,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            themes: ThemeSet::load_defaults(),
        }
    }

    pub fn highlight(&self, content: &str, path: &Path) -> HighlightedFile {
        let syntaxes = syntaxes();
        let syntax = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|ext| syntaxes.find_syntax_by_extension(ext))
            .unwrap_or_else(|| syntaxes.find_syntax_plain_text());
        let theme = &self.themes.themes[theme_name()];
        let mut h = HighlightLines::new(syntax, theme);

        let mut lines = Vec::new();
        for (idx, line) in LinesWithEndings::from(content).enumerate() {
            let ranges: Vec<(Style, &str)> = h.highlight_line(line, syntaxes).unwrap_or_default();
            let spans = ranges
                .into_iter()
                .map(|(style, text)| to_styled_span(style, text))
                .filter(|s| !s.text.is_empty())
                .collect();
            lines.push(FileLine {
                line_no: idx as u32,
                fold: vmux_core::event::FoldGutter::None,
                spans,
            });
        }
        HighlightedFile {
            language: syntax.name.clone(),
            lines,
        }
    }

    pub fn load_file(&self, path: &Path) -> Result<HighlightedFile, String> {
        let meta =
            std::fs::metadata(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        if !meta.is_file() {
            return Err(format!("not a file: {}", path.display()));
        }
        if meta.len() > FILE_VIEW_MAX_BYTES {
            return Err(format!(
                "file too large ({} bytes, max {})",
                meta.len(),
                FILE_VIEW_MAX_BYTES
            ));
        }
        let bytes =
            std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let content = String::from_utf8(bytes)
            .map_err(|_| format!("not a UTF-8 text file: {}", path.display()))?;
        if meta.len() > HIGHLIGHT_MAX_BYTES {
            return Ok(self.plain(&content, path));
        }
        Ok(self.highlight(&content, path))
    }

    /// The text, one span per line, in the theme's plain colour.
    ///
    /// The same trade the editor makes past [`HIGHLIGHT_MAX_BYTES`], for the callers that hand a
    /// whole file to syntect in one go rather than a window at a time. Without it, raising the
    /// open limit to fifty megabytes raised the highlighting limit with it.
    fn plain(&self, content: &str, path: &Path) -> HighlightedFile {
        let theme = &self.themes.themes[theme_name()];
        let fg = theme_foreground(theme);
        let lines = LinesWithEndings::from(content)
            .enumerate()
            .map(|(idx, line)| FileLine {
                line_no: idx as u32,
                fold: vmux_core::event::FoldGutter::None,
                spans: vec![StyledSpan {
                    text: line.trim_end_matches(['\n', '\r']).to_string(),
                    fg,
                    bold: false,
                    italic: false,
                }],
            })
            .collect();
        HighlightedFile {
            language: select_syntax(path).name.clone(),
            lines,
        }
    }
}

fn to_styled_span(style: Style, text: &str) -> StyledSpan {
    StyledSpan {
        text: text.trim_end_matches(['\n', '\r']).to_string(),
        fg: [style.foreground.r, style.foreground.g, style.foreground.b],
        bold: style.font_style.contains(FontStyle::BOLD),
        italic: style.font_style.contains(FontStyle::ITALIC),
    }
}

#[cfg(test)]
mod tests {
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
        let distinct: std::collections::HashSet<_> =
            out.lines[0].spans.iter().map(|s| s.fg).collect();
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

    /// Raising the open limit to fifty megabytes must not raise the highlighting limit with it:
    /// syntect over a file this size is what the cap exists to prevent.
    #[test]
    fn load_serves_a_file_past_the_highlight_cap_without_colouring_it() {
        let hl = Highlighter::new();
        let mut p = std::env::temp_dir();
        p.push(format!("vmux-editor-large-{}.rs", std::process::id()));
        let line = "fn main() { let x = 1; }\n";
        std::fs::write(
            &p,
            line.repeat(1 + HIGHLIGHT_MAX_BYTES as usize / line.len()),
        )
        .unwrap();
        let out = hl.load_file(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(out.language, "Rust", "the language is still recognised");
        assert!(
            out.lines.iter().all(|l| l.spans.len() <= 1),
            "a line past the cap is one span, not a syntect parse"
        );
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
}
