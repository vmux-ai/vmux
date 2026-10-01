use std::path::Path;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use vmux_ecs::event::{FileLine, StyledSpan};

use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) const FILE_VIEW_MAX_BYTES: u64 = 50 * 1024 * 1024;

pub(crate) const HIGHLIGHT_MAX_BYTES: u64 = 5 * 1024 * 1024;

static DARK_THEME: AtomicBool = AtomicBool::new(true);

#[derive(Debug)]
pub(crate) struct HighlightedFile {
    pub lines: Vec<FileLine>,
    pub encoding: vmux_ecs::event::FileEncoding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoadError {
    Binary,
    Unreadable(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Binary => f.write_str("not a text file"),
            Self::Unreadable(message) => f.write_str(message),
        }
    }
}

pub(crate) struct Highlighter;

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn syntax_set() -> &'static SyntaxSet {
        static SET: OnceLock<SyntaxSet> = OnceLock::new();
        SET.get_or_init(two_face::syntax::extra_newlines)
    }

    pub(crate) fn syntax(path: &Path) -> &'static syntect::parsing::SyntaxReference {
        let syntaxes = Self::syntax_set();
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(|ext| syntaxes.find_syntax_by_extension(ext))
            .unwrap_or_else(|| syntaxes.find_syntax_plain_text())
    }

    pub(crate) fn set_dark(dark: bool) -> bool {
        DARK_THEME.swap(dark, Ordering::Relaxed) != dark
    }

    pub(crate) fn is_dark() -> bool {
        DARK_THEME.load(Ordering::Relaxed)
    }

    pub(crate) fn theme() -> syntect::highlighting::Theme {
        crate::palette::Palette::for_scheme(Self::is_dark()).theme()
    }

    pub(crate) fn foreground(theme: &syntect::highlighting::Theme) -> [u8; 3] {
        theme
            .settings
            .foreground
            .map(|color| [color.r, color.g, color.b])
            .unwrap_or_else(|| {
                crate::palette::Palette::for_scheme(Self::is_dark()).foreground_rgb()
            })
    }

    pub(crate) fn span(style: Style, text: &str) -> StyledSpan {
        StyledSpan {
            text: text.trim_end_matches(['\n', '\r']).to_string(),
            fg: [style.foreground.r, style.foreground.g, style.foreground.b],
            bold: style.font_style.contains(FontStyle::BOLD),
            italic: style.font_style.contains(FontStyle::ITALIC),
        }
    }

    pub(crate) fn snippet(code: &str, lang_token: &str) -> Vec<FileLine> {
        let syntaxes = Self::syntax_set();
        let syntax = syntaxes
            .find_syntax_by_token(lang_token)
            .or_else(|| syntaxes.find_syntax_by_extension(lang_token))
            .unwrap_or_else(|| syntaxes.find_syntax_plain_text());
        let theme = Self::theme();
        let mut highlighter = HighlightLines::new(syntax, &theme);
        LinesWithEndings::from(code)
            .enumerate()
            .map(|(index, line)| {
                let ranges = highlighter
                    .highlight_line(line, syntaxes)
                    .unwrap_or_default();
                FileLine {
                    line_no: index as u32,
                    fold: vmux_ecs::event::FoldGutter::None,
                    indent_levels: 0,
                    spans: ranges
                        .into_iter()
                        .map(|(style, text)| Self::span(style, text))
                        .filter(|span| !span.text.is_empty())
                        .collect(),
                }
            })
            .collect()
    }

    pub(crate) fn highlight(&self, content: &str, path: &Path) -> HighlightedFile {
        let syntaxes = Self::syntax_set();
        let syntax = Self::syntax(path);
        let theme = Self::theme();
        let mut h = HighlightLines::new(syntax, &theme);

        let mut lines = Vec::new();
        for (idx, line) in LinesWithEndings::from(content).enumerate() {
            let ranges: Vec<(Style, &str)> = h.highlight_line(line, syntaxes).unwrap_or_default();
            let spans = ranges
                .into_iter()
                .map(|(style, text)| Self::span(style, text))
                .filter(|s| !s.text.is_empty())
                .collect();
            lines.push(FileLine {
                line_no: idx as u32,
                fold: vmux_ecs::event::FoldGutter::None,
                indent_levels: 0,
                spans,
            });
        }
        HighlightedFile {
            lines,
            encoding: vmux_ecs::event::FileEncoding::Utf8,
        }
    }

    pub(crate) fn load_file(&self, path: &Path) -> Result<HighlightedFile, LoadError> {
        let meta = std::fs::metadata(path)
            .map_err(|e| LoadError::Unreadable(format!("cannot open {}: {e}", path.display())))?;
        if !meta.is_file() {
            return Err(LoadError::Unreadable(format!(
                "not a file: {}",
                path.display()
            )));
        }
        if meta.len() > FILE_VIEW_MAX_BYTES {
            return Err(LoadError::Unreadable(format!(
                "file too large ({} bytes, max {})",
                meta.len(),
                FILE_VIEW_MAX_BYTES
            )));
        }
        let bytes = std::fs::read(path)
            .map_err(|e| LoadError::Unreadable(format!("cannot read {}: {e}", path.display())))?;
        let Some(decoded) = crate::encoding::DecodedText::decode(&bytes) else {
            return Err(LoadError::Binary);
        };
        let mut out = match meta.len() > HIGHLIGHT_MAX_BYTES {
            true => self.plain(&decoded.text),
            false => self.highlight(&decoded.text, path),
        };
        out.encoding = decoded.encoding;
        Ok(out)
    }

    fn plain(&self, content: &str) -> HighlightedFile {
        let theme = Self::theme();
        let fg = Self::foreground(&theme);
        let lines = LinesWithEndings::from(content)
            .enumerate()
            .map(|(idx, line)| FileLine {
                line_no: idx as u32,
                fold: vmux_ecs::event::FoldGutter::None,
                indent_levels: 0,
                spans: vec![StyledSpan {
                    text: line.trim_end_matches(['\n', '\r']).to_string(),
                    fg,
                    bold: false,
                    italic: false,
                }],
            })
            .collect();
        HighlightedFile {
            lines,
            encoding: vmux_ecs::event::FileEncoding::Utf8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_keywords_distinctly() {
        let hl = Highlighter::new();
        let out = hl.highlight("fn main() {}\n", std::path::Path::new("a.rs"));
        assert_eq!(
            Highlighter::syntax(std::path::Path::new("a.rs")).name,
            "Rust"
        );
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
        assert_eq!(
            Highlighter::syntax(std::path::Path::new("Cargo.toml")).name,
            "TOML"
        );
        let colors: std::collections::HashSet<_> = out
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.fg))
            .collect();
        assert!(colors.len() > 1, "expected highlighting, got {colors:?}");
    }

    #[test]
    fn recognizes_languages_beyond_syntect_defaults() {
        for file in ["a.ts", "a.tsx", "a.go", "a.py", "a.kt", "a.swift", "a.zig"] {
            assert_ne!(
                Highlighter::syntax(std::path::Path::new(file)).name,
                "Plain Text",
                "{file} not recognized"
            );
        }
    }

    #[test]
    fn unknown_extension_is_plaintext_single_span() {
        let hl = Highlighter::new();
        let out = hl.highlight("just text\n", std::path::Path::new("notes.xyzzy"));
        assert_eq!(
            Highlighter::syntax(std::path::Path::new("notes.xyzzy")).name,
            "Plain Text"
        );
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
        assert!(err.to_string().contains("/no/such/file.rs"), "got: {err}");
    }

    #[test]
    fn load_rejects_directory() {
        let hl = Highlighter::new();
        let dir = std::env::temp_dir();
        let err = hl.load_file(&dir).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("not a file"),
            "got: {err}"
        );
    }

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
        assert_eq!(
            Highlighter::syntax(&p).name,
            "Rust",
            "the language is still recognised"
        );
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
        assert_eq!(Highlighter::syntax(&p).name, "Rust");
        assert_eq!(out.lines.len(), 1);
    }
}
