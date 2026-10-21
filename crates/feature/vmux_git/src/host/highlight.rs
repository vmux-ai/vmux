use std::path::Path;
use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, Theme};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
use vmux_syntax::Palette;

use crate::event::StyledSpan;

struct Assets {
    syntaxes: SyntaxSet,
    light: Theme,
    dark: Theme,
}

impl Assets {
    fn shared() -> &'static Self {
        static ASSETS: OnceLock<Assets> = OnceLock::new();
        ASSETS.get_or_init(|| Self {
            syntaxes: two_face::syntax::extra_newlines(),
            light: Palette::for_scheme(false).theme(),
            dark: Palette::for_scheme(true).theme(),
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
        let syntax = assets.syntax(path);
        let mut light = HighlightLines::new(syntax, &assets.light);
        let mut dark = HighlightLines::new(assets.syntax(path), &assets.dark);
        LinesWithEndings::from(content)
            .map(|line| {
                let light = light
                    .highlight_line(line, &assets.syntaxes)
                    .unwrap_or_default();
                let dark = dark
                    .highlight_line(line, &assets.syntaxes)
                    .unwrap_or_default();
                Self::spans(light, dark)
            })
            .collect()
    }

    pub(crate) fn line(text: &str, path: &Path) -> Vec<StyledSpan> {
        let assets = Assets::shared();
        let syntax = assets.syntax(path);
        let mut light = HighlightLines::new(syntax, &assets.light);
        let mut dark = HighlightLines::new(syntax, &assets.dark);
        let light = light
            .highlight_line(text, &assets.syntaxes)
            .unwrap_or_default();
        let dark = dark
            .highlight_line(text, &assets.syntaxes)
            .unwrap_or_default();
        Self::spans(light, dark)
    }

    fn spans(light: Vec<(Style, &str)>, dark: Vec<(Style, &str)>) -> Vec<StyledSpan> {
        light
            .into_iter()
            .zip(dark)
            .map(|((light, light_text), (dark, dark_text))| {
                let text = if light_text == dark_text {
                    light_text
                } else {
                    dark_text
                };
                Self::span(light, dark, text)
            })
            .filter(|span| !span.text.is_empty())
            .collect()
    }

    fn span(light: Style, dark: Style, text: &str) -> StyledSpan {
        StyledSpan {
            text: text.trim_end_matches(['\n', '\r']).to_string(),
            light_fg: [light.foreground.r, light.foreground.g, light.foreground.b],
            dark_fg: [dark.foreground.r, dark.foreground.g, dark.foreground.b],
            bold: light.font_style.contains(FontStyle::BOLD)
                || dark.font_style.contains(FontStyle::BOLD),
            italic: light.font_style.contains(FontStyle::ITALIC)
                || dark.font_style.contains(FontStyle::ITALIC),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_with_github_colours() {
        let lines = Highlighter::file("fn main() {}\n", Path::new("a.rs"));
        assert_eq!(lines.len(), 1);
        let colors: std::collections::HashSet<_> =
            lines[0].iter().map(|span| span.dark_fg).collect();
        assert!(colors.len() > 1, "expected multiple colors");
        assert!(
            lines[0]
                .iter()
                .any(|span| span.light_fg == [207, 34, 46] && span.dark_fg == [255, 123, 114])
        );
    }

    #[test]
    fn single_line_independent() {
        let spans = Highlighter::line("let x = 1;", Path::new("a.rs"));
        let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "let x = 1;");
    }
}
