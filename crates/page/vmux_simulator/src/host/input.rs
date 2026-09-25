use super::device::{Axe, SimulatorDevice};
use super::hid::{HidBroker, HidRequest};
use super::{
    ActiveSimulatorView, DevicePixels, DevicePoints, HardwareButtonRequest,
    SimulatorClipboardRequest, SimulatorControlRequest, SimulatorControlResponse,
    SimulatorFocusRequest, SimulatorFocusSet, SimulatorInputSet, SimulatorSoftwareKeyboardRequest,
};
use crate::event::{
    HardwareButton, SimulatorClipboardCopyRequest, SimulatorClipboardCutRequest,
    SimulatorClipboardOperation, SimulatorClipboardOperationRequests,
    SimulatorClipboardPasteRequest, SimulatorClipboardSelectAllRequest,
    SimulatorInputHardwareButtonRequest, SimulatorInputKeyRequest,
    SimulatorInputModifiedKeyRequest, SimulatorInputOperation, SimulatorInputOperationRequests,
    SimulatorInputTextRequest, SimulatorSoftwareKeyboard, SimulatorTouch, SimulatorTouchPhase,
};
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use vmux_api::protocol::{SimulatorButton, SimulatorInput};

pub(super) struct SimulatorInputPlugin;

impl Plugin for SimulatorInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClipboardWorker>()
            .add_message::<SimulatorInputRequest>()
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
            .add_plugins(UiEventPlugin::<(SimulatorTouch, SimulatorSoftwareKeyboard)>::default())
            .add_plugins(UiEventPlugin::<SimulatorInputOperationRequests>::default())
            .add_plugins(UiEventPlugin::<SimulatorClipboardOperationRequests>::default())
            .add_observer(on_touch)
            .add_observer(on_input_text)
            .add_observer(on_input_key)
            .add_observer(on_input_modified_key)
            .add_observer(on_input_hardware_button)
            .add_observer(on_clipboard_copy)
            .add_observer(on_clipboard_cut)
            .add_observer(on_clipboard_paste)
            .add_observer(on_clipboard_select_all)
            .add_observer(on_software_keyboard);
    }
}

#[derive(Message)]
struct SimulatorInputRequest {
    view: Option<Entity>,
    operation: SimulatorInputOperation,
}

#[derive(Component)]
pub(super) struct SimulatorKeyboard {
    sender: mpsc::Sender<SimulatorInputOperation>,
}

impl SimulatorKeyboard {
    pub fn start(axe: &Axe, device: &SimulatorDevice) -> io::Result<Self> {
        let runner = SimulatorKeyboardRunner {
            axe: axe.path().to_path_buf(),
            udid: device.udid.clone(),
        };
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("vmux-simulator-keyboard".into())
            .spawn(move || runner.run(receiver))?;
        Ok(Self { sender })
    }

    fn dispatch(&self, operation: SimulatorInputOperation) {
        if self.sender.send(operation).is_err() {
            error!("simulator keyboard worker stopped");
        }
    }
}

struct SimulatorKeyboardRunner {
    axe: PathBuf,
    udid: String,
}

impl SimulatorKeyboardRunner {
    const MAX_BATCH_KEYS: usize = 256;

    fn run(&self, receiver: mpsc::Receiver<SimulatorInputOperation>) {
        while let Ok(first) = receiver.recv() {
            let batch = SimulatorKeyboardBatch::from_receiver(first, &receiver);
            if let Err(error) = self.execute(batch) {
                error!("simulator keyboard failed: {error}");
            }
        }
    }

    fn execute(&self, batch: SimulatorKeyboardBatch) -> Result<(), String> {
        let mut command = Command::new(&self.axe);
        command
            .arg("batch")
            .args(["--udid", &self.udid])
            .env("AXE_HID_STABILIZATION_MS", "200")
            .stdin(Stdio::null());
        for step in batch.steps() {
            command.arg("--step").arg(step);
        }
        let output = command
            .output()
            .map_err(|error| format!("could not start AXe: {error}"))?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(format!("AXe exited with {}", output.status));
        }
        Err(stderr)
    }
}

struct SimulatorKeyboardBatch {
    operations: Vec<SimulatorInputOperation>,
}

impl SimulatorKeyboardBatch {
    fn from_receiver(
        first: SimulatorInputOperation,
        receiver: &mpsc::Receiver<SimulatorInputOperation>,
    ) -> Self {
        let mut operations = vec![first];
        while operations.len() < SimulatorKeyboardRunner::MAX_BATCH_KEYS {
            let Ok(operation) = receiver.try_recv() else {
                break;
            };
            operations.push(operation);
        }
        Self { operations }
    }

