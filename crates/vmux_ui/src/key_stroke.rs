//! The one place a `keydown` becomes a [`KeyStroke`].
//!
//! Every page encodes through [`PressedKey`], so a keystroke leaving any page looks the same to
//! the core. There used to be a copy of this per page, and they disagreed — one checked for IME
//! composition and one did not, so the same key was deliverable mid-composition on one page and
//! swallowed on another.
//!
//! Encoding is all this does. Whether a key is worth sending, and what it goes on to mean, is not
//! decided here.

use dioxus::prelude::KeyboardData;
use dioxus::prelude::ModifiersInteraction;
use dioxus::prelude::keyboard_types::Modifiers;
use vmux_core::input::{KeyModifiers, KeyStroke};

/// A `keydown` as Dioxus delivered it, before anything has read meaning into it.
///
/// Reads Dioxus's own event data rather than a `web_sys::KeyboardEvent`. The downcast to a
/// platform event answers `None` off the web, and `on_keydown` returned early on that — so a page
/// running its components outside a browser had no keyboard at all, silently and with nothing to
/// see in a build log.
pub struct PressedKey<'a>(&'a KeyboardData);

impl<'a> PressedKey<'a> {
    pub fn new(data: &'a KeyboardData) -> Self {
        Self(data)
    }

    /// The keystroke to send, or `None` while an IME is composing.
    ///
    /// A composing keypress belongs to the input method, not to us: the browser is still turning it
    /// into a character and will deliver the result as text of its own. Acting on the raw key would
    /// fire a binding on a keystroke the user meant as part of a word.
    ///
    /// Modifier-only presses are *not* filtered here — [`KeyStroke::is_modifier_key`] is left to
    /// the caller, because whether a bare modifier is worth sending is the surface's policy rather
    /// than a fact about the event.
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
