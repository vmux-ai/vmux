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

#[cfg(test)]
mod tests {
    use super::*;

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
