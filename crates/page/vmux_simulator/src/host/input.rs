use super::device::{Axe, SimulatorDevice};
use super::geometry::Mapping;
use bevy::input::ButtonState;
use bevy::input::mouse::MouseButtonInput;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Turns clicks and drags in the mirror into AXe gestures on the guest.
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PressOrigin>()
            .add_systems(Update, Self::forward);
    }
}

/// Where the current drag started, in device points.
#[derive(Resource, Default)]
struct PressOrigin(Option<Vec2>);

/// Below this, a drag is a tap. Device points, so it is resolution-independent.
const DRAG_THRESHOLD: f32 = 10.0;

impl InputPlugin {
    fn forward(
        mut clicks: MessageReader<MouseButtonInput>,
        window: Single<&Window, With<PrimaryWindow>>,
        mapping: Option<Res<Mapping>>,
        device: Option<Res<SimulatorDevice>>,
        mut origin: ResMut<PressOrigin>,
    ) {
        let (Some(mapping), Some(device)) = (mapping.as_deref(), device.as_deref()) else {
            clicks.clear();
            return;
        };
        for click in clicks.read() {
            if click.button != MouseButton::Left {
                continue;
            }
            let Some(cursor) = window.cursor_position() else {
                continue;
            };
            let Some(point) = mapping.cursor_to_device(cursor) else {
                origin.0 = None;
                continue;
            };
            match click.state {
                ButtonState::Pressed => origin.0 = Some(point),
                ButtonState::Released => {
                    let Some(start) = origin.0.take() else {
                        continue;
                    };
                    Gesture::between(start, point).dispatch(device);
                }
            }
        }
    }
}

/// A completed pointer interaction, in device points.
enum Gesture {
    Tap { at: Vec2 },
    Swipe { from: Vec2, to: Vec2 },
}

impl Gesture {
    fn between(from: Vec2, to: Vec2) -> Self {
        if from.distance(to) < DRAG_THRESHOLD {
            Self::Tap { at: to }
        } else {
            Self::Swipe { from, to }
        }
    }

    /// Runs AXe off-thread: a tap costs a process spawn, which would otherwise stall the frame.
    fn dispatch(self, device: &SimulatorDevice) {
        let mut command = Axe::command();
        match self {
            Self::Tap { at } => {
                command
                    .arg("tap")
                    .args(["-x", &format!("{:.0}", at.x)])
                    .args(["-y", &format!("{:.0}", at.y)]);
            }
            Self::Swipe { from, to } => {
                command
                    .arg("swipe")
                    .args(["--start-x", &format!("{:.0}", from.x)])
                    .args(["--start-y", &format!("{:.0}", from.y)])
                    .args(["--end-x", &format!("{:.0}", to.x)])
                    .args(["--end-y", &format!("{:.0}", to.y)]);
            }
        }
        command.args(["--udid", &device.udid]);
        std::thread::spawn(move || {
            if let Err(error) = command.status() {
                warn!("axe gesture failed: {error}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_drag_is_a_tap() {
        let start = Vec2::new(200.0, 400.0);
        let end = start + Vec2::new(3.0, 4.0);

        assert!(matches!(Gesture::between(start, end), Gesture::Tap { .. }));
    }

    #[test]
    fn a_long_drag_is_a_swipe_that_keeps_its_direction() {
        let start = Vec2::new(200.0, 700.0);
        let end = Vec2::new(200.0, 200.0);

        let Gesture::Swipe { from, to } = Gesture::between(start, end) else {
            panic!("expected a swipe");
        };
        assert_eq!(from, start);
        assert_eq!(to, end);
    }
}
