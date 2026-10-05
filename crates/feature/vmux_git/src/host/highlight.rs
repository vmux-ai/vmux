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

impl Assets {
    fn shared() -> &'static Self {
        static ASSETS: OnceLock<Assets> = OnceLock::new();
        ASSETS.get_or_init(|| Self {
            syntaxes: two_face::syntax::extra_newlines(),
            themes: ThemeSet::load_defaults(),
        })
    }

    fn syntax(&self, path: &Path) -> &SyntaxReference {
        path.extension()
            .and_then(|extension| extension.to_str())
            .and_then(|extension| self.syntaxes.find_syntax_by_extension(extension))
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text())
    }
}

pub(crate) struct Highlighter;

impl Highlighter {
    pub(crate) fn file(content: &str, path: &Path) -> Vec<Vec<StyledSpan>> {
        let assets = Assets::shared();
        let mut highlighter = HighlightLines::new(
            assets.syntax(path),
            &assets.themes.themes["base16-ocean.dark"],
        );
        LinesWithEndings::from(content)
            .map(|line| {
                highlighter
                    .highlight_line(line, &assets.syntaxes)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(style, text)| Self::span(style, text))
                    .filter(|span| !span.text.is_empty())
                    .collect()
            })
            .collect()
    }

    pub(crate) fn line(text: &str, path: &Path) -> Vec<StyledSpan> {
        let assets = Assets::shared();
        let mut highlighter = HighlightLines::new(
            assets.syntax(path),
            &assets.themes.themes["base16-ocean.dark"],
        );
        highlighter
            .highlight_line(text, &assets.syntaxes)
            .unwrap_or_default()
            .into_iter()
            .map(|(style, text)| Self::span(style, text))
            .filter(|span| !span.text.is_empty())
            .collect()
    }

    fn span(style: Style, text: &str) -> StyledSpan {
        StyledSpan {
            text: text.trim_end_matches(['\n', '\r']).to_string(),
            fg: [style.foreground.r, style.foreground.g, style.foreground.b],
            bold: style.font_style.contains(FontStyle::BOLD),
            italic: style.font_style.contains(FontStyle::ITALIC),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_per_line_with_colors() {
        let lines = Highlighter::file("fn main() {}\n", Path::new("a.rs"));
        assert_eq!(lines.len(), 1);
        let colors: std::collections::HashSet<_> = lines[0].iter().map(|s| s.fg).collect();
        assert!(colors.len() > 1, "expected multiple colors");
    }

    #[test]
    fn single_line_independent() {
        let spans = Highlighter::line("let x = 1;", Path::new("a.rs"));
        let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "let x = 1;");
    }
}
