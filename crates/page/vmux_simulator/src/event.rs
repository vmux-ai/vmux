pub const SIMULATOR_READY_EVENT: &str = "simulator_ready";

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SimulatorReady {
    pub port: u16,
    pub capability: String,
    pub version: String,
    pub device_name: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frame_stride: u32,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SimulatorTouch {
    pub phase: SimulatorTouchPhase,
    pub x: f32,
    pub y: f32,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum SimulatorTouchPhase {
    #[default]
    Down,
    Move,
    Up,
    Cancel,
    Tap,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum SimulatorKey {
    Text(String),
    Code(u16),
    Modified {
        code: u16,
        modifiers: SimulatorKeyModifiers,
    },
    Button(HardwareButton),
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SimulatorKeyModifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum HardwareButton {
    Home,
    Lock,
    Siri,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum SimulatorClipboardAction {
    Copy,
    Paste,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SimulatorClipboard {
    pub action: SimulatorClipboardAction,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SimulatorSoftwareKeyboard;

impl HardwareButton {
    pub fn as_arg(&self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Lock => "lock",
            Self::Siri => "siri",
        }
    }
}

impl SimulatorKey {
    pub fn of_browser_key(key: &str) -> Option<Self> {
        let code = match key {
            "Enter" => 40,
            "Escape" => 41,
            "Backspace" => 42,
            "Tab" => 43,
            "ArrowRight" => 79,
            "ArrowLeft" => 80,
            "ArrowDown" => 81,
            "ArrowUp" => 82,
            _ => {
                let mut chars = key.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else {
                    return None;
                };
                return Some(Self::Text(c.to_string()));
            }
        };
        Some(Self::Code(code))
    }

    pub fn modified_browser_code(
        browser_code: &str,
        modifiers: SimulatorKeyModifiers,
    ) -> Option<Self> {
        if modifiers.is_empty() {
            return None;
        }
        let code = match browser_code {
            "KeyA" => 4,
            "KeyB" => 5,
            "KeyC" => 6,
            "KeyD" => 7,
            "KeyE" => 8,
            "KeyF" => 9,
            "KeyG" => 10,
            "KeyH" => 11,
            "KeyI" => 12,
            "KeyJ" => 13,
            "KeyK" => 14,
            "KeyL" => 15,
            "KeyM" => 16,
            "KeyN" => 17,
            "KeyO" => 18,
            "KeyP" => 19,
            "KeyQ" => 20,
            "KeyR" => 21,
            "KeyS" => 22,
            "KeyT" => 23,
            "KeyU" => 24,
            "KeyV" => 25,
            "KeyW" => 26,
            "KeyX" => 27,
            "KeyY" => 28,
            "KeyZ" => 29,
            "Digit1" => 30,
            "Digit2" => 31,
            "Digit3" => 32,
            "Digit4" => 33,
            "Digit5" => 34,
            "Digit6" => 35,
            "Digit7" => 36,
            "Digit8" => 37,
            "Digit9" => 38,
            "Digit0" => 39,
            "Enter" => 40,
            "Escape" => 41,
            "Backspace" => 42,
            "Tab" => 43,
            "Space" => 44,
            "Minus" => 45,
            "Equal" => 46,
            "BracketLeft" => 47,
            "BracketRight" => 48,
            "Backslash" => 49,
            "Semicolon" => 51,
            "Quote" => 52,
            "Backquote" => 53,
            "Comma" => 54,
            "Period" => 55,
            "Slash" => 56,
            "CapsLock" => 57,
            "F1" => 58,
            "F2" => 59,
            "F3" => 60,
            "F4" => 61,
            "F5" => 62,
            "F6" => 63,
            "F7" => 64,
            "F8" => 65,
            "F9" => 66,
            "F10" => 67,
            "F11" => 68,
            "F12" => 69,
            "Home" => 74,
            "PageUp" => 75,
            "Delete" => 76,
            "End" => 77,
            "PageDown" => 78,
            "ArrowRight" => 79,
            "ArrowLeft" => 80,
            "ArrowDown" => 81,
            "ArrowUp" => 82,
            _ => return None,
        };
        Some(Self::Modified { code, modifiers })
    }
}

impl SimulatorKeyModifiers {
    pub fn is_empty(self) -> bool {
        !self.control && !self.shift && !self.alt && !self.meta
    }

    pub fn hid_codes(self) -> Vec<u8> {
        let mut codes = Vec::with_capacity(4);
        if self.control {
            codes.push(224);
        }
        if self.shift {
            codes.push(225);
        }
        if self.alt {
            codes.push(226);
        }
        if self.meta {
            codes.push(227);
        }
        codes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_printable_key_is_typed_rather_than_coded() {
        assert_eq!(
            SimulatorKey::of_browser_key("a"),
            Some(SimulatorKey::Text("a".into()))
        );
        assert_eq!(
            SimulatorKey::of_browser_key("あ"),
            Some(SimulatorKey::Text("あ".into()))
        );
    }

    #[test]
    fn keys_with_no_text_become_hid_codes() {
        assert_eq!(
            SimulatorKey::of_browser_key("Enter"),
            Some(SimulatorKey::Code(40))
        );
        assert_eq!(
            SimulatorKey::of_browser_key("Backspace"),
            Some(SimulatorKey::Code(42))
        );
        assert_eq!(
            SimulatorKey::of_browser_key("ArrowUp"),
            Some(SimulatorKey::Code(82))
        );
    }

    #[test]
    fn a_modifier_or_unknown_named_key_is_dropped() {
        for key in ["Shift", "Meta", "F13", "Unidentified"] {
            assert_eq!(SimulatorKey::of_browser_key(key), None, "{key}");
        }
    }

    #[test]
    fn modified_browser_keys_become_hid_combinations() {
        let modifiers = SimulatorKeyModifiers {
            shift: true,
            meta: true,
            ..Default::default()
        };

        assert_eq!(
            SimulatorKey::modified_browser_code("ArrowLeft", modifiers),
            Some(SimulatorKey::Modified {
                code: 80,
                modifiers,
            })
        );
        assert_eq!(
            SimulatorKey::modified_browser_code("KeyA", modifiers),
            Some(SimulatorKey::Modified { code: 4, modifiers })
        );
        assert_eq!(modifiers.hid_codes(), vec![225, 227]);
    }
}
