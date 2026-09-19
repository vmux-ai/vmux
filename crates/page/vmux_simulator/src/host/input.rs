use super::device::{Axe, SimulatorDevice};
use super::hid::{HidBroker, HidRequest};
use super::{
    ActiveSimulatorView, DevicePixels, DevicePoints, HardwareButtonRequest,
    SimulatorClipboardRequest, SimulatorControlRequest, SimulatorControlResponse,
    SimulatorFocusRequest, SimulatorFocusSet, SimulatorInputSet, SimulatorSoftwareKeyboardRequest,
};
use crate::event::{
    SimulatorClipboard, SimulatorKey, SimulatorSoftwareKeyboard, SimulatorTouch,
    SimulatorTouchPhase,
};
use crate::url::PAGE_HOST;
use bevy::prelude::*;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use vmux_wire::protocol::{SimulatorAction, SimulatorButton};

pub(super) struct SimulatorInputPlugin;

impl Plugin for SimulatorInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClipboardWorker>()
            .add_message::<SimulatorKeyRequest>()
            .add_systems(Update, apply_focus_requests.in_set(SimulatorFocusSet))
            .add_systems(
                Update,
                (
                    handle_button_requests,
                    handle_clipboard_requests,
                    handle_control_requests,
                    send_key_requests,
                )
                    .chain()
                    .in_set(SimulatorInputSet),
            )
            .add_plugins(BinEventEmitterPlugin::<(
                SimulatorTouch,
                SimulatorKey,
                SimulatorClipboard,
                SimulatorSoftwareKeyboard,
            )>::for_hosts(&[PAGE_HOST]))
            .add_observer(on_touch)
            .add_observer(on_key)
            .add_observer(on_clipboard)
            .add_observer(on_software_keyboard);
    }
}

#[derive(Message)]
struct SimulatorKeyRequest {
    view: Option<Entity>,
    key: SimulatorKey,
}

#[derive(Component, Default)]
pub(super) struct DeviceTouchSession {
    start: Option<(f32, f32)>,
    last: Option<(f32, f32)>,
    dragging: bool,
}

#[derive(Resource)]
struct ClipboardWorker(mpsc::Sender<ClipboardJob>);

impl FromWorld for ClipboardWorker {
    fn from_world(_world: &mut World) -> Self {
        let (sender, receiver) = mpsc::channel::<ClipboardJob>();
        let spawned = std::thread::Builder::new()
            .name("vmux-simulator-clipboard".into())
            .spawn(move || {
                for job in receiver {
                    if let Err(error) = job.run() {
                        error!("simulator clipboard failed: {error}");
                    }
                }
            });
        if let Err(error) = spawned {
            error!("could not start simulator clipboard worker: {error}");
        }
        Self(sender)
    }
}

struct ClipboardJob {
    axe: PathBuf,
    udid: String,
    action: crate::event::SimulatorClipboardAction,
}

impl ClipboardJob {
    fn run(&self) -> Result<(), String> {
        match self.action {
            crate::event::SimulatorClipboardAction::Copy => {
                self.key_combo(6)?;
                self.sync(&self.udid, "host")
            }
            crate::event::SimulatorClipboardAction::Cut => {
                self.key_combo(27)?;
                self.sync(&self.udid, "host")
            }
            crate::event::SimulatorClipboardAction::Paste => {
                self.sync("host", &self.udid)?;
                self.key_combo(25)
            }
            crate::event::SimulatorClipboardAction::SelectAll => self.key_combo(4),
        }
    }

