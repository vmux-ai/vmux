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
                fold: vmux_core::event::FoldGutter::None,
                spans,
            });
        }
        out
    }
}

#[cfg(test)]
#[path = "highlight_cache.test.rs"]
mod tests;
