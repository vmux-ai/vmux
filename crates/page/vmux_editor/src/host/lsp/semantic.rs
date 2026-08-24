//! Colour from the server's understanding of the code, laid over syntect's guess.
//!
//! A regex grammar can tell a keyword from a string and little else: every identifier comes back
//! the same colour, so a type, a function and a local all read alike. The server already knows
//! which is which, and `textDocument/semanticTokens` is how it says so.

use std::collections::HashMap;
use std::sync::OnceLock;

use syntect::highlighting::Highlighter;
use syntect::parsing::ScopeStack;
use vmux_core::event::StyledSpan;

/// What this client tells a server it can colour. A server may only use types from this list, so
/// leaving one out is how a token silently never arrives.
pub const SEMANTIC_TOKEN_TYPES: &[&str] = &[
    "namespace",
    "type",
    "class",
    "enum",
    "interface",
    "struct",
    "typeParameter",
    "parameter",
    "variable",
    "property",
    "enumMember",
    "function",
    "method",
    "macro",
    "keyword",
    "comment",
    "string",
    "number",
    "operator",
    "lifetime",
];

/// The token types a server declared at `initialize`, in the order it will refer to them by.
///
/// The protocol sends indices into this list rather than names, so without it a response is
/// undecodable — which is why a server that omits a legend gets no semantic colour at all.
pub struct SemanticLegend {
    kinds: Vec<Option<SemanticKind>>,
}

impl SemanticLegend {
    pub fn of(capabilities: &lsp_types::ServerCapabilities) -> Option<Self> {
        let provider = capabilities.semantic_tokens_provider.as_ref()?;
        let legend = match provider {
            lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(o) => &o.legend,
            lsp_types::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(o) => {
                &o.semantic_tokens_options.legend
            }
        };
        let kinds = legend
            .token_types
            .iter()
            .map(|t| SemanticKind::of(t.as_str()))
            .collect();
        Some(Self { kinds })
    }

    /// Expand the flat, delta-encoded array the protocol sends into one token per entry.
    ///
    /// Five integers each: line delta, start delta, length, type index, modifier bits. Both
    /// deltas are relative to the previous token, and the start delta resets on a new line.
    pub fn decode(&self, data: &[u32]) -> Vec<SemanticToken> {
        let mut out = Vec::with_capacity(data.len() / 5);
        let mut line = 0u32;
        let mut start = 0u32;
        for entry in data.chunks_exact(5) {
            let (delta_line, delta_start, length, kind) = (entry[0], entry[1], entry[2], entry[3]);
            line += delta_line;
            start = if delta_line == 0 {
                start + delta_start
            } else {
                delta_start
            };
            if length == 0 {
                continue;
            }
            let Some(Some(kind)) = self.kinds.get(kind as usize).copied() else {
                continue;
            };
            out.push(SemanticToken {
                line,
                utf16_start: start,
                utf16_len: length,
                kind,
            });
        }
        out
    }
}

#[derive(Clone, Copy)]
pub struct SemanticToken {
    pub line: u32,
    pub utf16_start: u32,
    pub utf16_len: u32,
    pub kind: SemanticKind,
}

/// The token types worth a colour of their own.
///
/// Anything the server reports outside this set keeps whatever syntect gave it, which is the
/// right answer for comments, strings and numbers — a regex grammar gets those right already.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticKind {
    Type,
    Function,
    Macro,
    Namespace,
    EnumMember,
    Lifetime,
    Parameter,
}

impl SemanticKind {
    fn of(name: &str) -> Option<Self> {
        Some(match name {
            "type" | "struct" | "class" | "enum" | "interface" | "typeParameter" | "typeAlias"
            | "union" | "builtinType" => Self::Type,
            "function" | "method" => Self::Function,
            "macro" | "attributeBracket" | "derive" => Self::Macro,
            "namespace" | "module" => Self::Namespace,
            "enumMember" | "constant" => Self::EnumMember,
            "lifetime" | "label" => Self::Lifetime,
            "parameter" => Self::Parameter,
            _ => return None,
        })
    }

    const ALL: [Self; 7] = [
        Self::Type,
        Self::Function,
        Self::Macro,
        Self::Namespace,
        Self::EnumMember,
        Self::Lifetime,
        Self::Parameter,
    ];

