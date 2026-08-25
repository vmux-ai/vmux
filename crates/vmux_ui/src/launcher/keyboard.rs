use crate::caret::floor_char_boundary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtrlEditAction {
    Home,
    End,
    Forward,
    Back,
    Delete,
    Backspace,
    DeleteWord,
    DeleteToBeginning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtrlKeyCapture {
    Ignore,
    Edit(CtrlEditAction),
    PassToDioxus,
}

pub fn ctrl_key_capture_for_code(code: &str) -> CtrlKeyCapture {
    match code {
        "KeyA" => CtrlKeyCapture::Edit(CtrlEditAction::Home),
        "KeyE" => CtrlKeyCapture::Edit(CtrlEditAction::End),
        "KeyF" => CtrlKeyCapture::Edit(CtrlEditAction::Forward),
        "KeyB" => CtrlKeyCapture::Edit(CtrlEditAction::Back),
        "KeyD" => CtrlKeyCapture::Edit(CtrlEditAction::Delete),
        "KeyH" => CtrlKeyCapture::Edit(CtrlEditAction::Backspace),
        "KeyW" => CtrlKeyCapture::Edit(CtrlEditAction::DeleteWord),
        "KeyU" => CtrlKeyCapture::Edit(CtrlEditAction::DeleteToBeginning),
        "KeyC" | "KeyJ" | "KeyK" | "KeyN" | "KeyP" => CtrlKeyCapture::PassToDioxus,
        _ => CtrlKeyCapture::Ignore,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub value: String,
    pub caret: usize,
}

impl CtrlEditAction {
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
            Self::DeleteWord => cut(word_start_before(value, caret), caret),
            Self::DeleteToBeginning => Edited {
                value: value[caret..].to_string(),
                caret: 0,
            },
        }
    }
}

fn word_start_before(value: &str, caret: usize) -> usize {
    let bytes = value.as_bytes();
    let mut i = caret.saturating_sub(1);
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    while i > 0 && bytes[i - 1] != b' ' {
        i -= 1;
    }
    floor_char_boundary(value, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ctrl_chord_is_edited_passed_on_or_ignored() {
        let edits = [
            ("KeyA", CtrlEditAction::Home),
            ("KeyE", CtrlEditAction::End),
            ("KeyF", CtrlEditAction::Forward),
            ("KeyB", CtrlEditAction::Back),
            ("KeyD", CtrlEditAction::Delete),
            ("KeyH", CtrlEditAction::Backspace),
            ("KeyW", CtrlEditAction::DeleteWord),
            ("KeyU", CtrlEditAction::DeleteToBeginning),
        ];
        for (code, action) in edits {
            assert_eq!(
                ctrl_key_capture_for_code(code),
                CtrlKeyCapture::Edit(action),
                "{code}"
            );
        }

        for code in ["KeyC", "KeyJ", "KeyK", "KeyN", "KeyP"] {
            assert_eq!(
                ctrl_key_capture_for_code(code),
                CtrlKeyCapture::PassToDioxus,
                "{code}"
            );
        }

        for code in ["KeyG", "KeyZ", "Enter", "Tab", ""] {
            assert_eq!(
                ctrl_key_capture_for_code(code),
                CtrlKeyCapture::Ignore,
                "{code}"
            );
        }
    }

    #[test]
    fn ctrl_edits_move_and_cut_as_readline_does() {
        let cases = [
            (CtrlEditAction::Home, "foo bar", 7, "foo bar", 0),
            (CtrlEditAction::End, "foo bar", 0, "foo bar", 7),
            (CtrlEditAction::Forward, "foo bar", 3, "foo bar", 4),
            (CtrlEditAction::Back, "foo bar", 3, "foo bar", 2),
            (CtrlEditAction::Delete, "foo bar", 3, "foobar", 3),
            (CtrlEditAction::Backspace, "foo bar", 4, "foobar", 3),
            (CtrlEditAction::DeleteWord, "foo bar", 7, "foo ", 4),
            (CtrlEditAction::DeleteWord, "foo bar ", 8, "foo ", 4),
            (CtrlEditAction::DeleteToBeginning, "foo bar", 4, "bar", 0),
            (CtrlEditAction::DeleteToBeginning, "foo bar", 7, "", 0),
        ];
        for (action, value, caret, want_value, want_caret) in cases {
            let got = action.apply(value, caret, "");
            assert_eq!(
                got,
                Edited {
                    value: want_value.to_string(),
                    caret: want_caret
                },
                "{action:?} on {value:?} at {caret}"
            );
        }
    }

    #[test]
    fn edits_at_the_ends_of_the_text_change_nothing() {
        let at_start = CtrlEditAction::Backspace.apply("foo", 0, "");
        assert_eq!(at_start.value, "foo");
        assert_eq!(at_start.caret, 0);

        let at_end = CtrlEditAction::Delete.apply("foo", 3, "");
        assert_eq!(at_end.value, "foo");
        assert_eq!(at_end.caret, 3);

        assert_eq!(CtrlEditAction::Back.apply("foo", 0, "").caret, 0);
        assert_eq!(CtrlEditAction::Forward.apply("foo", 3, "").caret, 3);
    }

    #[test]
    fn edits_next_to_multibyte_characters_act_on_the_right_one() {
        let s = "aé本b";
        assert_eq!(CtrlEditAction::Delete.apply(s, 6, "").value, "aé本");
        assert_eq!(CtrlEditAction::Delete.apply(s, 3, "").value, "aéb");
        assert_eq!(CtrlEditAction::Backspace.apply(s, 6, "").value, "aéb");

        let back = CtrlEditAction::Back.apply(s, 6, "");
        assert_eq!(back.caret, 3, "one character back, not one byte");
        assert_eq!(CtrlEditAction::Forward.apply(s, 3, "").caret, 6);
    }

    #[test]
    fn a_caret_inside_a_character_is_pulled_to_its_start() {
        let inside = CtrlEditAction::Delete.apply("aé本b", 4, "");
        assert_eq!(inside.value, "aéb");
        assert_eq!(inside.caret, 3);
    }

    #[test]
    fn ctrl_e_accepts_the_inline_completion_and_lands_past_it() {
        let accepted = CtrlEditAction::End.apply("git.co", 6, "m/vmux");
        assert_eq!(accepted.value, "git.com/vmux");
        assert_eq!(accepted.caret, 12);

        assert_eq!(
            CtrlEditAction::Home.apply("git.co", 6, "m/vmux").value,
            "git.co"
        );
    }
}
