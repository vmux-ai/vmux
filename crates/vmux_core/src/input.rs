//! One keystroke, in the form every page ships and the core reads.
//!
//! A page encodes a key press into [`KeyStroke`] and sends it; nothing about what the key *means*
//! is decided there. Keymap lookup happens once, on the Bevy side, against this one shape — which
//! is what lets a binding be remapped in settings without a page knowing it changed.
//!
//! The other two types here are the rest of that seam. A page publishes [`PageKeyContext`] to say
//! what is true of it now, and the core answers with [`KeyClaims`]: the strokes that page must hand
//! over rather than let the browser have. A page tests membership; it never holds a keymap.
//!
//! This lives apart from the per-feature wire structs in [`crate::event`] on purpose: those are
//! owned by one surface each, and this is owned by none of them.

use serde::{Deserialize, Serialize};

/// Receives everything a page sends about its keyboard: [`KeyStroke`] and [`PageKeyContext`].
///
/// Added exactly once, here rather than by any consumer, because the registration is per *type*:
/// two plugins registering [`KeyStroke`] would each decode every keystroke and trigger it
/// separately, and the terminal would act on each press twice. Adding this plugin twice panics —
/// Bevy rejects a duplicate plugin — which is the failure we want, because the duplicate-input
/// version is silent. That guarantee only holds while every page-sent type is named in the one
/// tuple below: registering one in a second [`bevy_cef::prelude::BinEventEmitterPlugin`] elsewhere
/// is a *different* plugin type, so nothing would catch it.
///
/// [`KeyStrokePlugin::SENDERS`] is the whole allowlist. A page missing from it has its keys dropped
/// without an error, so a new page that sends keystrokes has to be added there.
#[cfg(not(web))]
pub struct KeyStrokePlugin;

#[cfg(not(web))]
impl KeyStrokePlugin {
    /// The page hosts permitted to send a [`KeyStroke`] or publish a [`PageKeyContext`].
    /// `start` is here because it is the same webview as `agent`: the launcher swaps itself for a
    /// chat page in place rather than navigating, so a prompt typed there is answered by a page
    /// whose host name never changed.
    pub const SENDERS: &'static [&'static str] = &[
        "terminal",
        "files",
        "command-bar",
        "layout",
        "agent",
        "start",
        "spaces",
    ];
}

#[cfg(not(web))]
impl bevy::prelude::Plugin for KeyStrokePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins(bevy_cef::prelude::BinEventEmitterPlugin::<(
            KeyStroke,
            PageKeyContext,
        )>::for_hosts(Self::SENDERS));
    }
}

/// The event id [`KeyClaims`] is pushed under.
///
/// Hand-picked and short, because the host->page direction matches on a constant rather than on a
/// type name the way the page->host direction does.
pub const KEY_CLAIMS_EVENT: &str = "key-claims";

/// What a page says is true of it now, so the core can tell which context-scoped bindings apply.
///
/// The whole set every time rather than a diff: a missed toggle would leave a key claimed by a
/// picker that has since closed, and the page cannot tell that its last message was lost.
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
pub struct PageKeyContext {
    pub keys: Vec<String>,
}

/// The strokes a page must hand over, as of its last published context.
///
/// Pushed on context change rather than consulted per keystroke, because a page cannot wait for an
/// answer: a printable key has to reach the `<textarea>` in the same tick. That makes the set
/// briefly stale after a context change, and the cost of staleness is what decides its membership
/// — see [`ClaimedKey`].
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
pub struct KeyClaims {
    pub keys: Vec<ClaimedKey>,
}

impl KeyClaims {
    /// Whether this stroke is one the core has claimed.
    ///
    /// Matched on `code` and the exact modifier set, the same pair the core built the claim from,
    /// so a layout that renames the character cannot change the answer.
    pub fn contains(&self, stroke: &KeyStroke) -> bool {
        self.keys
            .iter()
            .any(|claimed| claimed.code == stroke.code && claimed.mods == stroke.mods)
    }
}

/// Who gets a key press.
///
/// [`KeyVerdict::of`] is the whole page-side decision, and it lives here rather than in the UI
/// crate so that the rule and the set it reads are one file: the core decides what to claim by
/// asking whether a page could answer the stroke itself, and this is that same question from the
/// other side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyVerdict {
    /// Leave it alone: no `prevent_default`, nothing sent.
    Browser,
    /// Take it: `prevent_default`, then ship the stroke to the core.
    Send,
}

