use syntect::highlighting::{
    Color, FontStyle, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};

pub struct Token {
    pub scope: &'static str,
    pub colour: &'static str,
    pub style: &'static str,
}

pub struct Palette {
    pub background: &'static str,
    pub foreground: &'static str,
    pub selection: &'static str,
    pub line_highlight: &'static str,
    pub caret: &'static str,
    pub ansi: [&'static str; 16],
    pub tokens: &'static [Token],
}

impl Palette {
    pub fn of(dark: bool) -> &'static Self {
        if dark { &GITHUB_DARK } else { &GITHUB_LIGHT }
    }

    pub fn theme(&self) -> Theme {
        let settings = ThemeSettings {
            background: Some(rgb(self.background)),
            foreground: Some(rgb(self.foreground)),
            selection: Some(rgb(self.selection)),
            line_highlight: Some(rgb(self.line_highlight)),
            caret: Some(rgb(self.caret)),
            ..Default::default()
        };

        let mut scopes = Vec::with_capacity(self.tokens.len());
        for token in self.tokens {
            let Ok(selectors) = token.scope.parse::<ScopeSelectors>() else {
                continue;
            };
            scopes.push(ThemeItem {
                scope: selectors,
                style: StyleModifier {
                    foreground: Some(rgb(token.colour)),
                    background: None,
                    font_style: Some(font_style(token.style)),
                },
            });
        }

        Theme {
            name: None,
            author: None,
            settings,
            scopes,
        }
    }

    pub fn foreground_rgb(&self) -> [u8; 3] {
        let colour = rgb(self.foreground);
        [colour.r, colour.g, colour.b]
    }
}

fn font_style(style: &str) -> FontStyle {
    let mut out = FontStyle::empty();
    for word in style.split_whitespace() {
        match word {
            "bold" => out |= FontStyle::BOLD,
            "italic" => out |= FontStyle::ITALIC,
            "underline" => out |= FontStyle::UNDERLINE,
            _ => {}
        }
    }
    out
}

fn rgb(hex: &str) -> Color {
    let digits = hex.strip_prefix('#').unwrap_or(hex);
    let byte =
        |at: usize| u8::from_str_radix(digits.get(at..at + 2).unwrap_or("00"), 16).unwrap_or(0);
    Color {
        r: byte(0),
        g: byte(2),
        b: byte(4),
        a: 0xff,
    }
}

