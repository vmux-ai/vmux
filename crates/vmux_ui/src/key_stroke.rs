use dioxus::prelude::KeyboardData;
use dioxus::prelude::ModifiersInteraction;
use dioxus::prelude::keyboard_types::Modifiers;
use vmux_core::input::{KeyModifiers, KeyStroke};

pub struct PressedKey<'a>(&'a KeyboardData);

impl<'a> PressedKey<'a> {
    pub fn new(data: &'a KeyboardData) -> Self {
        Self(data)
    }

    pub fn stroke(&self) -> Option<KeyStroke> {
        if self.0.is_composing() {
            return None;
        }

        let key = self.0.key().to_string();
        let text = (key.chars().count() == 1).then(|| key.clone());

        Some(KeyStroke {
            key,
            code: self.0.code().to_string(),
            mods: self.mods(),
            text,
            repeat: self.0.is_auto_repeating(),
        })
    }

    fn mods(&self) -> KeyModifiers {
        let modifiers = self.0.modifiers();

        KeyModifiers {
            ctrl: modifiers.contains(Modifiers::CONTROL),
            shift: modifiers.contains(Modifiers::SHIFT),
            alt: modifiers.contains(Modifiers::ALT),
            super_key: modifiers.contains(Modifiers::META),
        }
    }
}