impl KeyVerdict {
    /// The decision for one stroke.
    ///
    /// `wanted_locally` is the page's own answer — is the caret free, is the editor in a mode that
    /// takes text — and it is asked first and wins outright. That is the part with no staleness, so
    /// letting it override the pushed set is what stops a set that is one context behind from
    /// swallowing a character, the one failure with no recovery.
    pub fn of(
        claims: &KeyClaims,
        unclaimed: Unclaimed,
        stroke: &KeyStroke,
        wanted_locally: bool,
    ) -> Self {
        if wanted_locally {
            return Self::Browser;
        }
        if claims.contains(stroke) {
            return Self::Send;
        }
        match unclaimed {
            Unclaimed::Types => Self::Browser,
            Unclaimed::Forwards => Self::Send,
        }
    }
}

/// What a page does with a key no binding claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unclaimed {
    /// The browser keeps it. A text surface, where a key nobody claimed is someone typing.
    Types,
    /// The page sends it anyway. A surface like the terminal, whose process downstream wants every
    /// key and where the browser has no business acting on any of them.
    Forwards,
}

/// One claimed stroke: the physical key and the modifiers that must be held with it.
///
/// Only strokes a page could not decide for itself are ever claimed — modifier-bearing combos and
/// the keys that type nothing. A printable key pressed bare is deliberately never here: the page
/// answers that one locally, and a stale set that claimed it would swallow a character, which is
/// the one failure with no recovery.
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
pub struct ClaimedKey {
    /// The `code` attribute: which physical key, regardless of layout.
    pub code: String,
    pub mods: KeyModifiers,
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

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: KeyModifiers = KeyModifiers {
        ctrl: true,
        shift: false,
        alt: false,
        super_key: false,
    };

    fn claims(codes: &[(&str, KeyModifiers)]) -> KeyClaims {
        let mut keys = Vec::new();
        for (code, mods) in codes {
            keys.push(ClaimedKey {
                code: (*code).to_string(),
                mods: *mods,
            });
        }
        KeyClaims { keys }
    }

    fn stroke(code: &str, mods: KeyModifiers) -> KeyStroke {
        KeyStroke {
            key: "x".to_string(),
            code: code.to_string(),
            mods,
            text: None,
            repeat: false,
        }
    }

    /// The point of the seam: a claimed stroke leaves the page, an unclaimed one does not, and a
    /// text surface keeps everything it was not asked for.
    #[test]
    fn a_text_surface_sends_only_what_was_claimed() {
        let claims = claims(&[("KeyX", CTRL)]);

        assert_eq!(
            KeyVerdict::of(&claims, Unclaimed::Types, &stroke("KeyX", CTRL), false),
            KeyVerdict::Send
        );
        assert_eq!(
            KeyVerdict::of(
                &claims,
                Unclaimed::Types,
                &stroke("KeyX", KeyModifiers::default()),
                false
            ),
            KeyVerdict::Browser
        );
    }

    /// The terminal's shape. Every key belongs to the process downstream, so a key nobody claimed
    /// is still the page's to forward — which is what makes migrating it a no-op.
    #[test]
    fn a_forwarding_surface_sends_what_nobody_claimed() {
        assert_eq!(
            KeyVerdict::of(
                &claims(&[]),
                Unclaimed::Forwards,
                &stroke("KeyX", KeyModifiers::default()),
                false
            ),
            KeyVerdict::Send
        );
    }

    /// The staleness guard. A set pushed before the caret moved must not take a key the page can
    /// see is text right now — and this has to hold for a forwarding surface too, or a page could
    /// never carve out an editable region inside one.
    #[test]
    fn what_the_page_wants_locally_beats_the_pushed_set() {
        let claims = claims(&[("KeyX", CTRL)]);

        for unclaimed in [Unclaimed::Types, Unclaimed::Forwards] {
            assert_eq!(
                KeyVerdict::of(&claims, unclaimed, &stroke("KeyX", CTRL), true),
                KeyVerdict::Browser
            );
        }
    }
}