pub const GITHUB_DARK: Palette = Palette {
    background: "#0d1117",
    foreground: "#e6edf3",
    selection: "#3fb950",
    line_highlight: "#6e7681",
    caret: "#2f81f7",
    ansi: [
        "#484f58", "#ff7b72", "#3fb950", "#d29922", "#58a6ff", "#bc8cff", "#39c5cf", "#b1bac4",
        "#6e7681", "#ffa198", "#56d364", "#e3b341", "#79c0ff", "#d2a8ff", "#56d4dd", "#ffffff",
    ],
    tokens: &[
        Token {
            scope: "comment, punctuation.definition.comment, string.comment",
            colour: "#8b949e",
            style: "",
        },
        Token {
            scope: "constant.other.placeholder, constant.character",
            colour: "#ff7b72",
            style: "",
        },
        Token {
            scope: "constant, entity.name.constant, variable.other.constant, variable.other.enummember, variable.language, entity",
            colour: "#79c0ff",
            style: "",
        },
        Token {
            scope: "entity.name, meta.export.default, meta.definition.variable",
            colour: "#ffa657",
            style: "",
        },
        Token {
            scope: "variable.parameter.function, meta.jsx.children, meta.block, meta.tag.attributes, entity.name.constant, meta.object.member, meta.embedded.expression",
            colour: "#e6edf3",
            style: "",
        },
        Token {
            scope: "entity.name.function",
            colour: "#d2a8ff",
            style: "",
        },
        Token {
            scope: "entity.name.tag, support.class.component",
            colour: "#7ee787",
            style: "",
        },
        Token {
            scope: "keyword",
            colour: "#ff7b72",
            style: "",
        },
        Token {
            scope: "storage, storage.type",
            colour: "#ff7b72",
            style: "",
        },
        Token {
            scope: "storage.modifier.package, storage.modifier.import, storage.type.java",
            colour: "#e6edf3",
            style: "",
        },
        Token {
            scope: "string, string punctuation.section.embedded source",
            colour: "#a5d6ff",
            style: "",
        },
        Token {
            scope: "support",
            colour: "#79c0ff",
            style: "",
        },
        Token {
            scope: "meta.property-name",
            colour: "#79c0ff",
            style: "",
        },
        Token {
            scope: "variable",
            colour: "#ffa657",
            style: "",
        },
        Token {
            scope: "variable.other",
            colour: "#e6edf3",
            style: "",
        },
        Token {
            scope: "invalid.broken",
            colour: "#ffa198",
            style: "italic",
        },
        Token {
            scope: "invalid.deprecated",
            colour: "#ffa198",
            style: "italic",
        },
        Token {
            scope: "invalid.illegal",
            colour: "#ffa198",
            style: "italic",
        },
        Token {
            scope: "invalid.unimplemented",
            colour: "#ffa198",
            style: "italic",
        },
        Token {
            scope: "carriage-return",
            colour: "#f0f6fc",
            style: "italic underline",
        },
        Token {
            scope: "message.error",
            colour: "#ffa198",
            style: "",
        },
        Token {
            scope: "string variable",
            colour: "#79c0ff",
            style: "",
        },
        Token {
            scope: "source.regexp, string.regexp",
            colour: "#a5d6ff",
            style: "",
        },
        Token {
            scope: "string.regexp.character-class, string.regexp constant.character.escape, string.regexp source.ruby.embedded, string.regexp string.regexp.arbitrary-repitition",
            colour: "#a5d6ff",
            style: "",
        },
        Token {
            scope: "string.regexp constant.character.escape",
            colour: "#7ee787",
            style: "bold",
        },
        Token {
            scope: "support.constant",
            colour: "#79c0ff",
            style: "",
        },
        Token {
            scope: "support.variable",
            colour: "#79c0ff",
            style: "",
        },
        Token {
            scope: "support.type.property-name.json",
            colour: "#7ee787",
            style: "",
        },
        Token {
            scope: "meta.module-reference",
            colour: "#79c0ff",
            style: "",
        },
        Token {
            scope: "punctuation.definition.list.begin.markdown",
            colour: "#ffa657",
            style: "",
        },
        Token {
            scope: "markup.heading, markup.heading entity.name",
            colour: "#79c0ff",
            style: "bold",
        },
        Token {
            scope: "markup.quote",
            colour: "#7ee787",
            style: "",
        },
        Token {
            scope: "markup.italic",
            colour: "#e6edf3",
            style: "italic",
        },
        Token {
            scope: "markup.bold",
            colour: "#e6edf3",
            style: "bold",
        },
        Token {
            scope: "markup.inline.raw",
            colour: "#79c0ff",
            style: "",
        },
        Token {
            scope: "markup.deleted, meta.diff.header.from-file, punctuation.definition.deleted",
            colour: "#ffa198",
            style: "",
        },
        Token {
            scope: "punctuation.section.embedded",
            colour: "#ff7b72",
            style: "",
        },
        Token {
            scope: "markup.inserted, meta.diff.header.to-file, punctuation.definition.inserted",
            colour: "#7ee787",
            style: "",
        },
        Token {
            scope: "markup.changed, punctuation.definition.changed",
            colour: "#ffa657",
            style: "",
        },
        Token {
            scope: "markup.ignored, markup.untracked",
            colour: "#161b22",
            style: "",
        },
        Token {
            scope: "meta.diff.range",
            colour: "#d2a8ff",
            style: "bold",
        },
        Token {
            scope: "meta.diff.header",
            colour: "#79c0ff",
            style: "",
        },
        Token {
            scope: "meta.separator",
            colour: "#79c0ff",
            style: "bold",
        },
        Token {
            scope: "meta.output",
            colour: "#79c0ff",
            style: "",
        },
        Token {
            scope: "brackethighlighter.tag, brackethighlighter.curly, brackethighlighter.round, brackethighlighter.square, brackethighlighter.angle, brackethighlighter.quote",
            colour: "#8b949e",
            style: "",
        },
        Token {
            scope: "brackethighlighter.unmatched",
            colour: "#ffa198",
            style: "",
        },
        Token {
            scope: "constant.other.reference.link, string.other.link",
            colour: "#a5d6ff",
            style: "",
        },
    ],
};

