use crate::edit::command::{EditCommand, Motion};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExCommand {
    Write,
    WriteQuit,
    Quit {
        force: bool,
    },
    NoHighlight,
    Goto(usize),
    Delete(ExRange),
    Yank(ExRange),
    Substitute {
        range: ExRange,
        pattern: String,
        replacement: String,
        all: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ExRange {
    #[default]
    CurrentLine,
    WholeFile,
    Selection,
    Lines(usize, usize),
}

fn parse_address(text: &str) -> Option<usize> {
    text.parse::<usize>().ok().map(|n| n.saturating_sub(1))
}

fn split_range(input: &str) -> (Option<ExRange>, &str) {
    if let Some(rest) = input.strip_prefix('%') {
        return (Some(ExRange::WholeFile), rest);
    }
    if let Some(rest) = input.strip_prefix("'<,'>") {
        return (Some(ExRange::Selection), rest);
    }
    let digits: String = input.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return (None, input);
    }
    let rest = &input[digits.len()..];
    let Some(first) = parse_address(&digits) else {
        return (None, input);
    };
    if let Some(after) = rest.strip_prefix(',') {
        let second: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Some(last) = parse_address(&second) {
            return (Some(ExRange::Lines(first, last)), &after[second.len()..]);
        }
    }
    (Some(ExRange::Lines(first, first)), rest)
}

fn split_substitute(body: &str) -> Option<(String, String, String)> {
    let mut chars = body.chars();
    let delim = chars.next()?;
    let mut parts = vec![String::new()];
    let mut escaped = false;
    for c in chars {
        if escaped {
            if c != delim {
                parts.last_mut()?.push('\\');
            }
            parts.last_mut()?.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == delim {
            if parts.len() == 3 {
                break;
            }
            parts.push(String::new());
            continue;
        }
        parts.last_mut()?.push(c);
    }
    while parts.len() < 3 {
        parts.push(String::new());
    }
    Some((parts[0].clone(), parts[1].clone(), parts[2].clone()))
}

impl ExCommand {
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }
        let (range, rest) = split_range(input);
        let rest = rest.trim();

        if rest.is_empty() {
            return match range {
                Some(ExRange::Lines(line, _)) => Some(Self::Goto(line)),
                _ => None,
            };
        }

        let range = range.unwrap_or_default();
        if let Some(body) = rest.strip_prefix("s").filter(|b| !b.is_empty())
            && !body.starts_with(|c: char| c.is_alphanumeric())
        {
            let (pattern, replacement, flags) = split_substitute(body)?;
            return Some(Self::Substitute {
                range,
                pattern,
                replacement,
                all: flags.contains('g'),
            });
        }

        match rest {
            "w" | "write" => Some(Self::Write),
            "wq" | "x" | "xit" => Some(Self::WriteQuit),
            "q" | "quit" => Some(Self::Quit { force: false }),
            "q!" | "quit!" => Some(Self::Quit { force: true }),
            "noh" | "nohl" | "nohlsearch" => Some(Self::NoHighlight),
            "d" | "delete" => Some(Self::Delete(range)),
            "y" | "yank" => Some(Self::Yank(range)),
            _ => None,
        }
    }

    pub fn edits(self) -> Vec<EditCommand> {
        match self {
            Self::Write | Self::WriteQuit => vec![EditCommand::Save],
            Self::Quit { .. } => Vec::new(),
            Self::NoHighlight => vec![EditCommand::ClearSearchHighlight],
            Self::Goto(line) => vec![EditCommand::Move(Motion::GotoLine(line as u32))],
            Self::Delete(range) => vec![EditCommand::ExDelete(range)],
            Self::Yank(range) => vec![EditCommand::ExYank(range)],
            Self::Substitute { pattern, .. } if pattern.is_empty() => Vec::new(),
            Self::Substitute {
                range,
                pattern,
                replacement,
                all,
            } => vec![EditCommand::Substitute {
                range,
                pattern,
                replacement,
                all,
            }],
        }
    }
}

pub struct ExLine;

