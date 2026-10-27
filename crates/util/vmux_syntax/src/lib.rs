use syntect::highlighting::{
    Color, FontStyle, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};

#[derive(Clone, Copy)]
struct Token {
    scope: TokenScope,
    colour: PaletteColour,
    style: TokenStyle,
}

#[derive(Clone, Copy)]
pub struct Palette {
    background: PaletteColour,
    foreground: PaletteColour,
    selection: PaletteColour,
    line_highlight: PaletteColour,
    caret: PaletteColour,
    ansi: [PaletteColour; 16],
    tokens: &'static [Token],
}

impl Palette {
    pub fn for_scheme(dark: bool) -> Self {
        if dark { GITHUB_DARK } else { GITHUB_LIGHT }
    }

    pub fn theme(&self) -> Theme {
        let settings = ThemeSettings {
            background: Some(self.background.syntect()),
            foreground: Some(self.foreground.syntect()),
            selection: Some(self.selection.syntect()),
            line_highlight: Some(self.line_highlight.syntect()),
            caret: Some(self.caret.syntect()),
            ..Default::default()
        };

        let mut scopes = Vec::with_capacity(self.tokens.len());
        for token in self.tokens {
            if let Some(item) = token.theme_item() {
                scopes.push(item);
            }
        }

        Theme {
            name: None,
            author: None,
            settings,
            scopes,
        }
    }

    pub fn foreground_rgb(&self) -> [u8; 3] {
        self.foreground.rgb()
    }

    pub fn ansi(&self) -> [PaletteColour; 16] {
        self.ansi
    }
}

impl Token {
    fn theme_item(self) -> Option<ThemeItem> {
        let Ok(scope) = self.scope.0.parse::<ScopeSelectors>() else {
            return None;
        };
        Some(ThemeItem {
            scope,
            style: StyleModifier {
                foreground: Some(self.colour.syntect()),
                background: None,
                font_style: Some(self.style.syntect()),
            },
        })
    }
}

