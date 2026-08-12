/// A `:` command line, parsed into a range and an action.
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

/// The line span a command applies to, resolved against the buffer later.
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

/// Split a leading range off the command line, returning it with the rest.
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

/// Split `s/pat/rep/flags` on its delimiter, honouring backslash escapes.
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

pub fn parse(input: &str) -> Option<ExCommand> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    let (range, rest) = split_range(input);
    let rest = rest.trim();

    if rest.is_empty() {
        return match range {
            Some(ExRange::Lines(line, _)) => Some(ExCommand::Goto(line)),
            _ => None,
        };
    }

    let range = range.unwrap_or_default();
    if let Some(body) = rest.strip_prefix("s").filter(|b| !b.is_empty())
        && !body.starts_with(|c: char| c.is_alphanumeric())
    {
        let (pattern, replacement, flags) = split_substitute(body)?;
        return Some(ExCommand::Substitute {
            range,
            pattern,
            replacement,
            all: flags.contains('g'),
        });
    }

    match rest {
        "w" | "write" => Some(ExCommand::Write),
        "wq" | "x" | "xit" => Some(ExCommand::WriteQuit),
        "q" | "quit" => Some(ExCommand::Quit { force: false }),
        "q!" | "quit!" => Some(ExCommand::Quit { force: true }),
        "noh" | "nohl" | "nohlsearch" => Some(ExCommand::NoHighlight),
        "d" | "delete" => Some(ExCommand::Delete(range)),
        "y" | "yank" => Some(ExCommand::Yank(range)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_commands() {
        assert_eq!(parse("w"), Some(ExCommand::Write));
        assert_eq!(parse("wq"), Some(ExCommand::WriteQuit));
        assert_eq!(parse("q"), Some(ExCommand::Quit { force: false }));
        assert_eq!(parse("q!"), Some(ExCommand::Quit { force: true }));
        assert_eq!(parse("noh"), Some(ExCommand::NoHighlight));
        assert_eq!(parse("bogus"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn a_bare_number_is_a_goto() {
        assert_eq!(parse("42"), Some(ExCommand::Goto(41)));
        assert_eq!(parse("1"), Some(ExCommand::Goto(0)));
    }

    #[test]
    fn substitute_parses_pattern_replacement_and_flags() {
        assert_eq!(
            parse("%s/foo/bar/g"),
            Some(ExCommand::Substitute {
                range: ExRange::WholeFile,
                pattern: "foo".into(),
                replacement: "bar".into(),
                all: true,
            })
        );
        assert_eq!(
            parse("s/foo/bar"),
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
            parse("2,5s/a/b/"),
            Some(ExCommand::Substitute {
                range: ExRange::Lines(1, 4),
                pattern: "a".into(),
                replacement: "b".into(),
                all: false,
            })
        );
        assert_eq!(
            parse("'<,'>s/a/b/"),
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
            parse("s/a\\/b/c/"),
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
            parse("s#a#b#g"),
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
        assert_eq!(parse("%d"), Some(ExCommand::Delete(ExRange::WholeFile)));
        assert_eq!(parse("3,4y"), Some(ExCommand::Yank(ExRange::Lines(2, 3))));
    }

    #[test]
    fn a_word_starting_with_s_is_not_a_substitute() {
        assert_eq!(parse("sort"), None);
    }
}
