//! What the page and the plugin say to each other.
//!
//! Ungated: it is the one part both halves compile, and the reason neither has to know how the
//! other is built. rkyv on the wire.

/// Bin-event id: native → page, where to point the `<img>` and what is being mirrored.
pub const SIMULATOR_READY_EVENT: &str = "simulator_ready";

/// Native → page: the loopback origin serving this device's MJPEG stream.
///
/// `port` is zero until the listener is up, which is how the view knows to keep waiting.
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
    /// Dotted iOS runtime, e.g. `27.0`. Empty when nothing is booted.
    pub version: String,
    pub device_name: String,
}

/// Page → native: where the pointer went, as a fraction of the streamed image.
///
/// Carries no event id of its own: `vmux_ui::hooks::send` keys page → native events by type
/// name, which is what `BinEventEmitterPlugin` matches on.
///
/// Fractions rather than pixels or points: the view knows how big it drew the image and nothing
/// else, and the host already knows the device's point size. Sending a fraction keeps the device
/// geometry on one side and survives any scale the stream is served at.
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
pub struct SimulatorGesture {
    pub from_x: f32,
    pub from_y: f32,
    pub to_x: f32,
    pub to_y: f32,
}

impl SimulatorGesture {
    /// Below this, a drag is a tap. A fraction of the shorter edge, so it does not depend on
    /// how large the view happens to be drawn.
    pub const DRAG_THRESHOLD: f32 = 0.012;

    pub fn is_tap(&self) -> bool {
        let dx = self.to_x - self.from_x;
        let dy = self.to_y - self.from_y;
        (dx * dx + dy * dy).sqrt() < Self::DRAG_THRESHOLD
    }
}

/// Page → native: a keystroke to replay on the device.
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
    /// Printable input, typed as-is.
    Text(String),
    /// A key that produces no text, as a USB HID usage code.
    Code(u16),
    /// A button on the side of the device rather than on its screen.
    Button(HardwareButton),
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

impl HardwareButton {
    /// The name `axe button` takes.
    pub fn as_arg(&self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Lock => "lock",
            Self::Siri => "siri",
        }
    }
}

impl SimulatorKey {
    /// From a browser `KeyboardEvent.key`, or `None` when the key means nothing to the device.
    ///
    /// Printable keys are typed rather than sent as codes: `axe type` handles any character,
    /// including ones no HID code names, and keeps the keyboard layout out of this.
    pub fn of_browser_key(key: &str) -> Option<Self> {
        // USB HID usage codes, as `axe key` documents them.
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
    fn a_stationary_press_is_a_tap() {
        let gesture = SimulatorGesture {
            from_x: 0.5,
            from_y: 0.4,
            to_x: 0.5,
            to_y: 0.4,
        };

        assert!(gesture.is_tap());
    }

    #[test]
    fn a_scroll_length_drag_is_not_a_tap() {
        let gesture = SimulatorGesture {
            from_x: 0.5,
            from_y: 0.8,
            to_x: 0.5,
            to_y: 0.2,
        };

        assert!(!gesture.is_tap());
    }

    #[test]
    fn a_few_pixels_of_hand_wobble_still_taps() {
        let gesture = SimulatorGesture {
            from_x: 0.500,
            from_y: 0.400,
            to_x: 0.505,
            to_y: 0.404,
        };

        assert!(gesture.is_tap());
    }
}