impl ExLine {
    pub fn edits(line: &str) -> Vec<EditCommand> {
        let Some(command) = ExCommand::parse(line) else {
            return Vec::new();
        };
        command.edits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_wire::command_bar::ExCommandName;

    #[test]
    fn bare_commands() {
        assert_eq!(ExCommand::parse("w"), Some(ExCommand::Write));
        assert_eq!(ExCommand::parse("wq"), Some(ExCommand::WriteQuit));
        assert_eq!(
            ExCommand::parse("q"),
            Some(ExCommand::Quit { force: false })
        );
        assert_eq!(
            ExCommand::parse("q!"),
            Some(ExCommand::Quit { force: true })
        );
        assert_eq!(ExCommand::parse("noh"), Some(ExCommand::NoHighlight));
        assert_eq!(ExCommand::parse("bogus"), None);
        assert_eq!(ExCommand::parse(""), None);
    }

    #[test]
    fn a_bare_number_is_a_goto() {
        assert_eq!(ExCommand::parse("42"), Some(ExCommand::Goto(41)));
        assert_eq!(ExCommand::parse("1"), Some(ExCommand::Goto(0)));
    }

    #[test]
    fn substitute_parses_pattern_replacement_and_flags() {
        assert_eq!(
            ExCommand::parse("%s/foo/bar/g"),
            Some(ExCommand::Substitute {
                range: ExRange::WholeFile,
                pattern: "foo".into(),
                replacement: "bar".into(),
                all: true,
            })
        );
        assert_eq!(
            ExCommand::parse("s/foo/bar"),
            Some(ExCommand::Substitute {
                range: ExRange::CurrentLine,
                pattern: "foo".into(),
                replacement: "bar".into(),
                all: false,
            })
        );
    }

    #[test]
    fn substitute_accepts_a_line_range_and_a_selection() {
        assert_eq!(
            ExCommand::parse("2,5s/a/b/"),
            Some(ExCommand::Substitute {
                range: ExRange::Lines(1, 4),
                pattern: "a".into(),
                replacement: "b".into(),
                all: false,
            })
        );
        assert_eq!(
            ExCommand::parse("'<,'>s/a/b/"),
            Some(ExCommand::Substitute {
                range: ExRange::Selection,
                pattern: "a".into(),
                replacement: "b".into(),
                all: false,
            })
        );
    }

    #[test]
    fn an_escaped_delimiter_stays_in_the_pattern() {
        assert_eq!(
            ExCommand::parse("s/a\\/b/c/"),
            Some(ExCommand::Substitute {
                range: ExRange::CurrentLine,
                pattern: "a/b".into(),
                replacement: "c".into(),
                all: false,
            })
        );
    }

    #[test]
    fn a_non_slash_delimiter_works() {
        assert_eq!(
            ExCommand::parse("s#a#b#g"),
            Some(ExCommand::Substitute {
                range: ExRange::CurrentLine,
                pattern: "a".into(),
                replacement: "b".into(),
                all: true,
            })
        );
    }

    #[test]
    fn ranged_delete_and_yank() {
        assert_eq!(
            ExCommand::parse("%d"),
            Some(ExCommand::Delete(ExRange::WholeFile))
        );
        assert_eq!(
            ExCommand::parse("3,4y"),
            Some(ExCommand::Yank(ExRange::Lines(2, 3)))
        );
    }

    #[test]
    fn a_word_starting_with_s_is_not_a_substitute() {
        assert_eq!(ExCommand::parse("sort"), None);
    }

    #[test]
    fn a_line_reaches_the_editor_without_a_keymap() {
        assert_eq!(ExLine::edits("w"), vec![EditCommand::Save]);
        assert_eq!(ExLine::edits("q"), Vec::new());
        assert_eq!(ExLine::edits("bogus"), Vec::new());
        assert_eq!(
            ExLine::edits("noh"),
            vec![EditCommand::ClearSearchHighlight]
        );
        assert_eq!(
            ExLine::edits("12"),
            vec![EditCommand::Move(Motion::GotoLine(11))]
        );
        assert_eq!(
            ExLine::edits("%s/a/b/g"),
            vec![EditCommand::Substitute {
                range: ExRange::WholeFile,
                pattern: "a".into(),
                replacement: "b".into(),
                all: true,
            }]
        );
    }

    #[test]
    fn a_substitute_with_no_pattern_leaves_the_buffer_alone() {
        assert_eq!(ExLine::edits("s/"), Vec::new());
        assert_eq!(ExLine::edits("%s//x/g"), Vec::new());
    }

    #[test]
    fn every_offered_ex_command_is_one_the_parser_accepts() {
        for entry in ExCommandName::ALL {
            assert!(
                ExCommand::parse(entry.name).is_some(),
                "`:{}` is offered as a completion but the parser rejects it",
                entry.name
            );
        }
    }
}
