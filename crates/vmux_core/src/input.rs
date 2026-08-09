//! One keystroke, in the form every page ships and the core reads.
//!
//! A page encodes a key press into [`KeyStroke`] and sends it; nothing about what the key *means*
//! is decided there. Keymap lookup happens once, on the Bevy side, against this one shape — which
//! is what lets a binding be remapped in settings without a page knowing it changed.
//!
//! This lives apart from the per-feature wire structs in [`crate::event`] on purpose: those are
//! owned by one surface each, and this is owned by none of them.

use serde::{Deserialize, Serialize};

/// Receives [`KeyStroke`] from the pages allowed to send one.
///
/// Added exactly once, here rather than by either consumer, because the registration is per *type*:
/// two plugins registering [`KeyStroke`] would each decode every keystroke and trigger it
/// separately, and the terminal would act on each press twice. Adding this twice panics, which is
/// the failure we want — the duplicate-input version is silent.
///
/// [`KeyStrokePlugin::SENDERS`] is the whole allowlist. A page missing from it has its keys dropped
/// without an error, so a new page that sends keystrokes has to be added there.
#[cfg(not(web))]
pub struct KeyStrokePlugin;

#[cfg(not(web))]
impl KeyStrokePlugin {
    /// The page hosts permitted to send a [`KeyStroke`].
    pub const SENDERS: &'static [&'static str] = &["terminal", "files"];
}

#[cfg(not(web))]
impl bevy::prelude::Plugin for KeyStrokePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins(bevy_cef::prelude::BinEventEmitterPlugin::<(
            KeyStroke,
        )>::for_hosts(Self::SENDERS));
    }
}

/// The modifiers held during a keystroke.
///
/// Named fields rather than a bitmask because the only thing the core does with these is compare
/// them against `vmux_command`'s `Modifiers`, which is also named-field — so the conversion is a
/// copy, with no constant left to pair up wrongly. The `MOD_*` bitmask in [`crate::event`] stays
/// where it earns its keep: mouse reporting does arithmetic on those bits to build an SGR sequence.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}

impl KeyModifiers {
    /// True when a held modifier turns a printable key into a command rather than into text.
    ///
    /// Shift is excluded: it selects which character the key produces, so `Shift+a` is still
    /// someone typing.
    pub fn has_chord(&self) -> bool {
        self.ctrl || self.alt || self.super_key
    }
}

/// One key press, as the page saw it.
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct KeyStroke {
    /// The `key` attribute: what the keypress produces, after layout and modifiers.
    pub key: String,
    /// The `code` attribute: which physical key, regardless of layout.
    #[serde(default)]
    pub code: String,
    pub mods: KeyModifiers,
    /// The character this keypress types, when it types one.
    pub text: Option<String>,
    /// True when the key is repeating because it is being held down.
    #[serde(default)]
    pub repeat: bool,
}

impl KeyStroke {
    /// True when this is a modifier being pressed on its own.
    ///
    /// Such a press carries no key to act on, and acting on one would fire a binding as soon as its
    /// modifier went down rather than when the key completing it did. Checked against both `key`
    /// and `code`, because a page that reports one may not report the other.
    pub fn is_modifier_key(&self) -> bool {
        matches!(
            self.key.as_str(),
            "Shift" | "Control" | "Alt" | "Meta" | "OS" | "Fn" | "CapsLock"
        ) || matches!(
            self.code.as_str(),
            "ShiftLeft"
                | "ShiftRight"
                | "ControlLeft"
                | "ControlRight"
                | "AltLeft"
                | "AltRight"
                | "MetaLeft"
                | "MetaRight"
                | "OSLeft"
                | "OSRight"
                | "CapsLock"
        )
    }

    /// True when this keypress is someone typing a character rather than invoking something.
    pub fn is_text_input(&self) -> bool {
        !self.mods.has_chord() && self.key.chars().count() == 1
    }

    /// The character this keypress types, falling back to `key` when no text was captured.
    pub fn typed_text(&self) -> &str {
        self.text.as_deref().unwrap_or(self.key.as_str())
    }
}
