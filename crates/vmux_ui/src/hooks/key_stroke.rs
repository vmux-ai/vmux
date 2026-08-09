//! The one place a browser `keydown` becomes a [`KeyStroke`].
//!
//! Every page encodes through [`WebKey`], so a keystroke leaving any page looks the same to the
//! core. There used to be a copy of this per page, and they disagreed — one checked for IME
//! composition and one did not, so the same key was deliverable mid-composition on one page and
//! swallowed on another.
//!
//! Encoding is all this does. Whether a key is worth sending, and what it goes on to mean, is not
//! decided here.

use vmux_core::input::{KeyModifiers, KeyStroke};

/// A `keydown` as the browser delivered it, before anything has read meaning into it.
pub struct WebKey<'a>(&'a web_sys::KeyboardEvent);

impl<'a> WebKey<'a> {
    pub fn new(raw: &'a web_sys::KeyboardEvent) -> Self {
        Self(raw)
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
        let key = self.0.key();
        let text = (key.chars().count() == 1).then(|| key.clone());
        Some(KeyStroke {
            key,
            code: self.0.code(),
            mods: self.mods(),
            text,
            repeat: self.0.repeat(),
        })
    }

    fn mods(&self) -> KeyModifiers {
        KeyModifiers {
            ctrl: self.0.ctrl_key(),
            shift: self.0.shift_key(),
            alt: self.0.alt_key(),
            super_key: self.0.meta_key(),
        }
    }
}
