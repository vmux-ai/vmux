#[cfg(test)]
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, Theme};
use syntect::parsing::{SyntaxReference, SyntaxSet};
#[cfg(test)]
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

#[cfg(test)]
thread_local! {
    static HIGHLIGHTED_LINES: Cell<usize> = const { Cell::new(0) };
}

impl Highlighter {
    #[cfg(test)]
    pub(super) fn take_line_count() -> usize {
        HIGHLIGHTED_LINES.with(|count| count.replace(0))
    }

    #[cfg(test)]
    pub(crate) fn file(content: &str, path: &Path) -> Vec<Vec<StyledSpan>> {
        let assets = Assets::shared();
        let syntax = assets.syntax(path);
        let mut light = HighlightLines::new(syntax, &assets.light);
        let mut dark = HighlightLines::new(assets.syntax(path), &assets.dark);
        let mut lines = Vec::new();
        for (index, line) in LinesWithEndings::from(content).enumerate() {
            if index % 256 == 0 && super::runtime::GitRuntime::canceled() {
                break;
            }
            let light = light
                .highlight_line(line, &assets.syntaxes)
                .unwrap_or_default();
            let dark = dark
                .highlight_line(line, &assets.syntaxes)
                .unwrap_or_default();
            lines.push(Self::spans(light, dark));
        }
        lines
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
        #[cfg(test)]
        HIGHLIGHTED_LINES.with(|count| count.set(count.get() + 1));
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

pub(crate) struct FileHighlight {
    content: String,
    path: PathBuf,
    offset: usize,
    lines: Vec<Vec<StyledSpan>>,
    state: Option<FileHighlightState>,
}

struct FileHighlightState {
    light: HighlightLines<'static>,
    dark: HighlightLines<'static>,
}

impl FileHighlight {
    pub(crate) fn new(content: String, path: &Path) -> Self {
        Self {
            content,
            path: path.to_path_buf(),
            offset: 0,
            lines: Vec::new(),
            state: None,
        }
    }

    pub(crate) fn line(
        &mut self,
        number: u32,
    ) -> Result<Option<Vec<StyledSpan>>, super::repository::GitError> {
        if number == 0 {
            return Ok(None);
        }
        let assets = Assets::shared();
        if self.state.is_none() {
            self.state = Some(FileHighlightState {
                light: HighlightLines::new(assets.syntax(&self.path), &assets.light),
                dark: HighlightLines::new(assets.syntax(&self.path), &assets.dark),
            });
        }
        let state = self
            .state
            .as_mut()
            .expect("the file syntax state is initialized");
        while self.lines.len() < number as usize && self.offset < self.content.len() {
            if super::runtime::GitRuntime::canceled() {
                return Err(super::repository::GitError(
                    "git command canceled".to_string(),
                ));
            }
            let remainder = &self.content[self.offset..];
            let length = remainder
                .find('\n')
                .map_or(remainder.len(), |index| index + 1);
            let line = &remainder[..length];
            let light = state
                .light
                .highlight_line(line, &assets.syntaxes)
                .unwrap_or_default();
            let dark = state
                .dark
                .highlight_line(line, &assets.syntaxes)
                .unwrap_or_default();
            self.lines.push(Highlighter::spans(light, dark));
            self.offset += length;
        }
        Ok(self.lines.get(number as usize - 1).cloned())
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

    #[test]
    fn incremental_file_highlighting_preserves_multiline_syntax_and_cached_prefixes() {
        let content = "fn main() {\n/*\nlet value = 42;\n*/\nlet answer = 7;\n}\n";
        let expected = Highlighter::file(content, Path::new("main.rs"));
        let mut highlight = FileHighlight::new(content.to_string(), Path::new("main.rs"));
        Highlighter::take_line_count();

        assert_eq!(highlight.line(3).unwrap(), Some(expected[2].clone()));
        assert_eq!(Highlighter::take_line_count(), 3);
        assert_eq!(highlight.line(5).unwrap(), Some(expected[4].clone()));
        assert_eq!(Highlighter::take_line_count(), 2);
        assert_eq!(highlight.line(1).unwrap(), Some(expected[0].clone()));
        assert_eq!(Highlighter::take_line_count(), 0);
    }

    #[test]
    fn canceled_file_highlighting_stops_before_the_next_requested_line() {
        let runtime = super::super::runtime::GitRuntime::default();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = std::sync::mpsc::sync_channel(1);
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
        let task = runtime
            .spawn(None, move || {
                let mut highlight =
                    FileHighlight::new("line\nline\n".to_string(), Path::new("file.txt"));
                highlight.line(1).unwrap();
                ready_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
                done_tx.send(highlight.line(2)).unwrap();
            })
            .unwrap();
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();

        drop(task);
        resume_tx.send(()).unwrap();

        let result = done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(result.unwrap_err().0, "git command canceled");
    }
}
