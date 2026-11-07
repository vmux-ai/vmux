use dioxus::prelude::*;

use crate::cn::cn;

const MAX_HIGHLIGHT_SPANS: usize = 128;

#[component]
pub fn ShellCode(
    source: String,
    #[props(default)] inline: bool,
    #[props(default)] class: String,
) -> Element {
    let wrapping = if inline {
        "whitespace-nowrap"
    } else {
        "whitespace-pre-wrap break-words"
    };
    let class = cn([
        "font-mono text-[11px] leading-relaxed",
        wrapping,
        class.as_str(),
    ]);
    let syntax = ShellSyntax::parse(&source);
    rsx! {
        code { class,
            for (index, span) in syntax.0.into_iter().enumerate() {
                span { key: "{index}", class: span.tone.class(), "{span.text}" }
            }
        }
    }
}

struct ShellSyntax(Vec<ShellSpan>);

impl ShellSyntax {
    fn parse(source: &str) -> Self {
        let mut spans = Vec::new();
        let mut index = 0;
        let mut expects_command = true;
        while index < source.len() {
            if spans.len() >= MAX_HIGHLIGHT_SPANS {
                spans.push(ShellSpan::new(&source[index..], ShellTone::Plain));
                break;
            }
            let character = source[index..].chars().next().unwrap_or_default();
            if character.is_whitespace() {
                let end = Self::take_while(source, index, char::is_whitespace);
                if source[index..end].contains('\n') {
                    expects_command = true;
                }
                spans.push(ShellSpan::new(&source[index..end], ShellTone::Plain));
                index = end;
                continue;
            }
            if character == '#' && Self::starts_token(source, index) {
                let end = source[index..]
                    .find('\n')
                    .map(|offset| index + offset)
                    .unwrap_or(source.len());
                spans.push(ShellSpan::new(&source[index..end], ShellTone::Comment));
                index = end;
                continue;
            }
            if matches!(character, '\'' | '"' | '`') {
                let end = Self::quoted_end(source, index, character);
                spans.push(ShellSpan::new(&source[index..end], ShellTone::String));
                expects_command = false;
                index = end;
                continue;
            }
            if character == '$' {
                let end = Self::variable_end(source, index);
                spans.push(ShellSpan::new(&source[index..end], ShellTone::Variable));
                expects_command = false;
                index = end;
                continue;
            }
            if Self::operator(character) {
                let end = Self::take_while(source, index, Self::operator);
                let operator = &source[index..end];
                if operator.contains(['|', '&', ';']) {
                    expects_command = true;
                }
                spans.push(ShellSpan::new(operator, ShellTone::Operator));
                index = end;
                continue;
            }
            let end = Self::take_while(source, index, |candidate| {
                !candidate.is_whitespace()
                    && !Self::operator(candidate)
                    && !matches!(candidate, '\'' | '"' | '`' | '$')
            });
            let word = &source[index..end];
            let assignment = word
                .split_once('=')
                .is_some_and(|(name, _)| Self::identifier(name));
            let tone = if assignment {
                ShellTone::Variable
            } else if Self::keyword(word) {
                ShellTone::Keyword
            } else if word.starts_with('-') {
                ShellTone::Option
            } else if expects_command {
                ShellTone::Command
            } else if word.parse::<f64>().is_ok() {
                ShellTone::Number
            } else {
                ShellTone::Plain
            };
            if !assignment {
                expects_command = Self::command_prefix(word);
            }
            spans.push(ShellSpan::new(word, tone));
            index = end;
        }
        Self(spans)
    }

    fn take_while(source: &str, start: usize, predicate: impl Fn(char) -> bool) -> usize {
        let mut end = start;
        for (offset, character) in source[start..].char_indices() {
            if !predicate(character) {
                break;
            }
            end = start + offset + character.len_utf8();
        }
        end
    }

    fn quoted_end(source: &str, start: usize, quote: char) -> usize {
        let mut escaped = false;
        let content = start + quote.len_utf8();
        for (offset, character) in source[content..].char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if character == '\\' {
                escaped = true;
                continue;
            }
            if character == quote {
                return content + offset + character.len_utf8();
            }
        }
        source.len()
    }

    fn variable_end(source: &str, start: usize) -> usize {
        let content = start + 1;
        if source[content..].starts_with('{') {
            return source[content..]
                .find('}')
                .map(|offset| content + offset + 1)
                .unwrap_or(source.len());
        }
        Self::take_while(source, content, |character| {
            character.is_ascii_alphanumeric() || character == '_'
        })
        .max(content)
    }

    fn starts_token(source: &str, index: usize) -> bool {
        source[..index]
            .chars()
            .next_back()
            .is_none_or(|character| character.is_whitespace() || Self::operator(character))
    }

    fn operator(character: char) -> bool {
        matches!(character, '|' | '&' | ';' | '<' | '>')
    }

    fn keyword(word: &str) -> bool {
        matches!(
            word,
            "if" | "then"
                | "else"
                | "elif"
                | "fi"
                | "for"
                | "while"
                | "until"
                | "do"
                | "done"
                | "case"
                | "esac"
                | "in"
                | "function"
        )
    }

    fn command_prefix(word: &str) -> bool {
        matches!(word, "command" | "env" | "exec" | "sudo" | "time" | "xargs")
    }

    fn identifier(value: &str) -> bool {
        !value.is_empty()
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
    }
}

struct ShellSpan {
    text: String,
    tone: ShellTone,
}

impl ShellSpan {
    fn new(text: &str, tone: ShellTone) -> Self {
        Self {
            text: text.to_string(),
            tone,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShellTone {
    Plain,
    Command,
    Keyword,
    Option,
    String,
    Variable,
    Operator,
    Number,
    Comment,
}

impl ShellTone {
    fn class(self) -> &'static str {
        match self {
            Self::Plain => "text-foreground/72",
            Self::Command => "font-medium text-[var(--syntax-shell-command)]",
            Self::Keyword => "text-[var(--syntax-shell-keyword)]",
            Self::Option => "text-[var(--syntax-shell-option)]",
            Self::String => "text-[var(--syntax-shell-string)]",
            Self::Variable => "text-[var(--syntax-shell-variable)]",
            Self::Operator => "text-[var(--syntax-shell-operator)]",
            Self::Number => "text-[var(--syntax-shell-number)]",
            Self::Comment => "text-[var(--syntax-shell-comment)]",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_syntax_preserves_source_and_classifies_common_tokens() {
        let source = "TOKEN=value cargo test --lib | sed 's/a/b/' # done";
        let syntax = ShellSyntax::parse(source);
        assert_eq!(
            syntax
                .0
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            source
        );
        assert!(syntax.0.iter().any(|span| span.tone == ShellTone::Variable));
        assert!(syntax.0.iter().any(|span| span.tone == ShellTone::Command));
        assert!(syntax.0.iter().any(|span| span.tone == ShellTone::Option));
        assert!(syntax.0.iter().any(|span| span.tone == ShellTone::Operator));
        assert!(syntax.0.iter().any(|span| span.tone == ShellTone::String));
        assert!(syntax.0.iter().any(|span| span.tone == ShellTone::Comment));
    }

    #[test]
    fn shell_syntax_caps_rendered_spans_without_losing_source() {
        let source = (0..1_000)
            .map(|index| format!("word{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let syntax = ShellSyntax::parse(&source);

        assert!(syntax.0.len() <= MAX_HIGHLIGHT_SPANS + 1);
        assert_eq!(
            syntax
                .0
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            source
        );
    }
}
