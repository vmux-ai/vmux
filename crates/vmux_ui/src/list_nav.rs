//! Moving a selection through a list.
//!
//! Every surface with a highlighted row answers the same two questions: did that key move the
//! selection, and where does the move land.
//!
//! Which surfaces ask *here* rather than through the keymap is decided by whether the surface has a
//! context of its own to publish. The command-bar modal, the chat page and the spaces page each
//! publish one and have their strokes resolved on the host, so their bindings live in
//! `settings.json` and none of this runs for them. A shared component has no page identity to
//! publish and would clobber its host page's context if it tried, and the start palette publishes
//! nothing on purpose because it shares a webview with the chat page. Those resolve the same chords
//! locally — and the point of this module is that "the same chords" is one table rather than one
//! per surface.

use dioxus::prelude::{Code, KeyboardData, ModifiersInteraction};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuDirection {
    Next,
    Previous,
}

impl MenuDirection {
    /// Which way a keystroke moves the highlighted row, or `None` when it is not navigation.
    ///
    /// The table is exactly the `command_bar_next` / `command_bar_previous` chord set, so a surface
    /// resolving its own keys and one going through the keymap cannot disagree about what `Ctrl+j`
    /// means. Read from `code` rather than `key` for the same reason the keymap is: `Ctrl+n` yields
    /// no printable character on macOS, and on a non-QWERTY layout `key` names a different physical
    /// key than the one under the user's finger.
    ///
    /// Every other modifier combination belongs to somebody else. `Ctrl+ArrowDown` and
    /// `Cmd+ArrowDown` are window-manager and text-editing chords on the platforms we ship, a
    /// shifted chord is not the one that was bound, and a bare `j` is a character being typed.
    pub fn of(key: &KeyboardData) -> Option<Self> {
        let modifiers = key.modifiers();
        if modifiers.meta() || modifiers.alt() || modifiers.shift() {
            return None;
        }
        match (key.code(), modifiers.ctrl()) {
            (Code::ArrowDown, false) => Some(Self::Next),
            (Code::ArrowUp, false) => Some(Self::Previous),
            (Code::KeyN | Code::KeyJ, true) => Some(Self::Next),
            (Code::KeyP | Code::KeyK, true) => Some(Self::Previous),
            _ => None,
        }
    }
}

/// A 1-based number key naming one of `len` rows.
pub fn choice_number_index(key: &str, len: usize) -> Option<usize> {
    let number = key.parse::<usize>().ok()?;
    if number == 0 || number > len {
        None
    } else {
        Some(number - 1)
    }
}

/// Wraps at both ends, so holding Down cycles rather than sticking at the bottom.
pub fn move_selection(current: usize, len: usize, direction: MenuDirection) -> usize {
    if len == 0 {
        return 0;
    }
    match direction {
        MenuDirection::Next => (current + 1) % len,
        MenuDirection::Previous => (current + len - 1) % len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::prelude::{Key, Location, Modifiers};

    /// One keystroke, built the way the DOM would hand it over.
    struct Press {
        code: Code,
        modifiers: Modifiers,
    }

    impl Press {
        fn of(code: Code, modifiers: Modifiers) -> KeyboardData {
            KeyboardData::new(Self { code, modifiers })
        }
    }

    impl ModifiersInteraction for Press {
        fn modifiers(&self) -> Modifiers {
            self.modifiers
        }
    }

    impl dioxus::prelude::HasKeyboardData for Press {
        fn key(&self) -> Key {
            Key::Unidentified
        }

        fn code(&self) -> Code {
            self.code
        }

        fn location(&self) -> Location {
            Location::Standard
        }

        fn is_auto_repeating(&self) -> bool {
            false
        }

        fn is_composing(&self) -> bool {
            false
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    /// The contract four surfaces had drifted apart on, as the whole table rather than a sample.
    ///
    /// The oracle is the keymap: these are precisely the chords `command_bar_next` and
    /// `command_bar_previous` are bound to by default, so a page that resolves navigation locally
    /// behaves like one that hands its keys to the host.
    #[test]
    fn one_chord_set_moves_a_selection_everywhere() {
        let none = Modifiers::empty();
        let ctrl = Modifiers::CONTROL;
        let table = [
            (Code::ArrowDown, none, Some(MenuDirection::Next)),
            (Code::ArrowUp, none, Some(MenuDirection::Previous)),
            (Code::KeyN, ctrl, Some(MenuDirection::Next)),
            (Code::KeyJ, ctrl, Some(MenuDirection::Next)),
            (Code::KeyP, ctrl, Some(MenuDirection::Previous)),
            (Code::KeyK, ctrl, Some(MenuDirection::Previous)),
            // A bare letter is a character, not a move. The spaces page used to navigate on these.
            (Code::KeyJ, none, None),
            (Code::KeyK, none, None),
            (Code::KeyN, none, None),
            // Ctrl on an arrow is a window-manager chord. The manager select used to navigate here.
            (Code::ArrowDown, ctrl, None),
            (Code::ArrowUp, ctrl, None),
            // Neither Meta nor Alt carries a list binding on any surface.
            (Code::ArrowDown, Modifiers::META, None),
            (Code::ArrowDown, Modifiers::ALT, None),
            (Code::KeyN, Modifiers::META, None),
            // A shifted chord is not the chord that was bound.
            (Code::ArrowDown, Modifiers::SHIFT, None),
            (Code::KeyJ, ctrl | Modifiers::SHIFT, None),
            (Code::KeyN, ctrl | Modifiers::ALT, None),
        ];

        for (code, modifiers, expected) in table {
            assert_eq!(
                MenuDirection::of(&Press::of(code, modifiers)),
                expected,
                "{code:?} with {modifiers:?}"
            );
        }
    }

    #[test]
    fn a_selection_wraps_at_both_ends() {
        assert_eq!(move_selection(2, 3, MenuDirection::Next), 0);
        assert_eq!(move_selection(0, 3, MenuDirection::Previous), 2);
        assert_eq!(move_selection(0, 0, MenuDirection::Next), 0);
    }

    #[test]
    fn a_number_key_names_a_row_only_when_the_row_exists() {
        assert_eq!(choice_number_index("2", 3), Some(1));
        assert_eq!(choice_number_index("4", 3), None);
        assert_eq!(choice_number_index("0", 3), None);
        assert_eq!(choice_number_index("Enter", 3), None);
    }
}
