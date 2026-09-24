#[cfg(host)]
pub struct KeyStrokePlugin;

#[cfg(host)]
impl KeyStrokePlugin {
    pub const SENDERS: &'static [&'static str] = &[
        "terminal",
        "files",
        "projects",
        "knowledge",
        "command-bar",
        "layout",
        "agent",
        "start",
        "spaces",
    ];
}

#[cfg(host)]
impl bevy::prelude::Plugin for KeyStrokePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins(bevy_cef::prelude::UiEventPlugin::<(
            KeyStroke,
            PageKeyContext,
        )>::default());
    }
}

#[vmux_api::ui_event(Default, Eq, targets = [
        "terminal",
        "files",
        "projects",
        "knowledge",
        "command-bar",
        "layout",
        "agent",
        "start",
        "spaces",
    ])]
pub struct PageKeyContext {
    pub keys: Vec<String>,
}

#[vmux_api::ui_state(Default, Eq, targets = [
    "terminal",
    "files",
    "projects",
    "knowledge",
    "command-bar",
    "layout",
    "agent",
    "start",
    "spaces",
])]
pub struct KeyClaims {
    pub keys: Vec<ClaimedKey>,
}

impl KeyClaims {
    pub fn contains(&self, stroke: &KeyStroke) -> bool {
        self.keys
            .iter()
            .any(|claimed| claimed.code == stroke.code && claimed.mods == stroke.mods)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyVerdict {
    Browser,
    Send,
}

impl KeyVerdict {
    pub fn decide(
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unclaimed {
    Types,
    Forwards,
}

#[vmux_api::contract(Default, Eq)]
pub struct ClaimedKey {
    pub code: String,
    pub mods: KeyModifiers,
}

#[vmux_api::contract(Copy, Default, Eq, Hash)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}

impl KeyModifiers {
    pub fn has_chord(&self) -> bool {
        self.ctrl || self.alt || self.super_key
    }
}

#[vmux_api::ui_event(Default, Eq, targets = [
        "terminal",
        "files",
        "projects",
        "knowledge",
        "command-bar",
        "layout",
        "agent",
        "start",
        "spaces",
    ])]
pub struct KeyStroke {
    pub key: String,
    #[serde(default)]
    pub code: String,
    pub mods: KeyModifiers,
    pub text: Option<String>,
    #[serde(default)]
    pub repeat: bool,
}

impl KeyStroke {
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

    pub fn is_text_input(&self) -> bool {
        !self.mods.has_chord() && self.key.chars().count() == 1
    }

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

    #[test]
    fn a_text_surface_sends_only_what_was_claimed() {
        let claims = claims(&[("KeyX", CTRL)]);

        assert_eq!(
            KeyVerdict::decide(&claims, Unclaimed::Types, &stroke("KeyX", CTRL), false),
            KeyVerdict::Send
        );
        assert_eq!(
            KeyVerdict::decide(
                &claims,
                Unclaimed::Types,
                &stroke("KeyX", KeyModifiers::default()),
                false
            ),
            KeyVerdict::Browser
        );
    }

    #[test]
    fn a_forwarding_surface_sends_what_nobody_claimed() {
        assert_eq!(
            KeyVerdict::decide(
                &claims(&[]),
                Unclaimed::Forwards,
                &stroke("KeyX", KeyModifiers::default()),
                false
            ),
            KeyVerdict::Send
        );
    }

    #[test]
    fn what_the_page_wants_locally_beats_the_pushed_set() {
        let claims = claims(&[("KeyX", CTRL)]);

        for unclaimed in [Unclaimed::Types, Unclaimed::Forwards] {
            assert_eq!(
                KeyVerdict::decide(&claims, unclaimed, &stroke("KeyX", CTRL), true),
                KeyVerdict::Browser
            );
        }
    }
}
