use ropey::Rope;
use syntect::highlighting::{HighlightIterator, HighlightState, Highlighter, Theme};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference};
use vmux_core::event::{FileLine, StyledSpan};

use crate::highlight::{default_theme, is_dark_theme, select_syntax, styled_span, syntax_set};

pub struct HighlightCache {
    syntax: &'static SyntaxReference,
    theme: Theme,
    dark: bool,
    befores: Vec<(ParseState, HighlightState)>,
    /// What the language server said each identifier actually is, laid over syntect's guess.
    /// Absent until the server answers, and stale for an edit or two after one — which shows as
    /// colour lagging a keystroke, not as wrong text.
    semantic: crate::lsp::semantic::SemanticHighlight,
    /// Serve the text without colouring it, for a file too large to keep a parser state per line.
    plain: bool,
    pub language: String,
}

impl HighlightCache {
    pub fn new(path: &std::path::Path) -> Self {
        let syntax = select_syntax(path);
        Self {
            language: syntax.name.clone(),
            syntax,
            theme: default_theme(),
            dark: is_dark_theme(),
            befores: Vec::new(),
            semantic: Default::default(),
            plain: false,
        }
    }

    /// A cache for a file too large to colour, which serves the text and nothing else.
    ///
    /// The saving is not the parsing but [`Self::befores`]: one syntect parser state per line is
    /// what makes a large file unaffordable, and this never fills it.
    pub fn plain(path: &std::path::Path) -> Self {
        Self {
            plain: true,
            ..Self::new(path)
        }
    }

    pub fn is_plain(&self) -> bool {
        self.plain
    }

    pub fn set_semantic(&mut self, semantic: crate::lsp::semantic::SemanticHighlight) {
        self.semantic = semantic;
    }

    fn refresh_theme(&mut self) {
        if is_dark_theme() != self.dark {
            self.theme = default_theme();
            self.dark = is_dark_theme();
            self.befores.clear();
        }
    }

    fn initial(&self) -> (ParseState, HighlightState) {
        let hl = Highlighter::new(&self.theme);
        (
            ParseState::new(self.syntax),
            HighlightState::new(&hl, ScopeStack::new()),
        )
    }

    pub fn invalidate_from(&mut self, line: usize) {
        self.befores.truncate(line + 1);
    }

    fn ensure_before(&mut self, rope: &Rope, line: usize) {
        if self.befores.is_empty() {
            self.befores.push(self.initial());
        }
        let ss = syntax_set();
        let hl = Highlighter::new(&self.theme);
        let total = rope.len_lines();
        while self.befores.len() <= line && self.befores.len() - 1 < total {
            let i = self.befores.len() - 1;
            let (mut ps, mut hs) = self.befores[i].clone();
            let text: String = rope.line(i).chars().collect();
            let ops = ps.parse_line(&text, ss).unwrap_or_default();
            {
                let mut it = HighlightIterator::new(&mut hs, &ops, &text, &hl);
                for _ in it.by_ref() {}
            }
            self.befores.push((ps, hs));
        }
    }

    pub fn line_window(&mut self, rope: &Rope, start: usize, end: usize) -> Vec<FileLine> {
        self.refresh_theme();
        let total = rope.len_lines();
        let end = end.min(total);
        if start >= end {
            return Vec::new();
        }
        if self.plain {
            return self.plain_window(rope, start, end);
        }
        self.ensure_before(rope, end - 1);
        let ss = syntax_set();
        let hl = Highlighter::new(&self.theme);
        let mut out = Vec::with_capacity(end - start);
        for i in start..end {
            let (mut ps, mut hs) = self.befores[i].clone();
            let text: String = rope.line(i).chars().collect();
            let ops = ps.parse_line(&text, ss).unwrap_or_default();
            let spans: Vec<StyledSpan> = HighlightIterator::new(&mut hs, &ops, &text, &hl)
                .map(|(style, t)| styled_span(style, t))
                .filter(|s| !s.text.is_empty())
                .collect();
            let spans = self.semantic.apply(i as u32, spans, self.dark);
            out.push(FileLine {
                line_no: i as u32,
                fold: vmux_core::event::FoldGutter::None,
                spans,
            });
        }
        out
    }

