use ropey::Rope;
use syntect::highlighting::{
    HighlightIterator, HighlightState, Highlighter as SyntectHighlighter, Theme,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference};
use vmux_ecs::event::{FileLine, StyledSpan};

use crate::host::highlight::Highlighter;

#[derive(bevy::prelude::Component)]
pub struct HighlightCache {
    syntax: &'static SyntaxReference,
    theme: Theme,
    dark: bool,
    befores: Vec<(ParseState, HighlightState)>,
    spans: Vec<Vec<StyledSpan>>,
    semantic: crate::lsp::semantic::SemanticHighlight,
    plain: bool,
    pub language: String,
}

impl HighlightCache {
    pub fn new(path: &std::path::Path) -> Self {
        Self::new_with_scheme(path, true)
    }

    pub fn new_with_scheme(path: &std::path::Path, dark: bool) -> Self {
        let syntax = Highlighter::syntax(path);
        Self {
            language: syntax.name.clone(),
            syntax,
            theme: Highlighter::theme(dark),
            dark,
            befores: Vec::new(),
            spans: Vec::new(),
            semantic: Default::default(),
            plain: false,
        }
    }

    pub fn plain(path: &std::path::Path) -> Self {
        Self::plain_with_scheme(path, true)
    }

    pub fn plain_with_scheme(path: &std::path::Path, dark: bool) -> Self {
        Self {
            plain: true,
            ..Self::new_with_scheme(path, dark)
        }
    }

    pub fn is_plain(&self) -> bool {
        self.plain
    }

    pub fn is_dark(&self) -> bool {
        self.dark
    }

    pub fn set_dark(&mut self, dark: bool) -> bool {
        if self.dark == dark {
            return false;
        }
        self.dark = dark;
        self.theme = Highlighter::theme(dark);
        self.befores.clear();
        self.spans.clear();
        true
    }

    pub fn set_semantic(&mut self, semantic: crate::lsp::semantic::SemanticHighlight) {
        self.semantic = semantic;
    }

    fn initial(&self) -> (ParseState, HighlightState) {
        let hl = SyntectHighlighter::new(&self.theme);
        (
            ParseState::new(self.syntax),
            HighlightState::new(&hl, ScopeStack::new()),
        )
    }

    pub fn invalidate_from(&mut self, line: usize) {
        self.befores.truncate(line + 1);
        self.spans.truncate(line);
    }

    fn ensure_rendered(&mut self, rope: &Rope, line: usize) {
        if self.befores.is_empty() {
            self.befores.push(self.initial());
        }
        let total = rope.len_lines();
        if self.befores.len() > line || self.befores.len() > total {
            return;
        }
        let ss = Highlighter::syntax_set();
        let hl = SyntectHighlighter::new(&self.theme);
        while self.befores.len() <= line && self.befores.len() - 1 < total {
            let i = self.befores.len() - 1;
            let (mut ps, mut hs) = self.befores[i].clone();
            let text: String = rope.line(i).chars().collect();
            let ops = ps.parse_line(&text, ss).unwrap_or_default();
            let mut spans = Vec::new();
            for (style, text) in HighlightIterator::new(&mut hs, &ops, &text, &hl) {
                let span = Highlighter::span(style, text);
                if !span.text.is_empty() {
                    spans.push(span);
                }
            }
            if self.spans.len() <= i {
                self.spans.resize(i + 1, Vec::new());
            }
            self.spans[i] = spans;
            self.befores.push((ps, hs));
        }
    }

    pub fn line_window(&mut self, rope: &Rope, start: usize, end: usize) -> Vec<FileLine> {
        let total = rope.len_lines();
        let end = end.min(total);
        if start >= end {
            return Vec::new();
        }
        if self.plain {
            return self.plain_window(rope, start, end);
        }
        self.ensure_rendered(rope, end);
        let mut out = Vec::with_capacity(end - start);
        for i in start..end {
            let spans = self.spans.get(i).cloned().unwrap_or_default();
            let spans = self.semantic.apply(i as u32, spans, self.dark);
            out.push(FileLine {
                line_no: i as u32,
                fold: vmux_ecs::event::FoldGutter::None,
                indent_levels: 0,
                spans,
            });
        }
        out
    }

