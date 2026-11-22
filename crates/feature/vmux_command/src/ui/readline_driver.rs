#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edited {
    pub(crate) value: String,
    pub(crate) caret: usize,
}

impl Edited {
    pub(crate) fn handles(command: &str) -> bool {
        matches!(
            command,
            "command_bar_home"
                | "command_bar_end"
                | "command_bar_forward"
                | "command_bar_back"
                | "command_bar_delete"
                | "command_bar_backspace"
                | "command_bar_delete_word"
                | "command_bar_delete_to_beginning"
        )
    }

    pub(crate) fn apply_command(
        command: &str,
        value: &str,
        caret: usize,
        ghost: &str,
    ) -> Option<Self> {
        let caret = Self::floor_boundary(value, caret);
        let kept = |caret| Self {
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
        let cut = |start: usize, end: usize| Self {
            value: format!("{}{}", &value[..start], &value[end..]),
            caret: start,
        };

        let edited = match command {
            "command_bar_home" => kept(0),
            "command_bar_end" if ghost.is_empty() => kept(value.len()),
            "command_bar_end" => {
                let value = format!("{value}{ghost}");
                Self {
                    caret: value.len(),
                    value,
                }
            }
            "command_bar_forward" => kept(next()),
            "command_bar_back" => kept(prev()),
            "command_bar_delete" => cut(caret, next()),
            "command_bar_backspace" => cut(prev(), caret),
            "command_bar_delete_word" => {
                let bytes = value.as_bytes();
                let mut start = caret.saturating_sub(1);
                while start > 0 && bytes[start - 1] == b' ' {
                    start -= 1;
                }
                while start > 0 && bytes[start - 1] != b' ' {
                    start -= 1;
                }
                cut(Self::floor_boundary(value, start), caret)
            }
            "command_bar_delete_to_beginning" => Self {
                value: value[caret..].to_string(),
                caret: 0,
            },
            _ => return None,
        };
        Some(edited)
    }

    fn floor_boundary(value: &str, mut byte: usize) -> usize {
        if byte >= value.len() {
            return value.len();
        }
        while byte > 0 && !value.is_char_boundary(byte) {
            byte -= 1;
        }
        byte
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readline_commands_map_to_text_edits() {
        for id in [
            "command_bar_home",
            "command_bar_end",
            "command_bar_forward",
            "command_bar_back",
            "command_bar_delete",
            "command_bar_backspace",
            "command_bar_delete_word",
            "command_bar_delete_to_beginning",
        ] {
            assert!(Edited::handles(id), "{id}");
            assert!(Edited::apply_command(id, "foo", 1, "").is_some(), "{id}");
        }

        for id in ["command_bar_next", "command_bar_submit", ""] {
            assert!(!Edited::handles(id), "{id}");
            assert_eq!(Edited::apply_command(id, "foo", 1, ""), None, "{id}");
        }
    }

    #[test]
    fn ctrl_edits_move_and_cut_as_readline_does() {
        let cases = [
            ("command_bar_home", "foo bar", 7, "foo bar", 0),
            ("command_bar_end", "foo bar", 0, "foo bar", 7),
            ("command_bar_forward", "foo bar", 3, "foo bar", 4),
            ("command_bar_back", "foo bar", 3, "foo bar", 2),
            ("command_bar_delete", "foo bar", 3, "foobar", 3),
            ("command_bar_backspace", "foo bar", 4, "foobar", 3),
            ("command_bar_delete_word", "foo bar", 7, "foo ", 4),
            ("command_bar_delete_word", "foo bar ", 8, "foo ", 4),
            ("command_bar_delete_to_beginning", "foo bar", 4, "bar", 0),
            ("command_bar_delete_to_beginning", "foo bar", 7, "", 0),
        ];
        for (command, value, caret, want_value, want_caret) in cases {
            let got = Edited::apply_command(command, value, caret, "").unwrap();
            assert_eq!(
                got,
                Edited {
                    value: want_value.to_string(),
                    caret: want_caret
                },
                "{command} on {value:?} at {caret}"
            );
        }
    }

    #[test]
    fn edits_at_the_ends_of_the_text_change_nothing() {
        let at_start = Edited::apply_command("command_bar_backspace", "foo", 0, "").unwrap();
        assert_eq!(at_start.value, "foo");
        assert_eq!(at_start.caret, 0);

        let at_end = Edited::apply_command("command_bar_delete", "foo", 3, "").unwrap();
        assert_eq!(at_end.value, "foo");
        assert_eq!(at_end.caret, 3);

        assert_eq!(
            Edited::apply_command("command_bar_back", "foo", 0, "")
                .unwrap()
                .caret,
            0
        );
        assert_eq!(
            Edited::apply_command("command_bar_forward", "foo", 3, "")
                .unwrap()
                .caret,
            3
        );
    }

    #[test]
    fn edits_next_to_multibyte_characters_act_on_the_right_one() {
        let s = "aé本b";
        assert_eq!(
            Edited::apply_command("command_bar_delete", s, 6, "")
                .unwrap()
                .value,
            "aé本"
        );
        assert_eq!(
            Edited::apply_command("command_bar_delete", s, 3, "")
                .unwrap()
                .value,
            "aéb"
        );
        assert_eq!(
            Edited::apply_command("command_bar_backspace", s, 6, "")
                .unwrap()
                .value,
            "aéb"
        );

        let back = Edited::apply_command("command_bar_back", s, 6, "").unwrap();
        assert_eq!(back.caret, 3, "one character back, not one byte");
        assert_eq!(
            Edited::apply_command("command_bar_forward", s, 3, "")
                .unwrap()
                .caret,
            6
        );
    }

    #[test]
    fn a_caret_inside_a_character_is_pulled_to_its_start() {
        let inside = Edited::apply_command("command_bar_delete", "aé本b", 4, "").unwrap();
        assert_eq!(inside.value, "aéb");
        assert_eq!(inside.caret, 3);
    }

    #[test]
    fn ctrl_e_accepts_the_inline_completion_and_lands_past_it() {
        let accepted = Edited::apply_command("command_bar_end", "git.co", 6, "m/vmux").unwrap();
        assert_eq!(accepted.value, "git.com/vmux");
        assert_eq!(accepted.caret, 12);

        assert_eq!(
            Edited::apply_command("command_bar_home", "git.co", 6, "m/vmux")
                .unwrap()
                .value,
            "git.co"
        );
    }
}
