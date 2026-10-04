#[vmux_api::ui_event(Default, Eq)]
pub struct UiKeyContext {
    pub keys: Vec<String>,
}

#[vmux_api::ui_state(Default, Eq)]
pub struct KeyClaims {
    pub keys: Vec<ClaimedKey>,
}

impl KeyClaims {
    pub fn contains(&self, stroke: &KeyStroke) -> bool {
        self.command(stroke).is_some()
    }

    pub fn command(&self, stroke: &KeyStroke) -> Option<&str> {
        self.keys
            .iter()
            .find(|claimed| claimed.code == stroke.code && claimed.mods == stroke.mods)
            .map(|claimed| claimed.command.as_str())
    }
}

#[vmux_api::contract(Default, Eq)]
pub struct ClaimedKey {
    pub code: String,
    pub mods: KeyModifiers,
    pub command: String,
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

#[vmux_api::ui_event(Default, Eq)]
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