#[derive(Clone, Copy)]
struct TokenScope(&'static str);

impl TokenScope {
    const fn new(value: &'static str) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy)]
enum TokenStyle {
    Plain,
    Bold,
    Italic,
    ItalicUnderline,
}

impl TokenStyle {
    fn syntect(self) -> FontStyle {
        match self {
            Self::Plain => FontStyle::empty(),
            Self::Bold => FontStyle::BOLD,
            Self::Italic => FontStyle::ITALIC,
            Self::ItalicUnderline => FontStyle::ITALIC | FontStyle::UNDERLINE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteColour([u8; 3]);

impl PaletteColour {
    const fn hex(value: u32) -> Self {
        Self([
            ((value >> 16) & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
        ])
    }

    pub const fn rgb(self) -> [u8; 3] {
        self.0
    }

    const fn syntect(self) -> Color {
        Color {
            r: self.0[0],
            g: self.0[1],
            b: self.0[2],
            a: 0xff,
        }
    }
}

pub const GITHUB_DARK: Palette = Palette {
    background: PaletteColour::hex(0x0d1117),
    foreground: PaletteColour::hex(0xe6edf3),
    selection: PaletteColour::hex(0x3fb950),
    line_highlight: PaletteColour::hex(0x6e7681),
    caret: PaletteColour::hex(0x2f81f7),
    ansi: [
        PaletteColour::hex(0x484f58),
        PaletteColour::hex(0xff7b72),
        PaletteColour::hex(0x3fb950),
        PaletteColour::hex(0xd29922),
        PaletteColour::hex(0x58a6ff),
        PaletteColour::hex(0xbc8cff),
        PaletteColour::hex(0x39c5cf),
        PaletteColour::hex(0xb1bac4),
        PaletteColour::hex(0x6e7681),
        PaletteColour::hex(0xffa198),
        PaletteColour::hex(0x56d364),
        PaletteColour::hex(0xe3b341),
        PaletteColour::hex(0x79c0ff),
        PaletteColour::hex(0xd2a8ff),
        PaletteColour::hex(0x56d4dd),
        PaletteColour::hex(0xffffff),
    ],
    tokens: &[
        Token {
            scope: TokenScope::new("comment, punctuation.definition.comment, string.comment"),
            colour: PaletteColour::hex(0x8b949e),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("constant.other.placeholder, constant.character"),
            colour: PaletteColour::hex(0xff7b72),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "constant, entity.name.constant, variable.other.constant, variable.other.enummember, variable.language, entity",
            ),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("entity.name, meta.export.default, meta.definition.variable"),
            colour: PaletteColour::hex(0xffa657),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "variable.parameter.function, meta.jsx.children, meta.block, meta.tag.attributes, entity.name.constant, meta.object.member, meta.embedded.expression",
            ),
            colour: PaletteColour::hex(0xe6edf3),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("entity.name.function"),
            colour: PaletteColour::hex(0xd2a8ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("entity.name.tag, support.class.component"),
            colour: PaletteColour::hex(0x7ee787),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("keyword"),
            colour: PaletteColour::hex(0xff7b72),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("storage, storage.type"),
            colour: PaletteColour::hex(0xff7b72),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "storage.modifier.package, storage.modifier.import, storage.type.java",
            ),
            colour: PaletteColour::hex(0xe6edf3),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("string, string punctuation.section.embedded source"),
            colour: PaletteColour::hex(0xa5d6ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("support"),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("meta.property-name"),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("variable"),
            colour: PaletteColour::hex(0xffa657),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("variable.other"),
            colour: PaletteColour::hex(0xe6edf3),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("invalid.broken"),
            colour: PaletteColour::hex(0xffa198),
            style: TokenStyle::Italic,
        },
        Token {
            scope: TokenScope::new("invalid.deprecated"),
            colour: PaletteColour::hex(0xffa198),
            style: TokenStyle::Italic,
        },
        Token {
            scope: TokenScope::new("invalid.illegal"),
            colour: PaletteColour::hex(0xffa198),
            style: TokenStyle::Italic,
        },
        Token {
            scope: TokenScope::new("invalid.unimplemented"),
            colour: PaletteColour::hex(0xffa198),
            style: TokenStyle::Italic,
        },
        Token {
            scope: TokenScope::new("carriage-return"),
            colour: PaletteColour::hex(0xf0f6fc),
            style: TokenStyle::ItalicUnderline,
        },
        Token {
            scope: TokenScope::new("message.error"),
            colour: PaletteColour::hex(0xffa198),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("string variable"),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("source.regexp, string.regexp"),
            colour: PaletteColour::hex(0xa5d6ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "string.regexp.character-class, string.regexp constant.character.escape, string.regexp source.ruby.embedded, string.regexp string.regexp.arbitrary-repitition",
            ),
            colour: PaletteColour::hex(0xa5d6ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("string.regexp constant.character.escape"),
            colour: PaletteColour::hex(0x7ee787),
            style: TokenStyle::Bold,
        },
        Token {
            scope: TokenScope::new("support.constant"),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("support.variable"),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("support.type.property-name.json"),
            colour: PaletteColour::hex(0x7ee787),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("meta.module-reference"),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("punctuation.definition.list.begin.markdown"),
            colour: PaletteColour::hex(0xffa657),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("markup.heading, markup.heading entity.name"),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Bold,
        },
        Token {
            scope: TokenScope::new("markup.quote"),
            colour: PaletteColour::hex(0x7ee787),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("markup.italic"),
            colour: PaletteColour::hex(0xe6edf3),
            style: TokenStyle::Italic,
        },
        Token {
            scope: TokenScope::new("markup.bold"),
            colour: PaletteColour::hex(0xe6edf3),
            style: TokenStyle::Bold,
        },
        Token {
            scope: TokenScope::new("markup.inline.raw"),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "markup.deleted, meta.diff.header.from-file, punctuation.definition.deleted",
            ),
            colour: PaletteColour::hex(0xffa198),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("punctuation.section.embedded"),
            colour: PaletteColour::hex(0xff7b72),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "markup.inserted, meta.diff.header.to-file, punctuation.definition.inserted",
            ),
            colour: PaletteColour::hex(0x7ee787),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("markup.changed, punctuation.definition.changed"),
            colour: PaletteColour::hex(0xffa657),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("markup.ignored, markup.untracked"),
            colour: PaletteColour::hex(0x161b22),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("meta.diff.range"),
            colour: PaletteColour::hex(0xd2a8ff),
            style: TokenStyle::Bold,
        },
        Token {
            scope: TokenScope::new("meta.diff.header"),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("meta.separator"),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Bold,
        },
        Token {
            scope: TokenScope::new("meta.output"),
            colour: PaletteColour::hex(0x79c0ff),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "brackethighlighter.tag, brackethighlighter.curly, brackethighlighter.round, brackethighlighter.square, brackethighlighter.angle, brackethighlighter.quote",
            ),
            colour: PaletteColour::hex(0x8b949e),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("brackethighlighter.unmatched"),
            colour: PaletteColour::hex(0xffa198),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("constant.other.reference.link, string.other.link"),
            colour: PaletteColour::hex(0xa5d6ff),
            style: TokenStyle::Plain,
        },
    ],
};

pub const GITHUB_LIGHT: Palette = Palette {
    background: PaletteColour::hex(0xffffff),
    foreground: PaletteColour::hex(0x1f2328),
    selection: PaletteColour::hex(0x4ac26b),
    line_highlight: PaletteColour::hex(0xeaeef2),
    caret: PaletteColour::hex(0x0969da),
    ansi: [
        PaletteColour::hex(0x24292f),
        PaletteColour::hex(0xcf222e),
        PaletteColour::hex(0x116329),
        PaletteColour::hex(0x4d2d00),
        PaletteColour::hex(0x0969da),
        PaletteColour::hex(0x8250df),
        PaletteColour::hex(0x1b7c83),
        PaletteColour::hex(0x6e7781),
        PaletteColour::hex(0x57606a),
        PaletteColour::hex(0xa40e26),
        PaletteColour::hex(0x1a7f37),
        PaletteColour::hex(0x633c01),
        PaletteColour::hex(0x218bff),
        PaletteColour::hex(0xa475f9),
        PaletteColour::hex(0x3192aa),
        PaletteColour::hex(0x8c959f),
    ],
    tokens: &[
        Token {
            scope: TokenScope::new("comment, punctuation.definition.comment, string.comment"),
            colour: PaletteColour::hex(0x6e7781),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("constant.other.placeholder, constant.character"),
            colour: PaletteColour::hex(0xcf222e),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "constant, entity.name.constant, variable.other.constant, variable.other.enummember, variable.language, entity",
            ),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("entity.name, meta.export.default, meta.definition.variable"),
            colour: PaletteColour::hex(0x953800),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "variable.parameter.function, meta.jsx.children, meta.block, meta.tag.attributes, entity.name.constant, meta.object.member, meta.embedded.expression",
            ),
            colour: PaletteColour::hex(0x1f2328),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("entity.name.function"),
            colour: PaletteColour::hex(0x8250df),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("entity.name.tag, support.class.component"),
            colour: PaletteColour::hex(0x116329),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("keyword"),
            colour: PaletteColour::hex(0xcf222e),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("storage, storage.type"),
            colour: PaletteColour::hex(0xcf222e),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "storage.modifier.package, storage.modifier.import, storage.type.java",
            ),
            colour: PaletteColour::hex(0x1f2328),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("string, string punctuation.section.embedded source"),
            colour: PaletteColour::hex(0x0a3069),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("support"),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("meta.property-name"),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("variable"),
            colour: PaletteColour::hex(0x953800),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("variable.other"),
            colour: PaletteColour::hex(0x1f2328),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("invalid.broken"),
            colour: PaletteColour::hex(0x82071e),
            style: TokenStyle::Italic,
        },
        Token {
            scope: TokenScope::new("invalid.deprecated"),
            colour: PaletteColour::hex(0x82071e),
            style: TokenStyle::Italic,
        },
        Token {
            scope: TokenScope::new("invalid.illegal"),
            colour: PaletteColour::hex(0x82071e),
            style: TokenStyle::Italic,
        },
        Token {
            scope: TokenScope::new("invalid.unimplemented"),
            colour: PaletteColour::hex(0x82071e),
            style: TokenStyle::Italic,
        },
        Token {
            scope: TokenScope::new("carriage-return"),
            colour: PaletteColour::hex(0xf6f8fa),
            style: TokenStyle::ItalicUnderline,
        },
        Token {
            scope: TokenScope::new("message.error"),
            colour: PaletteColour::hex(0x82071e),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("string variable"),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("source.regexp, string.regexp"),
            colour: PaletteColour::hex(0x0a3069),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "string.regexp.character-class, string.regexp constant.character.escape, string.regexp source.ruby.embedded, string.regexp string.regexp.arbitrary-repitition",
            ),
            colour: PaletteColour::hex(0x0a3069),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("string.regexp constant.character.escape"),
            colour: PaletteColour::hex(0x116329),
            style: TokenStyle::Bold,
        },
        Token {
            scope: TokenScope::new("support.constant"),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("support.variable"),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("support.type.property-name.json"),
            colour: PaletteColour::hex(0x116329),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("meta.module-reference"),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("punctuation.definition.list.begin.markdown"),
            colour: PaletteColour::hex(0x953800),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("markup.heading, markup.heading entity.name"),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Bold,
        },
        Token {
            scope: TokenScope::new("markup.quote"),
            colour: PaletteColour::hex(0x116329),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("markup.italic"),
            colour: PaletteColour::hex(0x1f2328),
            style: TokenStyle::Italic,
        },
        Token {
            scope: TokenScope::new("markup.bold"),
            colour: PaletteColour::hex(0x1f2328),
            style: TokenStyle::Bold,
        },
        Token {
            scope: TokenScope::new("markup.inline.raw"),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "markup.deleted, meta.diff.header.from-file, punctuation.definition.deleted",
            ),
            colour: PaletteColour::hex(0x82071e),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("punctuation.section.embedded"),
            colour: PaletteColour::hex(0xcf222e),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "markup.inserted, meta.diff.header.to-file, punctuation.definition.inserted",
            ),
            colour: PaletteColour::hex(0x116329),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("markup.changed, punctuation.definition.changed"),
            colour: PaletteColour::hex(0x953800),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("markup.ignored, markup.untracked"),
            colour: PaletteColour::hex(0xeaeef2),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("meta.diff.range"),
            colour: PaletteColour::hex(0x8250df),
            style: TokenStyle::Bold,
        },
        Token {
            scope: TokenScope::new("meta.diff.header"),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("meta.separator"),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Bold,
        },
        Token {
            scope: TokenScope::new("meta.output"),
            colour: PaletteColour::hex(0x0550ae),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new(
                "brackethighlighter.tag, brackethighlighter.curly, brackethighlighter.round, brackethighlighter.square, brackethighlighter.angle, brackethighlighter.quote",
            ),
            colour: PaletteColour::hex(0x57606a),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("brackethighlighter.unmatched"),
            colour: PaletteColour::hex(0x82071e),
            style: TokenStyle::Plain,
        },
        Token {
            scope: TokenScope::new("constant.other.reference.link, string.other.link"),
            colour: PaletteColour::hex(0x0a3069),
            style: TokenStyle::Plain,
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;
    use syntect::highlighting::Highlighter;
    use syntect::parsing::ScopeStack;

    #[test]
    fn every_rule_survives_the_trip_into_syntect() {
        for (name, palette) in [("dark", &GITHUB_DARK), ("light", &GITHUB_LIGHT)] {
            let parsed = palette.theme().scopes.len();
            assert_eq!(
                parsed,
                palette.tokens.len(),
                "{name}: {} of {} scope selectors failed to parse, and a dropped rule is a token \
                 silently drawn in the foreground colour",
                palette.tokens.len() - parsed,
                palette.tokens.len()
            );
        }
    }

    #[test]
    fn a_comment_and_a_keyword_are_the_colours_github_gives_them() {
        let theme = GITHUB_DARK.theme();
        let highlighter = Highlighter::new(&theme);
        let colour_of = |scope: &str| {
            let mut stack = ScopeStack::new();
            stack.push(scope.parse().expect("a scope parses"));
            let style = highlighter.style_for_stack(stack.as_slice());
            format!(
                "#{:02x}{:02x}{:02x}",
                style.foreground.r, style.foreground.g, style.foreground.b
            )
        };

        assert_eq!(colour_of("comment.line.double-slash"), "#8b949e");
        assert_eq!(colour_of("keyword.control"), "#ff7b72");
        assert_eq!(colour_of("string.quoted.double"), "#a5d6ff");
        assert_eq!(colour_of("entity.name.function"), "#d2a8ff");
    }

    #[test]
    fn the_two_schemes_do_not_share_a_background() {
        assert_eq!(GITHUB_DARK.background, PaletteColour::hex(0x0d1117));
        assert_eq!(GITHUB_LIGHT.background, PaletteColour::hex(0xffffff));
    }
}
