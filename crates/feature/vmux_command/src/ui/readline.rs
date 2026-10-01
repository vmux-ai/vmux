use vmux_ui::caret::floor_char_boundary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextEditCommand {
    Home,
    End,
    Forward,
    Back,
    Delete,
    Backspace,
    DeleteWord,
    DeleteToBeginning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edited {
    pub(crate) value: String,
    pub(crate) caret: usize,
}

impl TextEditCommand {
    pub fn from_command(command: &str) -> Option<Self> {
        match command {
            "command_bar_home" => Some(Self::Home),
            "command_bar_end" => Some(Self::End),
            "command_bar_forward" => Some(Self::Forward),
            "command_bar_back" => Some(Self::Back),
            "command_bar_delete" => Some(Self::Delete),
            "command_bar_backspace" => Some(Self::Backspace),
            "command_bar_delete_word" => Some(Self::DeleteWord),
            "command_bar_delete_to_beginning" => Some(Self::DeleteToBeginning),
            _ => None,
        }
    }

    pub fn apply(self, value: &str, caret: usize, ghost: &str) -> Edited {
        let caret = floor_char_boundary(value, caret);
        let kept = |caret| Edited {
            value: value.to_string(),
            caret,
        };
        let next = || {
            value[caret..]
                .chars()
                .next()
                .map_or(caret, |c| caret + c.len_utf8())
        };
        let prev = || {
            value[..caret]
                .chars()
                .next_back()
                .map_or(0, |c| caret - c.len_utf8())
        };
        let cut = |start: usize, end: usize| Edited {
            value: format!("{}{}", &value[..start], &value[end..]),
            caret: start,
        };

        match self {
            Self::Home => kept(0),
            Self::End if ghost.is_empty() => kept(value.len()),
            Self::End => {
                let value = format!("{value}{ghost}");
                Edited {
                    caret: value.len(),
                    value,
                }
            }
            Self::Forward => kept(next()),
            Self::Back => kept(prev()),
            Self::Delete => cut(caret, next()),
            Self::Backspace => cut(prev(), caret),
            Self::DeleteWord => {
                let bytes = value.as_bytes();
                let mut start = caret.saturating_sub(1);
                while start > 0 && bytes[start - 1] == b' ' {
                    start -= 1;
                }
                while start > 0 && bytes[start - 1] != b' ' {
                    start -= 1;
                }
                cut(floor_char_boundary(value, start), caret)
            }
            Self::DeleteToBeginning => Edited {
                value: value[caret..].to_string(),
                caret: 0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readline_commands_map_to_text_edits() {
        let edits = [
            ("command_bar_home", TextEditCommand::Home),
            ("command_bar_end", TextEditCommand::End),
            ("command_bar_forward", TextEditCommand::Forward),
            ("command_bar_back", TextEditCommand::Back),
            ("command_bar_delete", TextEditCommand::Delete),
            ("command_bar_backspace", TextEditCommand::Backspace),
            ("command_bar_delete_word", TextEditCommand::DeleteWord),
            (
                "command_bar_delete_to_beginning",
                TextEditCommand::DeleteToBeginning,
            ),
        ];
        for (id, command) in edits {
            assert_eq!(TextEditCommand::from_command(id), Some(command), "{id}");
        }

        for id in ["command_bar_next", "command_bar_submit", ""] {
            assert_eq!(TextEditCommand::from_command(id), None, "{id}");
        }
    }

    #[test]
    fn ctrl_edits_move_and_cut_as_readline_does() {
        let cases = [
            (TextEditCommand::Home, "foo bar", 7, "foo bar", 0),
            (TextEditCommand::End, "foo bar", 0, "foo bar", 7),
            (TextEditCommand::Forward, "foo bar", 3, "foo bar", 4),
            (TextEditCommand::Back, "foo bar", 3, "foo bar", 2),
            (TextEditCommand::Delete, "foo bar", 3, "foobar", 3),
            (TextEditCommand::Backspace, "foo bar", 4, "foobar", 3),
            (TextEditCommand::DeleteWord, "foo bar", 7, "foo ", 4),
            (TextEditCommand::DeleteWord, "foo bar ", 8, "foo ", 4),
            (TextEditCommand::DeleteToBeginning, "foo bar", 4, "bar", 0),
            (TextEditCommand::DeleteToBeginning, "foo bar", 7, "", 0),
        ];
        for (command, value, caret, want_value, want_caret) in cases {
            let got = command.apply(value, caret, "");
            assert_eq!(
                got,
                Edited {
                    value: want_value.to_string(),
                    caret: want_caret
                },
                "{command:?} on {value:?} at {caret}"
            );
        }
    }

    #[test]
    fn edits_at_the_ends_of_the_text_change_nothing() {
        let at_start = TextEditCommand::Backspace.apply("foo", 0, "");
        assert_eq!(at_start.value, "foo");
        assert_eq!(at_start.caret, 0);

        let at_end = TextEditCommand::Delete.apply("foo", 3, "");
        assert_eq!(at_end.value, "foo");
        assert_eq!(at_end.caret, 3);

        assert_eq!(TextEditCommand::Back.apply("foo", 0, "").caret, 0);
        assert_eq!(TextEditCommand::Forward.apply("foo", 3, "").caret, 3);
    }

    #[test]
    fn edits_next_to_multibyte_characters_act_on_the_right_one() {
        let s = "aé本b";
        assert_eq!(TextEditCommand::Delete.apply(s, 6, "").value, "aé本");
        assert_eq!(TextEditCommand::Delete.apply(s, 3, "").value, "aéb");
        assert_eq!(TextEditCommand::Backspace.apply(s, 6, "").value, "aéb");

        let back = TextEditCommand::Back.apply(s, 6, "");
        assert_eq!(back.caret, 3, "one character back, not one byte");
        assert_eq!(TextEditCommand::Forward.apply(s, 3, "").caret, 6);
    }

    #[test]
    fn a_caret_inside_a_character_is_pulled_to_its_start() {
        let inside = TextEditCommand::Delete.apply("aé本b", 4, "");
        assert_eq!(inside.value, "aéb");
        assert_eq!(inside.caret, 3);
    }

    #[test]
    fn ctrl_e_accepts_the_inline_completion_and_lands_past_it() {
        let accepted = TextEditCommand::End.apply("git.co", 6, "m/vmux");
        assert_eq!(accepted.value, "git.com/vmux");
        assert_eq!(accepted.caret, 12);

        assert_eq!(
            TextEditCommand::Home.apply("git.co", 6, "m/vmux").value,
            "git.co"
        );
    }
}
