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
        }
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
            out.push(FileLine {
                line_no: i as u32,
                spans,
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

    #[test]
    fn multicolor_for_code() {
        let mut c = HighlightCache::new(std::path::Path::new("a.rs"));
        let r = rope("fn main() {}\n");
        let w = c.line_window(&r, 0, 1);
        let colors: std::collections::HashSet<_> = w[0].spans.iter().map(|s| s.fg).collect();
        assert!(colors.len() > 1);
    }
}
