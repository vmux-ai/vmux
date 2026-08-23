use super::device::{Axe, SimulatorDevice};
use crate::event::{SimulatorGesture, SimulatorKey};

/// A gesture resolved onto a specific device, in the points `axe` addresses.
pub struct DeviceGesture {
    udid: String,
    from: (f32, f32),
    to: (f32, f32),
    tap: bool,
}

impl DeviceGesture {
    /// Scales the view's 0..1 fractions by the device's point size.
    pub fn resolve(
        gesture: &SimulatorGesture,
        device: &SimulatorDevice,
        points: (f32, f32),
    ) -> Option<Self> {
        if points.0 <= 0.0 || points.1 <= 0.0 {
            return None;
        }
        let on_device =
            |x: f32, y: f32| (x.clamp(0.0, 1.0) * points.0, y.clamp(0.0, 1.0) * points.1);
        Some(Self {
            udid: device.udid.clone(),
            from: on_device(gesture.from_x, gesture.from_y),
            to: on_device(gesture.to_x, gesture.to_y),
            tap: gesture.is_tap(),
        })
    }

    /// Runs `axe` off-thread: a gesture costs a process spawn, which would otherwise stall a frame.
    pub fn dispatch(self, axe: &Axe) {
        let mut command = axe.command();
        if self.tap {
            command
                .arg("tap")
                .args(["-x", &format!("{:.0}", self.to.0)])
                .args(["-y", &format!("{:.0}", self.to.1)]);
        } else {
            command
                .arg("swipe")
                .args(["--start-x", &format!("{:.0}", self.from.0)])
                .args(["--start-y", &format!("{:.0}", self.from.1)])
                .args(["--end-x", &format!("{:.0}", self.to.0)])
                .args(["--end-y", &format!("{:.0}", self.to.1)]);
        }
        command.args(["--udid", &self.udid]);
        Axe::run_detached(command);
    }
}

/// A keystroke resolved onto a specific device.
pub struct DeviceKey {
    udid: String,
    key: SimulatorKey,
}

impl DeviceKey {
    pub fn resolve(key: &SimulatorKey, device: &SimulatorDevice) -> Self {
        Self {
            udid: device.udid.clone(),
            key: key.clone(),
        }
    }

    pub fn dispatch(self, axe: &Axe) {
        let mut command = axe.command();
        match &self.key {
            SimulatorKey::Text(text) => {
                command.arg("type").arg(text);
            }
            SimulatorKey::Code(code) => {
                command.arg("key").arg(code.to_string());
            }
            SimulatorKey::Button(button) => {
                command.arg("button").arg(button.as_arg());
            }
        }
        command.args(["--udid", &self.udid]);
        Axe::run_detached(command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POINTS: (f32, f32) = (402.0, 874.0);

    impl SimulatorDevice {
        fn fixture() -> Self {
            Self {
                udid: "174D774A-1F21-455C-AB54-AF19D513988A".into(),
                name: "iPhone 17 Pro".into(),
                version: None,
            }
        }
    }

    #[test]
    fn the_centre_of_the_view_is_the_centre_of_the_device() {
        let gesture = SimulatorGesture {
            from_x: 0.5,
            from_y: 0.5,
            to_x: 0.5,
            to_y: 0.5,
        };

        let resolved = DeviceGesture::resolve(&gesture, &SimulatorDevice::fixture(), POINTS)
            .expect("resolved");

        assert!(
            (resolved.to.0 - 201.0).abs() < 0.01,
            "got {:?}",
            resolved.to
        );
        assert!(
            (resolved.to.1 - 437.0).abs() < 0.01,
            "got {:?}",
            resolved.to
        );
        assert!(resolved.tap);
    }

    #[test]
    fn a_drag_keeps_its_direction_and_is_not_a_tap() {
        let gesture = SimulatorGesture {
            from_x: 0.5,
            from_y: 0.8,
            to_x: 0.5,
            to_y: 0.2,
        };

        let resolved = DeviceGesture::resolve(&gesture, &SimulatorDevice::fixture(), POINTS)
            .expect("resolved");

        assert!(!resolved.tap);
        assert!(resolved.from.1 > resolved.to.1, "expected an upward swipe");
    }

    #[test]
    fn fractions_outside_the_image_are_clamped_onto_it() {
        let gesture = SimulatorGesture {
            from_x: -0.5,
            from_y: 2.0,
            to_x: -0.5,
            to_y: 2.0,
        };

        let resolved = DeviceGesture::resolve(&gesture, &SimulatorDevice::fixture(), POINTS)
            .expect("resolved");

        assert_eq!(resolved.to.0, 0.0);
        assert!(
            (resolved.to.1 - POINTS.1).abs() < 0.01,
            "got {:?}",
            resolved.to
        );
    }

    #[test]
    fn a_device_with_no_measured_point_size_has_no_gesture() {
        let gesture = SimulatorGesture::default();

        assert!(
            DeviceGesture::resolve(&gesture, &SimulatorDevice::fixture(), (0.0, 0.0)).is_none()
        );
    }
}