    /// One span per line in the theme's foreground colour, keeping no parser state.
    ///
    /// The saving is `befores`, not the parsing: a syntect state per line is what makes a large
    /// file unaffordable, and this never fills it. Semantic tokens still apply — the server
    /// answers for a large file just as readily, and colour from it costs nothing per line.
    fn plain_window(&self, rope: &Rope, start: usize, end: usize) -> Vec<FileLine> {
        let fg = crate::highlight::theme_foreground(&self.theme);
        let mut out = Vec::with_capacity(end - start);
        for i in start..end {
            let text: String = rope
                .line(i)
                .chars()
                .filter(|c| !matches!(c, '\n' | '\r'))
                .collect();
            let spans = match text.is_empty() {
                true => Vec::new(),
                false => vec![StyledSpan {
                    text,
                    fg,
                    bold: false,
                    italic: false,
                }],
            };
            out.push(FileLine {
                line_no: i as u32,
                fold: vmux_core::event::FoldGutter::None,
                spans: self.semantic.apply(i as u32, spans, self.dark),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(s: &str) -> Rope {
        Rope::from_str(s)
    }

    #[test]
    fn window_line_numbers_and_text() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let r = rope("fn a() {}\nlet x = 1;\nstruct S;\n");
        let w = c.line_window(&r, 1, 3);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].line_no, 1);
        let joined: String = w[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined.trim_end(), "let x = 1;");
    }

    #[test]
    fn invalidate_recomputes_tail() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let mut r = rope("let a = 1;\nlet b = 2;\nlet c = 3;\n");
        let _ = c.line_window(&r, 0, 3);
        r.insert(0, "// ");
        c.invalidate_from(0);
        let w = c.line_window(&r, 2, 3);
        let joined: String = w[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined.trim_end(), "let c = 3;");
    }

    /// A file too large to colour still has to serve its text, and each line has to stay one
    /// line: returning nothing, or folding the file into one span, is what "too large" used to
    /// mean here.
    #[test]
    fn a_plain_cache_still_serves_the_text() {
        let mut c = HighlightCache::plain(std::path::Path::new("a.rs"));
        let r = rope("fn main() {}\nlet x = 1;\n");
        let w = c.line_window(&r, 0, 2);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].spans[0].text, "fn main() {}");
        assert_eq!(w[1].spans[0].text, "let x = 1;");
    }

    /// The saving is the per-line parser state, so a plain window must not fill it however many
    /// lines it serves.
    #[test]
    fn a_plain_cache_keeps_no_parser_state() {
        let mut c = HighlightCache::plain(std::path::Path::new("a.rs"));
        let r = rope(&"let x = 1;\n".repeat(500));
        let _ = c.line_window(&r, 0, 500);
        assert!(c.befores.is_empty());
    }

    /// A carriage return left in the span is a control character the renderer draws, and a CRLF
    /// file is exactly the kind large enough to reach this path.
    #[test]
    fn a_plain_cache_drops_the_line_ending_whichever_it_is() {
        let mut c = HighlightCache::plain(std::path::Path::new("a.rs"));
        let r = rope("fn main() {}\r\nlet x = 1;\r\n");
        let w = c.line_window(&r, 0, 2);
        assert_eq!(w[0].spans[0].text, "fn main() {}");
        assert_eq!(w[1].spans[0].text, "let x = 1;");
    }

    #[test]
    fn a_plain_cache_uses_one_colour() {
        let mut c = HighlightCache::plain(std::path::Path::new("a.rs"));
        let r = rope("fn main() {}\n");
        let w = c.line_window(&r, 0, 1);
        assert_eq!(w[0].spans.len(), 1, "no colour means no splitting");
    }

    #[test]
    fn multicolor_for_code() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let r = rope("fn main() {}\n");
        let w = c.line_window(&r, 0, 1);
        let colors: std::collections::HashSet<_> = w[0].spans.iter().map(|s| s.fg).collect();
        assert!(colors.len() > 1);
    }
}