    /// The TextMate scope this kind is the server's word for.
    ///
    /// No colours here. A kind names a scope, the theme answers for the scope, and so the two
    /// cannot drift: a server's `function` lands on exactly what syntect would have reached for
    /// unaided, and a token does not change shade as the server warms up and starts answering.
    fn scope(self) -> &'static str {
        match self {
            Self::Type => "entity.name.type",
            Self::Function => "entity.name.function",
            Self::Macro => "entity.name.function.macro",
            Self::Namespace => "entity.name.namespace",
            Self::EnumMember => "variable.other.enummember",
            Self::Lifetime => "storage.modifier.lifetime",
            Self::Parameter => "variable.parameter",
        }
    }

    fn colour(self, dark: bool) -> [u8; 3] {
        let slot = Self::ALL
            .iter()
            .position(|kind| *kind == self)
            .unwrap_or_default();
        Self::resolved(dark)[slot]
    }

    /// Asked of the theme once per scheme: building a highlighter is not free, and the answer
    /// cannot change while the scheme does not.
    fn resolved(dark: bool) -> &'static [[u8; 3]; 7] {
        static DARK: OnceLock<[[u8; 3]; 7]> = OnceLock::new();
        static LIGHT: OnceLock<[[u8; 3]; 7]> = OnceLock::new();
        let cell = if dark { &DARK } else { &LIGHT };
        cell.get_or_init(|| {
            let theme = crate::palette::Palette::of(dark).theme();
            let highlighter = Highlighter::new(&theme);
            let mut out = [[0u8; 3]; 7];
            for (slot, kind) in Self::ALL.iter().enumerate() {
                let mut stack = ScopeStack::new();
                if let Ok(scope) = kind.scope().parse() {
                    stack.push(scope);
                }
                let style = highlighter.style_for_stack(stack.as_slice());
                out[slot] = [style.foreground.r, style.foreground.g, style.foreground.b];
            }
            out
        })
    }
}

/// One file's tokens, ready to lay over a line at a time.
#[derive(Default)]
pub struct SemanticHighlight {
    by_line: HashMap<u32, Vec<(u32, u32, SemanticKind)>>,
}

impl SemanticHighlight {
    pub fn of(tokens: Vec<SemanticToken>) -> Self {
        let mut by_line: HashMap<u32, Vec<(u32, u32, SemanticKind)>> = HashMap::new();
        for token in tokens {
            by_line.entry(token.line).or_default().push((
                token.utf16_start,
                token.utf16_len,
                token.kind,
            ));
        }
        Self { by_line }
    }

    pub fn is_empty(&self) -> bool {
        self.by_line.is_empty()
    }

    /// Recolour the parts of `spans` a token covers, leaving the rest as syntect had it.
    ///
    /// Spans are split at token boundaries rather than replaced wholesale, so a token that
    /// covers half of one syntect span colours only that half.
    pub fn apply(&self, line: u32, spans: Vec<StyledSpan>, dark: bool) -> Vec<StyledSpan> {
        let Some(tokens) = self.by_line.get(&line) else {
            return spans;
        };
        let chars: Vec<char> = spans.iter().flat_map(|s| s.text.chars()).collect();
        if chars.is_empty() {
            return spans;
        }
        let mut overrides: Vec<Option<[u8; 3]>> = vec![None; chars.len()];
        for (utf16_start, utf16_len, kind) in tokens {
            let start = Self::char_index(&chars, *utf16_start);
            let end = Self::char_index(&chars, utf16_start + utf16_len);
            let colour = kind.colour(dark);
            for slot in overrides.iter_mut().take(end.min(chars.len())).skip(start) {
                *slot = Some(colour);
            }
        }

        let mut out: Vec<StyledSpan> = Vec::with_capacity(spans.len());
        let mut at = 0usize;
        for span in spans {
            let mut run = String::new();
            let mut run_colour = None;
            let mut started = false;
            for ch in span.text.chars() {
                let colour = overrides[at.min(overrides.len() - 1)];
                if started && colour != run_colour {
                    out.push(StyledSpan {
                        text: std::mem::take(&mut run),
                        fg: run_colour.unwrap_or(span.fg),
                        bold: span.bold,
                        italic: span.italic,
                    });
                }
                run_colour = colour;
                started = true;
                run.push(ch);
                at += 1;
            }
            if !run.is_empty() {
                out.push(StyledSpan {
                    text: run,
                    fg: run_colour.unwrap_or(span.fg),
                    bold: span.bold,
                    italic: span.italic,
                });
            }
        }
        out
    }