    fn steps(self) -> Vec<String> {
        let mut steps = Vec::new();
        let mut text = String::new();
        for operation in self.operations {
            match operation {
                SimulatorInputOperation::Text { text: value } => text.push_str(&value),
                other => {
                    Self::push_text(&mut steps, &mut text);
                    steps.push(Self::step(other));
                }
            }
        }
        Self::push_text(&mut steps, &mut text);
        steps
    }

    fn push_text(steps: &mut Vec<String>, text: &mut String) {
        if text.is_empty() {
            return;
        }
        steps.push(format!("type {}", Self::quote(text)));
        text.clear();
    }

    fn step(operation: SimulatorInputOperation) -> String {
        match operation {
            SimulatorInputOperation::Text { text } => format!("type {}", Self::quote(&text)),
            SimulatorInputOperation::Key { code } => format!("key {code}"),
            SimulatorInputOperation::ModifiedKey { code, modifiers } => {
                let modifiers = modifiers
                    .hid_codes()
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("key-combo --modifiers {modifiers} --key {code}")
            }
            SimulatorInputOperation::HardwareButton { button } => {
                format!("button {}", button.as_arg())
            }
        }
    }

    fn quote(text: &str) -> String {
        let mut quoted = String::with_capacity(text.len() + 2);
        quoted.push('"');
        for character in text.chars() {
            if matches!(character, '\\' | '"') {
                quoted.push('\\');
            }
            quoted.push(character);
        }
        quoted.push('"');
        quoted
    }
}

#[derive(Component, Default)]
pub(super) struct DeviceTouchSession {
    start: Option<(f32, f32)>,
    last: Option<(f32, f32)>,
    focus: Option<(f32, f32)>,
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
    operation: SimulatorClipboardOperation,
}

