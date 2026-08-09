use std::path::Path;
use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::event::StyledSpan;

struct Assets {
    syntaxes: SyntaxSet,
    themes: ThemeSet,
}

fn assets() -> &'static Assets {
    static A: OnceLock<Assets> = OnceLock::new();
    A.get_or_init(|| Assets {
        syntaxes: two_face::syntax::extra_newlines(),
        themes: ThemeSet::load_defaults(),
    })
}

fn syntax_for<'a>(ss: &'a SyntaxSet, path: &Path) -> &'a SyntaxReference {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .unwrap_or_else(|| ss.find_syntax_plain_text())
}

fn to_span(style: Style, text: &str) -> StyledSpan {
    StyledSpan {
        text: text.trim_end_matches(['\n', '\r']).to_string(),
        fg: [style.foreground.r, style.foreground.g, style.foreground.b],
        bold: style.font_style.contains(FontStyle::BOLD),
        italic: style.font_style.contains(FontStyle::ITALIC),
    }
}

pub fn highlight_file(content: &str, path: &Path) -> Vec<Vec<StyledSpan>> {
    let a = assets();
    let syntax = syntax_for(&a.syntaxes, path);
    let theme = &a.themes.themes["base16-ocean.dark"];
    let mut h = HighlightLines::new(syntax, theme);
    LinesWithEndings::from(content)
        .map(|line| {
            h.highlight_line(line, &a.syntaxes)
                .unwrap_or_default()
                .into_iter()
                .map(|(style, text)| to_span(style, text))
                .filter(|s| !s.text.is_empty())
                .collect()
        })
        .collect()
}

pub fn highlight_line(text: &str, path: &Path) -> Vec<StyledSpan> {
    let a = assets();
    let syntax = syntax_for(&a.syntaxes, path);
    let theme = &a.themes.themes["base16-ocean.dark"];
    let mut h = HighlightLines::new(syntax, theme);
    h.highlight_line(text, &a.syntaxes)
        .unwrap_or_default()
        .into_iter()
        .map(|(style, t)| to_span(style, t))
        .filter(|s| !s.text.is_empty())
        .collect()
}

#[cfg(test)]
#[path = "highlight.test.rs"]
mod tests;