    /// LSP counts columns in UTF-16 code units; a span's text is chars.
    fn char_index(chars: &[char], utf16: u32) -> usize {
        let mut seen = 0u32;
        for (index, ch) in chars.iter().enumerate() {
            if seen >= utf16 {
                return index;
            }
            seen += ch.len_utf16() as u32;
        }
        chars.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legend(types: &[&str]) -> SemanticLegend {
        SemanticLegend {
            kinds: types.iter().map(|t| SemanticKind::of(t)).collect(),
        }
    }

    fn span(text: &str) -> StyledSpan {
        StyledSpan {
            text: text.to_string(),
            fg: [0xc0, 0xc5, 0xce],
            bold: false,
            italic: false,
        }
    }

    #[test]
    fn deltas_expand_to_absolute_positions() {
        let legend = legend(&["struct", "function"]);
        // line 0 col 4 len 3 struct; same line col 10 len 2 function; line 2 col 1 len 5 struct
        let tokens = legend.decode(&[0, 4, 3, 0, 0, 0, 6, 2, 1, 0, 2, 1, 5, 0, 0]);
        let at: Vec<_> = tokens
            .iter()
            .map(|t| (t.line, t.utf16_start, t.utf16_len, t.kind))
            .collect();
        assert_eq!(
            at,
            vec![
                (0, 4, 3, SemanticKind::Type),
                (0, 10, 2, SemanticKind::Function),
                (2, 1, 5, SemanticKind::Type),
            ]
        );
    }

    #[test]
    fn a_type_the_legend_does_not_name_is_dropped() {
        let legend = legend(&["comment"]);
        assert!(legend.decode(&[0, 0, 4, 0, 0]).is_empty());
    }

    #[test]
    fn a_token_type_beyond_the_legend_is_dropped_rather_than_panicking() {
        let legend = legend(&["struct"]);
        assert!(legend.decode(&[0, 0, 4, 9, 0]).is_empty());
    }

    #[test]
    fn a_token_recolours_only_what_it_covers() {
        let hl = SemanticHighlight::of(vec![SemanticToken {
            line: 0,
            utf16_start: 4,
            utf16_len: 3,
            kind: SemanticKind::Type,
        }]);
        let out = hl.apply(0, vec![span("let Foo = 1;")], true);
        let texts: Vec<_> = out.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["let ", "Foo", " = 1;"]);
        assert_eq!(out[1].fg, SemanticKind::Type.colour(true));
        assert_eq!(
            out[0].fg,
            [0xc0, 0xc5, 0xce],
            "the rest keeps syntect's colour"
        );
    }

    /// Splitting has to survive a token that starts inside one syntect span and ends in another.
    #[test]
    fn a_token_spanning_two_syntect_spans_is_coloured_throughout() {
        let hl = SemanticHighlight::of(vec![SemanticToken {
            line: 0,
            utf16_start: 0,
            utf16_len: 6,
            kind: SemanticKind::Function,
        }]);
        let out = hl.apply(0, vec![span("foo"), span("bar!")], true);
        let coloured: String = out
            .iter()
            .filter(|s| s.fg == SemanticKind::Function.colour(true))
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(coloured, "foobar");
    }

    /// Columns are UTF-16, so the emoji counts twice and `Foo` starts at 9 rather than 8.
    /// Reading the column as chars would land the colour on `= F`.
    #[test]
    fn columns_are_utf16() {
        let hl = SemanticHighlight::of(vec![SemanticToken {
            line: 0,
            utf16_start: 9,
            utf16_len: 3,
            kind: SemanticKind::Type,
        }]);
        let out = hl.apply(0, vec![span("let 😀 = Foo;")], true);
        let coloured: String = out
            .iter()
            .filter(|s| s.fg == SemanticKind::Type.colour(true))
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(coloured, "Foo");
    }

    #[test]
    fn a_line_with_no_tokens_is_left_alone() {
        let hl = SemanticHighlight::of(vec![SemanticToken {
            line: 5,
            utf16_start: 0,
            utf16_len: 3,
            kind: SemanticKind::Type,
        }]);
        let out = hl.apply(0, vec![span("untouched")], true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].fg, [0xc0, 0xc5, 0xce]);
    }
}
