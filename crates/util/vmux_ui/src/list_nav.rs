use dioxus::prelude::{Code, KeyboardData, ModifiersInteraction};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuDirection {
    Next,
    Previous,
}

impl MenuDirection {
    pub fn from_key(key: &KeyboardData) -> Option<Self> {
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

    pub fn move_selection(self, current: usize, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        match self {
            Self::Next => (current + 1) % len,
            Self::Previous => (current + len - 1) % len,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::prelude::{Key, Location, Modifiers};

    struct Press {
        code: Code,
        modifiers: Modifiers,
    }

    impl Press {
        fn event(code: Code, modifiers: Modifiers) -> KeyboardData {
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
            (Code::KeyJ, none, None),
            (Code::KeyK, none, None),
            (Code::KeyN, none, None),
            (Code::ArrowDown, ctrl, None),
            (Code::ArrowUp, ctrl, None),
            (Code::ArrowDown, Modifiers::META, None),
            (Code::ArrowDown, Modifiers::ALT, None),
            (Code::KeyN, Modifiers::META, None),
            (Code::ArrowDown, Modifiers::SHIFT, None),
            (Code::KeyJ, ctrl | Modifiers::SHIFT, None),
            (Code::KeyN, ctrl | Modifiers::ALT, None),
        ];

        for (code, modifiers, expected) in table {
            assert_eq!(
                MenuDirection::from_key(&Press::event(code, modifiers)),
                expected,
                "{code:?} with {modifiers:?}"
            );
        }
    }

    #[test]
    fn a_selection_wraps_at_both_ends() {
        assert_eq!(MenuDirection::Next.move_selection(2, 3), 0);
        assert_eq!(MenuDirection::Previous.move_selection(0, 3), 2);
        assert_eq!(MenuDirection::Next.move_selection(0, 0), 0);
    }
}
