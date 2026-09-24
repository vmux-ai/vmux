#[vmux_api::ui_state(Default, target = "simulator")]
pub struct SimulatorReady {
    pub port: u16,
    pub capability: String,
    pub version: String,
    pub device_name: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frame_stride: u32,
}

#[vmux_api::ui_event(Default, target = "simulator")]
pub struct SimulatorTouch {
    pub phase: SimulatorTouchPhase,
    pub x: f32,
    pub y: f32,
}

#[vmux_api::contract(Copy, Default, Eq)]
pub enum SimulatorTouchPhase {
    #[default]
    Down,
    Move,
    Up,
    Cancel,
    Tap,
}

#[vmux_api::ui_event_variants(Eq, target = "simulator")]
pub enum SimulatorInputOperation {
    Text {
        text: String,
    },
    Key {
        code: u16,
    },
    ModifiedKey {
        code: u16,
        modifiers: SimulatorKeyModifiers,
    },
    HardwareButton {
        button: HardwareButton,
    },
}

#[vmux_api::contract(Copy, Default, Eq)]
pub struct SimulatorKeyModifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

#[vmux_api::contract(Copy, Eq)]
pub enum HardwareButton {
    Home,
    Lock,
    Siri,
}

#[vmux_api::ui_event_variants(Copy, Eq, target = "simulator")]
pub enum SimulatorClipboardOperation {
    Copy,
    Cut,
    Paste,
    SelectAll,
}

#[vmux_api::ui_event(Copy, Eq, target = "simulator")]
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

impl SimulatorInputOperation {
    fn parse_browser_key(key: &str) -> Option<Self> {
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
                return Some(Self::Text {
                    text: c.to_string(),
                });
            }
        };
        Some(Self::Key { code })
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
        Some(Self::ModifiedKey { code, modifiers })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSimulatorKey;

impl std::fmt::Display for InvalidSimulatorKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid simulator browser key")
    }
}

impl std::error::Error for InvalidSimulatorKey {}

impl TryFrom<&str> for SimulatorInputOperation {
    type Error = InvalidSimulatorKey;

    fn try_from(key: &str) -> Result<Self, Self::Error> {
        Self::parse_browser_key(key).ok_or(InvalidSimulatorKey)
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
            SimulatorInputOperation::try_from("a"),
            Ok(SimulatorInputOperation::Text { text: "a".into() })
        );
        assert_eq!(
            SimulatorInputOperation::try_from("あ"),
            Ok(SimulatorInputOperation::Text { text: "あ".into() })
        );
    }

    #[test]
    fn keys_with_no_text_become_hid_codes() {
        assert_eq!(
            SimulatorInputOperation::try_from("Enter"),
            Ok(SimulatorInputOperation::Key { code: 40 })
        );
        assert_eq!(
            SimulatorInputOperation::try_from("Backspace"),
            Ok(SimulatorInputOperation::Key { code: 42 })
        );
        assert_eq!(
            SimulatorInputOperation::try_from("ArrowUp"),
            Ok(SimulatorInputOperation::Key { code: 82 })
        );
    }

    #[test]
    fn a_modifier_or_unknown_named_key_is_dropped() {
        for key in ["Shift", "Meta", "F13", "Unidentified"] {
            assert!(SimulatorInputOperation::try_from(key).is_err(), "{key}");
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
            SimulatorInputOperation::modified_browser_code("ArrowLeft", modifiers),
            Some(SimulatorInputOperation::ModifiedKey {
                code: 80,
                modifiers,
            })
        );
        assert_eq!(
            SimulatorInputOperation::modified_browser_code("KeyA", modifiers),
            Some(SimulatorInputOperation::ModifiedKey { code: 4, modifiers })
        );
        assert_eq!(modifiers.hid_codes(), vec![225, 227]);
    }
}
