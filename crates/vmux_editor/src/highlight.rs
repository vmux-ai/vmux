use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use vmux_core::event::{FileLine, StyledSpan};

pub const FILE_VIEW_MAX_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug)]
pub struct HighlightedFile {
    pub language: String,
    pub lines: Vec<FileLine>,
}

pub struct Highlighter {
    syntaxes: SyntaxSet,
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
            syntaxes: SyntaxSet::load_defaults_newlines(),
            themes: ThemeSet::load_defaults(),
        }
    }

    pub fn highlight(&self, content: &str, path: &Path) -> HighlightedFile {
        let syntax = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|ext| self.syntaxes.find_syntax_by_extension(ext))
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let theme = &self.themes.themes["base16-ocean.dark"];
        let mut h = HighlightLines::new(syntax, theme);

        let mut lines = Vec::new();
        for (idx, line) in LinesWithEndings::from(content).enumerate() {
            let ranges: Vec<(Style, &str)> =
                h.highlight_line(line, &self.syntaxes).unwrap_or_default();
            let spans = ranges
                .into_iter()
                .map(|(style, text)| to_styled_span(style, text))
                .filter(|s| !s.text.is_empty())
                .collect();
            lines.push(FileLine {
                line_no: idx as u32,
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
        Ok(self.highlight(&content, path))
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
}