impl ClipboardJob {
    fn run(&self) -> Result<(), String> {
        match self.operation {
            SimulatorClipboardOperation::Copy => {
                self.key_combo(6)?;
                self.sync(&self.udid, "host")
            }
            SimulatorClipboardOperation::Cut => {
                self.key_combo(27)?;
                self.sync(&self.udid, "host")
            }
            SimulatorClipboardOperation::Paste => {
                self.sync("host", &self.udid)?;
                self.key_combo(25)
            }
            SimulatorClipboardOperation::SelectAll => self.key_combo(4),
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
            .env("AXE_HID_STABILIZATION_MS", "200")
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
    trigger: On<UiInput<SimulatorTouch>>,
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
            if !session.dragging {
                session.focus = Some(point);
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
            session.focus = Some(point);
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

fn on_input_text(
    trigger: On<UiInput<SimulatorInputTextRequest>>,
    mut requests: MessageWriter<SimulatorInputRequest>,
) {
    requests.write(SimulatorInputRequest {
        view: Some(trigger.event().webview),
        operation: trigger.event().payload.operation(),
    });
}

fn on_input_key(
    trigger: On<UiInput<SimulatorInputKeyRequest>>,
    mut requests: MessageWriter<SimulatorInputRequest>,
) {
    requests.write(SimulatorInputRequest {
        view: Some(trigger.event().webview),
        operation: trigger.event().payload.operation(),
    });
}

fn on_input_modified_key(
    trigger: On<UiInput<SimulatorInputModifiedKeyRequest>>,
    mut requests: MessageWriter<SimulatorInputRequest>,
) {
    requests.write(SimulatorInputRequest {
        view: Some(trigger.event().webview),
        operation: trigger.event().payload.operation(),
    });
}

fn on_input_hardware_button(
    trigger: On<UiInput<SimulatorInputHardwareButtonRequest>>,
    mut requests: MessageWriter<SimulatorInputRequest>,
) {
    requests.write(SimulatorInputRequest {
        view: Some(trigger.event().webview),
        operation: trigger.event().payload.operation(),
    });
}

fn on_clipboard_copy(
    trigger: On<UiInput<SimulatorClipboardCopyRequest>>,
    mut requests: MessageWriter<SimulatorClipboardRequest>,
) {
    requests.write(SimulatorClipboardRequest {
        view: Some(trigger.event().webview),
        operation: trigger.event().payload.operation(),
    });
}

fn on_clipboard_cut(
    trigger: On<UiInput<SimulatorClipboardCutRequest>>,
    mut requests: MessageWriter<SimulatorClipboardRequest>,
) {
    requests.write(SimulatorClipboardRequest {
        view: Some(trigger.event().webview),
        operation: trigger.event().payload.operation(),
    });
}

fn on_clipboard_paste(
    trigger: On<UiInput<SimulatorClipboardPasteRequest>>,
    mut requests: MessageWriter<SimulatorClipboardRequest>,
) {
    requests.write(SimulatorClipboardRequest {
        view: Some(trigger.event().webview),
        operation: trigger.event().payload.operation(),
    });
}

fn on_clipboard_select_all(
    trigger: On<UiInput<SimulatorClipboardSelectAllRequest>>,
    mut requests: MessageWriter<SimulatorClipboardRequest>,
) {
    requests.write(SimulatorClipboardRequest {
        view: Some(trigger.event().webview),
        operation: trigger.event().payload.operation(),
    });
}

fn on_software_keyboard(
    trigger: On<UiInput<SimulatorSoftwareKeyboard>>,
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
    mut inputs: MessageWriter<SimulatorInputRequest>,
) {
    for request in requests.read() {
        inputs.write(SimulatorInputRequest {
            view: request.view,
            operation: SimulatorInputOperation::HardwareButton {
                button: request.button,
            },
        });
    }
}

fn handle_clipboard_requests(
    mut requests: MessageReader<SimulatorClipboardRequest>,
    active: Res<ActiveSimulatorView>,
    attachments: Query<(
        Entity,
        &SimulatorDevice,
        &Axe,
        &HidBroker,
        &DeviceTouchSession,
    )>,
    worker: Res<ClipboardWorker>,
) {
    for request in requests.read() {
        let target = request
            .view
            .filter(|entity| attachments.contains(*entity))
            .or_else(|| active.select(attachments.iter().map(|(entity, ..)| entity)));
        let Some(target) = target else {
            continue;
        };
        let Ok((_, device, axe, hid, touch)) = attachments.get(target) else {
            continue;
        };
        if request.operation == SimulatorClipboardOperation::SelectAll
            && let Some(point) = touch.focus
        {
            hid.dispatch(HidRequest::triple_tap(point));
            continue;
        }
        let job = ClipboardJob {
            axe: axe.path().to_path_buf(),
            udid: device.udid.clone(),
            operation: request.operation,
        };
        if worker.0.send(job).is_err() {
            error!("simulator clipboard worker stopped");
        }
    }
}

fn handle_control_requests(
    mut requests: MessageReader<SimulatorControlRequest>,
    mut responses: MessageWriter<SimulatorControlResponse>,
    mut inputs: MessageWriter<SimulatorInputRequest>,
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
        let result = match &request.input {
            SimulatorInput::Tap { x, y } => match (control_coordinates(points, pixels), hid) {
                (Ok(coordinates), Some(hid)) => {
                    hid.dispatch(HidRequest::tap(coordinates.point((*x, *y))));
                    Ok(format!("tapped simulator at ({x}, {y})"))
                }
                (Err(error), _) => Err(error),
                (_, None) => Err("simulator input is unavailable".to_string()),
            },
            SimulatorInput::Swipe {
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
            SimulatorInput::TypeText(text) => {
                inputs.write(SimulatorInputRequest {
                    view: Some(target),
                    operation: SimulatorInputOperation::Text { text: text.clone() },
                });
                Ok("typed text into simulator".to_string())
            }
            SimulatorInput::Key(keycode) => {
                inputs.write(SimulatorInputRequest {
                    view: Some(target),
                    operation: SimulatorInputOperation::Key {
                        code: u16::from(*keycode),
                    },
                });
                Ok(format!("pressed simulator keycode {keycode}"))
            }
            SimulatorInput::Button(button) => {
                let button = match button {
                    SimulatorButton::Home => HardwareButton::Home,
                    SimulatorButton::Lock => HardwareButton::Lock,
                    SimulatorButton::Siri => HardwareButton::Siri,
                };
                inputs.write(SimulatorInputRequest {
                    view: Some(target),
                    operation: SimulatorInputOperation::HardwareButton { button },
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
    mut requests: MessageReader<SimulatorInputRequest>,
    active: Res<ActiveSimulatorView>,
    attachments: Query<(Entity, &SimulatorKeyboard)>,
) {
    for request in requests.read() {
        let target = request
            .view
            .filter(|entity| attachments.contains(*entity))
            .or_else(|| active.select(attachments.iter().map(|(entity, _)| entity)));
        let Some(target) = target else {
            continue;
        };
        let Ok((_, keyboard)) = attachments.get(target) else {
            continue;
        };
        keyboard.dispatch(request.operation.clone());
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

    #[test]
    fn keyboard_batch_preserves_order_and_quotes_text() {
        let batch = SimulatorKeyboardBatch {
            operations: vec![
                SimulatorInputOperation::Text { text: "a".into() },
                SimulatorInputOperation::Text { text: "\"".into() },
                SimulatorInputOperation::Key { code: 42 },
                SimulatorInputOperation::Text { text: "\\".into() },
            ],
        };

        assert_eq!(
            batch.steps(),
            vec![
                "type \"a\\\"\"".to_string(),
                "key 42".to_string(),
                "type \"\\\\\"".to_string(),
            ]
        );
    }
}