    fn plain_window(&self, rope: &Rope, start: usize, end: usize) -> Vec<FileLine> {
        let fg = Highlighter::foreground(&self.theme, self.dark);
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
                fold: vmux_ecs::event::FoldGutter::None,
                indent_levels: 0,
                spans: self.semantic.apply(i as u32, spans, self.dark),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_line_numbers_and_text() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let r = Rope::from_str("fn a() {}\nlet x = 1;\nstruct S;\n");
        let w = c.line_window(&r, 1, 3);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].line_no, 1);
        let joined: String = w[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined.trim_end(), "let x = 1;");
    }

    #[test]
    fn invalidate_recomputes_tail() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let mut r = Rope::from_str("let a = 1;\nlet b = 2;\nlet c = 3;\n");
        let _ = c.line_window(&r, 0, 3);
        r.insert(0, "// ");
        c.invalidate_from(0);
        let w = c.line_window(&r, 2, 3);
        let joined: String = w[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined.trim_end(), "let c = 3;");
    }

    #[test]
    fn invalidating_a_later_line_keeps_the_lines_above_it() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let mut r = Rope::from_str("let a = 1;\nlet b = 2;\nlet c = 3;\n");
        let _ = c.line_window(&r, 0, 3);
        let first = c.spans[0].clone();
        let start = r.line_to_char(1);
        r.insert(start, "// ");
        c.invalidate_from(1);
        assert_eq!(
            c.spans.len(),
            1,
            "only the edited line and below are dropped"
        );
        let w = c.line_window(&r, 1, 2);
        let joined: String = w[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined.trim_end(), "// let b = 2;");
        assert_eq!(c.spans[0], first, "the line above was not re-rendered");
    }

    #[test]
    fn a_window_below_what_was_rendered_still_resolves() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let r = Rope::from_str(&"let x = 1;\n".repeat(200));
        let _ = c.line_window(&r, 0, 5);
        let w = c.line_window(&r, 150, 155);
        assert_eq!(w.len(), 5);
        assert_eq!(w[0].line_no, 150);
        let joined: String = w[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined.trim_end(), "let x = 1;");
    }

    #[test]
    fn a_plain_cache_still_serves_the_text() {
        let mut c = HighlightCache::plain(std::path::Path::new("a.rs"));
        let r = Rope::from_str("fn main() {}\nlet x = 1;\n");
        let w = c.line_window(&r, 0, 2);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].spans[0].text, "fn main() {}");
        assert_eq!(w[1].spans[0].text, "let x = 1;");
    }

    #[test]
    fn a_plain_cache_keeps_no_parser_state() {
        let mut c = HighlightCache::plain(std::path::Path::new("a.rs"));
        let r = Rope::from_str(&"let x = 1;\n".repeat(500));
        let _ = c.line_window(&r, 0, 500);
        assert!(c.befores.is_empty());
    }

    #[test]
    fn a_plain_cache_drops_the_line_ending_whichever_it_is() {
        let mut c = HighlightCache::plain(std::path::Path::new("a.rs"));
        let r = Rope::from_str("fn main() {}\r\nlet x = 1;\r\n");
        let w = c.line_window(&r, 0, 2);
        assert_eq!(w[0].spans[0].text, "fn main() {}");
        assert_eq!(w[1].spans[0].text, "let x = 1;");
    }

    #[test]
    fn a_plain_cache_uses_one_colour() {
        let mut c = HighlightCache::plain(std::path::Path::new("a.rs"));
        let r = Rope::from_str("fn main() {}\n");
        let w = c.line_window(&r, 0, 1);
        assert_eq!(w[0].spans.len(), 1, "no colour means no splitting");
    }

    #[test]
    fn multicolor_for_code() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let r = Rope::from_str("fn main() {}\n");
        let w = c.line_window(&r, 0, 1);
        let colors: std::collections::HashSet<_> = w[0].spans.iter().map(|s| s.fg).collect();
        assert!(colors.len() > 1);
    }

    #[test]
    fn switching_color_scheme_recolors_cached_lines_without_shared_globals() {
        let mut dark = HighlightCache::new_with_scheme(std::path::Path::new("a.rs"), true);
        let mut light = HighlightCache::new_with_scheme(std::path::Path::new("a.rs"), false);
        let r = Rope::from_str("fn main() {}\n");
        let dark_before = dark.line_window(&r, 0, 1);
        let light_before = light.line_window(&r, 0, 1);
        assert_ne!(dark_before[0].spans, light_before[0].spans);
        assert!(dark.set_dark(false));
        let dark_after = dark.line_window(&r, 0, 1);
        assert_eq!(dark_after[0].spans, light_before[0].spans);
    }
}