pub const GITHUB_LIGHT: Palette = Palette {
    background: "#ffffff",
    foreground: "#1f2328",
    selection: "#4ac26b",
    line_highlight: "#eaeef2",
    caret: "#0969da",
    ansi: [
        "#24292f", "#cf222e", "#116329", "#4d2d00", "#0969da", "#8250df", "#1b7c83", "#6e7781",
        "#57606a", "#a40e26", "#1a7f37", "#633c01", "#218bff", "#a475f9", "#3192aa", "#8c959f",
    ],
    tokens: &[
        Token {
            scope: "comment, punctuation.definition.comment, string.comment",
            colour: "#6e7781",
            style: "",
        },
        Token {
            scope: "constant.other.placeholder, constant.character",
            colour: "#cf222e",
            style: "",
        },
        Token {
            scope: "constant, entity.name.constant, variable.other.constant, variable.other.enummember, variable.language, entity",
            colour: "#0550ae",
            style: "",
        },
        Token {
            scope: "entity.name, meta.export.default, meta.definition.variable",
            colour: "#953800",
            style: "",
        },
        Token {
            scope: "variable.parameter.function, meta.jsx.children, meta.block, meta.tag.attributes, entity.name.constant, meta.object.member, meta.embedded.expression",
            colour: "#1f2328",
            style: "",
        },
        Token {
            scope: "entity.name.function",
            colour: "#8250df",
            style: "",
        },
        Token {
            scope: "entity.name.tag, support.class.component",
            colour: "#116329",
            style: "",
        },
        Token {
            scope: "keyword",
            colour: "#cf222e",
            style: "",
        },
        Token {
            scope: "storage, storage.type",
            colour: "#cf222e",
            style: "",
        },
        Token {
            scope: "storage.modifier.package, storage.modifier.import, storage.type.java",
            colour: "#1f2328",
            style: "",
        },
        Token {
            scope: "string, string punctuation.section.embedded source",
            colour: "#0a3069",
            style: "",
        },
        Token {
            scope: "support",
            colour: "#0550ae",
            style: "",
        },
        Token {
            scope: "meta.property-name",
            colour: "#0550ae",
            style: "",
        },
        Token {
            scope: "variable",
            colour: "#953800",
            style: "",
        },
        Token {
            scope: "variable.other",
            colour: "#1f2328",
            style: "",
        },
        Token {
            scope: "invalid.broken",
            colour: "#82071e",
            style: "italic",
        },
        Token {
            scope: "invalid.deprecated",
            colour: "#82071e",
            style: "italic",
        },
        Token {
            scope: "invalid.illegal",
            colour: "#82071e",
            style: "italic",
        },
        Token {
            scope: "invalid.unimplemented",
            colour: "#82071e",
            style: "italic",
        },
        Token {
            scope: "carriage-return",
            colour: "#f6f8fa",
            style: "italic underline",
        },
        Token {
            scope: "message.error",
            colour: "#82071e",
            style: "",
        },
        Token {
            scope: "string variable",
            colour: "#0550ae",
            style: "",
        },
        Token {
            scope: "source.regexp, string.regexp",
            colour: "#0a3069",
            style: "",
        },
        Token {
            scope: "string.regexp.character-class, string.regexp constant.character.escape, string.regexp source.ruby.embedded, string.regexp string.regexp.arbitrary-repitition",
            colour: "#0a3069",
            style: "",
        },
        Token {
            scope: "string.regexp constant.character.escape",
            colour: "#116329",
            style: "bold",
        },
        Token {
            scope: "support.constant",
            colour: "#0550ae",
            style: "",
        },
        Token {
            scope: "support.variable",
            colour: "#0550ae",
            style: "",
        },
        Token {
            scope: "support.type.property-name.json",
            colour: "#116329",
            style: "",
        },
        Token {
            scope: "meta.module-reference",
            colour: "#0550ae",
            style: "",
        },
        Token {
            scope: "punctuation.definition.list.begin.markdown",
            colour: "#953800",
            style: "",
        },
        Token {
            scope: "markup.heading, markup.heading entity.name",
            colour: "#0550ae",
            style: "bold",
        },
        Token {
            scope: "markup.quote",
            colour: "#116329",
            style: "",
        },
        Token {
            scope: "markup.italic",
            colour: "#1f2328",
            style: "italic",
        },
        Token {
            scope: "markup.bold",
            colour: "#1f2328",
            style: "bold",
        },
        Token {
            scope: "markup.inline.raw",
            colour: "#0550ae",
            style: "",
        },
        Token {
            scope: "markup.deleted, meta.diff.header.from-file, punctuation.definition.deleted",
            colour: "#82071e",
            style: "",
        },
        Token {
            scope: "punctuation.section.embedded",
            colour: "#cf222e",
            style: "",
        },
        Token {
            scope: "markup.inserted, meta.diff.header.to-file, punctuation.definition.inserted",
            colour: "#116329",
            style: "",
        },
        Token {
            scope: "markup.changed, punctuation.definition.changed",
            colour: "#953800",
            style: "",
        },
        Token {
            scope: "markup.ignored, markup.untracked",
            colour: "#eaeef2",
            style: "",
        },
        Token {
            scope: "meta.diff.range",
            colour: "#8250df",
            style: "bold",
        },
        Token {
            scope: "meta.diff.header",
            colour: "#0550ae",
            style: "",
        },
        Token {
            scope: "meta.separator",
            colour: "#0550ae",
            style: "bold",
        },
        Token {
            scope: "meta.output",
            colour: "#0550ae",
            style: "",
        },
        Token {
            scope: "brackethighlighter.tag, brackethighlighter.curly, brackethighlighter.round, brackethighlighter.square, brackethighlighter.angle, brackethighlighter.quote",
            colour: "#57606a",
            style: "",
        },
        Token {
            scope: "brackethighlighter.unmatched",
            colour: "#82071e",
            style: "",
        },
        Token {
            scope: "constant.other.reference.link, string.other.link",
            colour: "#0a3069",
            style: "",
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
        assert_eq!(GITHUB_DARK.background, "#0d1117");
        assert_eq!(GITHUB_LIGHT.background, "#ffffff");
    }
}
