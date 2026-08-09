use std::path::Path;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use vmux_core::event::{FileLine, StyledSpan};

pub const FILE_VIEW_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// Broad language coverage (~200 syntaxes from the bat project) instead of
/// syntect's small default set.
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

/// Select the dark or light syntax-highlight theme. Returns true if it changed.
pub fn set_dark_theme(dark: bool) -> bool {
    DARK_THEME.swap(dark, Ordering::Relaxed) != dark
}

/// Whether syntax highlighting currently uses the dark theme.
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
#[path = "highlight.test.rs"]
mod tests;