    fn key_combo(&self, key: u8) -> Result<(), String> {
        let output = Command::new(&self.axe)
            .args([
                "key-combo",
                "--modifiers",
                "227",
                "--key",
                &key.to_string(),
                "--udid",
                &self.udid,
            ])
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("could not send simulator clipboard shortcut: {error}"))?;
        if output.status.success() {
            return Ok(());
        }
        Err(format!(
            "could not send simulator clipboard shortcut: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }

    fn sync(&self, source: &str, destination: &str) -> Result<(), String> {
        let output = Command::new("xcrun")
            .args(["simctl", "pbsync", source, destination])
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("could not sync simulator clipboard: {error}"))?;
        if output.status.success() {
            return Ok(());
        }
        Err(format!(
            "could not sync simulator clipboard: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

pub struct DeviceCoordinates {
    points: (f32, f32),
    pixels: (u32, u32),
}

type ControlAttachments<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static SimulatorDevice,
        &'static Axe,
        Option<&'static DevicePoints>,
        Option<&'static DevicePixels>,
        Option<&'static HidBroker>,
    ),
>;

impl DeviceCoordinates {
    pub fn new(points: (f32, f32), pixels: (u32, u32)) -> Option<Self> {
        if points.0 <= 0.0 || points.1 <= 0.0 || pixels.0 == 0 || pixels.1 == 0 {
            return None;
        }
        Some(Self { points, pixels })
    }

    pub fn point(&self, pixel: (u32, u32)) -> (f32, f32) {
        let max_pixel_x = self.pixels.0.saturating_sub(1).max(1) as f32;
        let max_pixel_y = self.pixels.1.saturating_sub(1).max(1) as f32;
        let max_point_x = (self.points.0 - 1.0).max(0.0);
        let max_point_y = (self.points.1 - 1.0).max(0.0);
        (
            pixel.0.min(self.pixels.0.saturating_sub(1)) as f32 / max_pixel_x * max_point_x,
            pixel.1.min(self.pixels.1.saturating_sub(1)) as f32 / max_pixel_y * max_point_y,
        )
    }
}

fn on_touch(
    trigger: On<BinReceive<SimulatorTouch>>,
    mut attachments: Query<(&DevicePoints, &HidBroker, &mut DeviceTouchSession)>,
) {
    const DRAG_THRESHOLD: f32 = 6.0;

    let Ok((points, hid, mut session)) = attachments.get_mut(trigger.event().webview) else {
        return;
    };
    let touch = &trigger.event().payload;
    let Some(point) = normalized_point(touch, (points.0, points.1)) else {
        return;
    };
    match touch.phase {
        SimulatorTouchPhase::Down => {
            if let Some(previous) = session.last {
                hid.dispatch(HidRequest::up(previous));
            }
            session.start = Some(point);
            session.last = Some(point);
            session.dragging = false;
            hid.dispatch(HidRequest::down(point));
        }
        SimulatorTouchPhase::Move => {
            let Some(start) = session.start else {
                return;
            };
            let distance = ((point.0 - start.0).powi(2) + (point.1 - start.1).powi(2)).sqrt();
            if !session.dragging && distance >= DRAG_THRESHOLD {
                session.dragging = true;
            }
            session.last = Some(point);
            if session.dragging {
                hid.dispatch(HidRequest::move_to(point));
            }
        }
        SimulatorTouchPhase::Up => {
            if session.start.take().is_none() {
                return;
            }
            session.last = None;
            hid.dispatch(HidRequest::up(point));
            session.dragging = false;
        }
        SimulatorTouchPhase::Cancel => {
            session.start = None;
            if let Some(last) = session.last.take() {
                hid.dispatch(HidRequest::up(last));
            }
            session.last = None;
            session.dragging = false;
        }
        SimulatorTouchPhase::Tap => {
            session.start = None;
            session.last = None;
            session.dragging = false;
            hid.dispatch(HidRequest::tap(point));
        }
    }
}

fn normalized_point(touch: &SimulatorTouch, points: (f32, f32)) -> Option<(f32, f32)> {
    if points.0 <= 0.0 || points.1 <= 0.0 {
        return None;
    }
    let max_x = (points.0 - 1.0).max(0.0);
    let max_y = (points.1 - 1.0).max(0.0);
    Some((
        touch.x.clamp(0.0, 1.0) * max_x,
        touch.y.clamp(0.0, 1.0) * max_y,
    ))
}

fn on_key(trigger: On<BinReceive<SimulatorKey>>, mut requests: MessageWriter<SimulatorKeyRequest>) {
    requests.write(SimulatorKeyRequest {
        view: Some(trigger.event().webview),
        key: trigger.event().payload.clone(),
    });
}

fn on_clipboard(
    trigger: On<BinReceive<SimulatorClipboard>>,
    mut requests: MessageWriter<SimulatorClipboardRequest>,
) {
    requests.write(SimulatorClipboardRequest {
        view: Some(trigger.event().webview),
        action: trigger.event().payload.action,
    });
}

fn on_software_keyboard(
    trigger: On<BinReceive<SimulatorSoftwareKeyboard>>,
    mut requests: MessageWriter<SimulatorSoftwareKeyboardRequest>,
) {
    requests.write(SimulatorSoftwareKeyboardRequest {
        view: Some(trigger.event().webview),
    });
}

fn apply_focus_requests(
    mut requests: MessageReader<SimulatorFocusRequest>,
    mut active: ResMut<ActiveSimulatorView>,
) {
    for request in requests.read() {
        active.0 = request.0;
    }
}

fn handle_button_requests(
    mut requests: MessageReader<HardwareButtonRequest>,
    mut keys: MessageWriter<SimulatorKeyRequest>,
) {
    for request in requests.read() {
        keys.write(SimulatorKeyRequest {
            view: request.view,
            key: SimulatorKey::Button(request.button),
        });
    }
}

fn handle_clipboard_requests(
    mut requests: MessageReader<SimulatorClipboardRequest>,
    active: Res<ActiveSimulatorView>,
    attachments: Query<(Entity, &SimulatorDevice, &Axe)>,
    worker: Res<ClipboardWorker>,
) {
    for request in requests.read() {
        let target = request
            .view
            .filter(|entity| attachments.contains(*entity))
            .or_else(|| active.select(attachments.iter().map(|(entity, _, _)| entity)));
        let Some(target) = target else {
            continue;
        };
        let Ok((_, device, axe)) = attachments.get(target) else {
            continue;
        };
        let job = ClipboardJob {
            axe: axe.path().to_path_buf(),
            udid: device.udid.clone(),
            action: request.action,
        };
        if worker.0.send(job).is_err() {
            error!("simulator clipboard worker stopped");
        }
    }
}

fn handle_control_requests(
    mut requests: MessageReader<SimulatorControlRequest>,
    mut responses: MessageWriter<SimulatorControlResponse>,
    mut keys: MessageWriter<SimulatorKeyRequest>,
    active: Res<ActiveSimulatorView>,
    attachments: ControlAttachments,
) {
    for request in requests.read() {
        let target = active.select(attachments.iter().map(|(entity, ..)| entity));
        let Some(target) = target else {
            responses.write(SimulatorControlResponse {
                request_id: request.request_id,
                result: Err("no iOS Simulator is attached".to_string()),
            });
            continue;
        };
        let Ok((_, _, _, points, pixels, hid)) = attachments.get(target) else {
            continue;
        };
        let result = match &request.action {
            SimulatorAction::Tap { x, y } => match (control_coordinates(points, pixels), hid) {
                (Ok(coordinates), Some(hid)) => {
                    hid.dispatch(HidRequest::tap(coordinates.point((*x, *y))));
                    Ok(format!("tapped simulator at ({x}, {y})"))
                }
                (Err(error), _) => Err(error),
                (_, None) => Err("simulator input is unavailable".to_string()),
            },
            SimulatorAction::Swipe {
                start_x,
                start_y,
                end_x,
                end_y,
                duration_ms,
            } => match (control_coordinates(points, pixels), hid) {
                (Ok(coordinates), Some(hid)) => {
                    let from = coordinates.point((*start_x, *start_y));
                    let to = coordinates.point((*end_x, *end_y));
                    hid.dispatch(HidRequest::swipe(from, to, *duration_ms));
                    Ok(format!(
                        "swiped simulator from ({start_x}, {start_y}) to ({end_x}, {end_y})"
                    ))
                }
                (Err(error), _) => Err(error),
                (_, None) => Err("simulator input is unavailable".to_string()),
            },
            SimulatorAction::TypeText(text) => {
                keys.write(SimulatorKeyRequest {
                    view: Some(target),
                    key: SimulatorKey::Text(text.clone()),
                });
                Ok("typed text into simulator".to_string())
            }
            SimulatorAction::Key(keycode) => {
                keys.write(SimulatorKeyRequest {
                    view: Some(target),
                    key: SimulatorKey::Code(u16::from(*keycode)),
                });
                Ok(format!("pressed simulator keycode {keycode}"))
            }
            SimulatorAction::Button(button) => {
                let button = match button {
                    SimulatorButton::Home => crate::event::HardwareButton::Home,
                    SimulatorButton::Lock => crate::event::HardwareButton::Lock,
                    SimulatorButton::Siri => crate::event::HardwareButton::Siri,
                };
                keys.write(SimulatorKeyRequest {
                    view: Some(target),
                    key: SimulatorKey::Button(button),
                });
                Ok("pressed simulator hardware button".to_string())
            }
        };
        responses.write(SimulatorControlResponse {
            request_id: request.request_id,
            result,
        });
    }
}

fn control_coordinates(
    points: Option<&DevicePoints>,
    pixels: Option<&DevicePixels>,
) -> Result<DeviceCoordinates, String> {
    let points = points.ok_or("simulator point dimensions are unavailable")?;
    let pixels = pixels.ok_or("simulator pixel dimensions are unavailable")?;
    DeviceCoordinates::new((points.0, points.1), (pixels.0, pixels.1))
        .ok_or_else(|| "simulator dimensions are invalid".to_string())
}

fn send_key_requests(
    mut requests: MessageReader<SimulatorKeyRequest>,
    active: Res<ActiveSimulatorView>,
    attachments: Query<(Entity, &SimulatorDevice, &Axe)>,
) {
    for request in requests.read() {
        let target = request
            .view
            .filter(|entity| attachments.contains(*entity))
            .or_else(|| active.select(attachments.iter().map(|(entity, _, _)| entity)));
        let Some(target) = target else {
            continue;
        };
        let Ok((_, device, axe)) = attachments.get(target) else {
            continue;
        };
        let mut command = axe.command();
        match &request.key {
            SimulatorKey::Text(text) => {
                command.arg("type").arg(text);
            }
            SimulatorKey::Code(code) => {
                command.arg("key").arg(code.to_string());
            }
            SimulatorKey::Modified { code, modifiers } => {
                let modifiers = modifiers
                    .hid_codes()
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                command
                    .arg("key-combo")
                    .arg("--modifiers")
                    .arg(modifiers)
                    .arg("--key")
                    .arg(code.to_string());
            }
            SimulatorKey::Button(button) => {
                command.arg("button").arg(button.as_arg());
            }
        }
        command.args(["--udid", &device.udid]);
        Axe::run_detached(command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POINTS: (f32, f32) = (402.0, 874.0);

    #[test]
    fn the_centre_of_the_view_is_the_centre_of_the_device() {
        let touch = SimulatorTouch {
            phase: SimulatorTouchPhase::Move,
            x: 0.5,
            y: 0.5,
        };

        let point = normalized_point(&touch, POINTS).expect("resolved");

        assert!((point.0 - 200.5).abs() < 0.01, "got {point:?}");
        assert!((point.1 - 436.5).abs() < 0.01, "got {point:?}");
    }

    #[test]
    fn fractions_outside_the_image_are_clamped_onto_it() {
        let touch = SimulatorTouch {
            phase: SimulatorTouchPhase::Down,
            x: -0.5,
            y: 2.0,
        };

        let point = normalized_point(&touch, POINTS).expect("resolved");

        assert_eq!(point.0, 0.0);
        assert!((point.1 - (POINTS.1 - 1.0)).abs() < 0.01, "got {point:?}");
    }

    #[test]
    fn a_device_with_no_measured_point_size_has_no_gesture() {
        let touch = SimulatorTouch {
            phase: SimulatorTouchPhase::Down,
            x: 0.5,
            y: 0.5,
        };

        assert!(normalized_point(&touch, (0.0, 0.0)).is_none());
    }

    #[test]
    fn screenshot_pixels_map_to_simulator_points() {
        let coordinates = DeviceCoordinates::new((402.0, 874.0), (1206, 2622)).expect("mapping");

        let point = coordinates.point((603, 1311));

        assert!((point.0 - 200.67).abs() < 0.01);
        assert!((point.1 - 436.67).abs() < 0.01);
    }
}
