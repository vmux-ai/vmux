use super::device::{Axe, SimulatorDevice};
use super::hid::{HidBroker, HidRequest};
use super::{
    ActiveSimulatorView, DevicePixels, DevicePoints, HardwareButtonRequest, SimulatorButton,
    SimulatorButtonPressRequest, SimulatorClipboardRequest, SimulatorControlResponse,
    SimulatorInputSet, SimulatorKeyPressRequest, SimulatorSoftwareKeyboardRequest,
    SimulatorSwipeRequest, SimulatorTapRequest, SimulatorTypeTextRequest,
};
use crate::event::{
    HardwareButton, SimulatorClipboardCopyRequest, SimulatorClipboardCutRequest,
    SimulatorClipboardOperation, SimulatorClipboardPasteRequest,
    SimulatorClipboardSelectAllRequest, SimulatorInputHardwareButtonRequest,
    SimulatorInputKeyRequest, SimulatorInputModifiedKeyRequest, SimulatorInputTextRequest,
    SimulatorKeyModifiers, SimulatorSoftwareKeyboard, SimulatorTouch, SimulatorTouchPhase,
};
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use vmux_core::input::{
    ConsumesNativeKey, NativeKey, NativeKeyClaimSet, NativeKeyInput, NativeKeyInputSet,
    PassesNativeKey,
};
use vmux_core::{Active, KeyModifiers};

pub(super) struct SimulatorInputPlugin;

impl Plugin for SimulatorInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SimulatorInputRequest>()
            .add_message::<SimulatorClipboardInputRequest>()
            .add_plugins(SimulatorNativeKeyPlugin)
            .add_systems(
                Update,
                (
                    handle_button_requests,
                    handle_clipboard_requests,
                    handle_tap_requests,
                    handle_swipe_requests,
                    handle_type_text_requests,
                    handle_key_press_requests,
                    handle_button_press_requests,
                    send_clipboard_requests,
                    send_key_requests,
                )
                    .chain()
                    .in_set(SimulatorInputSet),
            )
            .add_plugins(UiEventPlugin::<(
                SimulatorTouch,
                SimulatorSoftwareKeyboard,
                SimulatorInputTextRequest,
                SimulatorInputKeyRequest,
                SimulatorInputModifiedKeyRequest,
                SimulatorInputHardwareButtonRequest,
            )>::default())
            .add_plugins(UiEventPlugin::<(
                SimulatorClipboardCopyRequest,
                SimulatorClipboardCutRequest,
                SimulatorClipboardPasteRequest,
                SimulatorClipboardSelectAllRequest,
            )>::default())
            .add_observer(touch)
            .add_observer(text)
            .add_observer(key)
            .add_observer(modified_key)
            .add_observer(hardware_button)
            .add_observer(clipboard_copy)
            .add_observer(clipboard_cut)
            .add_observer(clipboard_paste)
            .add_observer(clipboard_select_all)
            .add_observer(software_keyboard);
    }
}

struct SimulatorNativeKeyPlugin;

impl Plugin for SimulatorNativeKeyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<NativeKeyInput>()
            .add_systems(Startup, spawn_native_key_bindings)
            .add_systems(
                Update,
                (sync_native_key_bindings, ApplyDeferred)
                    .chain()
                    .in_set(NativeKeyClaimSet),
            )
            .add_systems(
                Update,
                handle_native_key
                    .after(NativeKeyInputSet)
                    .before(SimulatorInputSet),
            );
    }
}

#[derive(Component)]
struct SimulatorNativeKey;

#[derive(Component)]
struct SimulatorHardwareKey(HardwareButton);

#[derive(Component)]
struct SimulatorClipboardKey(SimulatorClipboardOperation);

#[derive(Component)]
struct SimulatorSoftwareKeyboardKey;

type SimulatorNativeKeyBindings<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static SimulatorHardwareKey>,
        Option<&'static SimulatorClipboardKey>,
        Has<SimulatorSoftwareKeyboardKey>,
    ),
    With<Active>,
>;

fn spawn_native_key_bindings(mut commands: Commands) {
    let command = KeyModifiers {
        super_key: true,
        ..default()
    };
    for (key, button) in [
        (KeyCode::KeyH, HardwareButton::Home),
        (KeyCode::KeyL, HardwareButton::Lock),
        (KeyCode::KeyS, HardwareButton::Siri),
    ] {
        commands.spawn((
            Name::new(format!("Simulator {button:?} key")),
            SimulatorNativeKey,
            SimulatorHardwareKey(button),
            NativeKey {
                key,
                modifiers: command,
            },
            ConsumesNativeKey,
        ));
    }
    for (key, operation) in [
        (KeyCode::KeyA, SimulatorClipboardOperation::SelectAll),
        (KeyCode::KeyC, SimulatorClipboardOperation::Copy),
        (KeyCode::KeyX, SimulatorClipboardOperation::Cut),
        (KeyCode::KeyV, SimulatorClipboardOperation::Paste),
    ] {
        commands.spawn((
            Name::new(format!("Simulator {operation:?} key")),
            SimulatorNativeKey,
            SimulatorClipboardKey(operation),
            NativeKey {
                key,
                modifiers: command,
            },
            ConsumesNativeKey,
        ));
    }
    commands.spawn((
        Name::new("Simulator software keyboard key"),
        SimulatorNativeKey,
        SimulatorSoftwareKeyboardKey,
        NativeKey {
            key: KeyCode::KeyK,
            modifiers: command,
        },
        ConsumesNativeKey,
    ));
    for key in [
        KeyCode::KeyZ,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::Backspace,
        KeyCode::Delete,
    ] {
        for shift in [false, true] {
            commands.spawn((
                Name::new(format!("Simulator command {key:?} pass-through")),
                SimulatorNativeKey,
                NativeKey {
                    key,
                    modifiers: KeyModifiers { shift, ..command },
                },
                PassesNativeKey,
            ));
        }
    }
    for key in [
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::Backspace,
        KeyCode::Delete,
    ] {
        for shift in [false, true] {
            commands.spawn((
                Name::new(format!("Simulator option {key:?} pass-through")),
                SimulatorNativeKey,
                NativeKey {
                    key,
                    modifiers: KeyModifiers {
                        shift,
                        alt: true,
                        ..default()
                    },
                },
                PassesNativeKey,
            ));
        }
    }
}

fn sync_native_key_bindings(
    simulator: Query<(), With<ActiveSimulatorView>>,
    bindings: Query<(Entity, Has<Active>), With<SimulatorNativeKey>>,
    mut commands: Commands,
) {
    let enabled = !simulator.is_empty();
    for (entity, active) in &bindings {
        if active == enabled {
            continue;
        }
        if enabled {
            commands.entity(entity).insert(Active);
        } else {
            commands.entity(entity).remove::<Active>();
        }
    }
}

fn handle_native_key(
    mut inputs: MessageReader<NativeKeyInput>,
    bindings: SimulatorNativeKeyBindings,
    mut buttons: MessageWriter<HardwareButtonRequest>,
    mut clipboard: MessageWriter<SimulatorClipboardRequest>,
    mut keyboard: MessageWriter<SimulatorSoftwareKeyboardRequest>,
) {
    for input in inputs.read() {
        let Some(claim) = input.claim else { continue };
        let Ok((button, operation, software_keyboard)) = bindings.get(claim) else {
            continue;
        };
        if let Some(button) = button {
            buttons.write(HardwareButtonRequest {
                view: None,
                button: button.0,
            });
        }
        if let Some(operation) = operation {
            clipboard.write(SimulatorClipboardRequest {
                view: None,
                operation: operation.0,
            });
        }
        if software_keyboard {
            keyboard.write(SimulatorSoftwareKeyboardRequest { view: None });
        }
    }
}

#[derive(Message)]
struct SimulatorInputRequest {
    view: Option<Entity>,
    input: SimulatorKeyboardInput,
}

#[derive(Message)]
struct SimulatorClipboardInputRequest {
    view: Option<Entity>,
    input: SimulatorClipboardInput,
}

#[derive(Clone)]
enum SimulatorKeyboardInput {
    Text(String),
    Key(u16),
    ModifiedKey {
        code: u16,
        modifiers: SimulatorKeyModifiers,
    },
    HardwareButton(HardwareButton),
}

#[derive(Component)]
pub(super) struct SimulatorKeyboard {
    sender: mpsc::Sender<SimulatorKeyboardInput>,
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

    fn dispatch(&self, input: SimulatorKeyboardInput) {
        if self.sender.send(input).is_err() {
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

    fn run(&self, receiver: mpsc::Receiver<SimulatorKeyboardInput>) {
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
    inputs: Vec<SimulatorKeyboardInput>,
}

impl SimulatorKeyboardBatch {
    fn from_receiver(
        first: SimulatorKeyboardInput,
        receiver: &mpsc::Receiver<SimulatorKeyboardInput>,
    ) -> Self {
        let mut inputs = vec![first];
        while inputs.len() < SimulatorKeyboardRunner::MAX_BATCH_KEYS {
            let Ok(input) = receiver.try_recv() else {
                break;
            };
            inputs.push(input);
        }
        Self { inputs }
    }

    fn steps(self) -> Vec<String> {
        let mut steps = Vec::new();
        let mut text = String::new();
        for input in self.inputs {
            match input {
                SimulatorKeyboardInput::Text(value) => text.push_str(&value),
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

    fn step(input: SimulatorKeyboardInput) -> String {
        match input {
            SimulatorKeyboardInput::Text(text) => format!("type {}", Self::quote(&text)),
            SimulatorKeyboardInput::Key(code) => format!("key {code}"),
            SimulatorKeyboardInput::ModifiedKey { code, modifiers } => {
                let modifiers = modifiers
                    .hid_codes()
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("key-combo --modifiers {modifiers} --key {code}")
            }
            SimulatorKeyboardInput::HardwareButton(button) => {
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

#[derive(Component)]
pub(super) struct SimulatorClipboard {
    sender: mpsc::Sender<SimulatorClipboardInput>,
}

impl SimulatorClipboard {
    pub fn start(axe: &Axe, device: &SimulatorDevice) -> io::Result<Self> {
        let runner = SimulatorClipboardRunner {
            axe: axe.path().to_path_buf(),
            udid: device.udid.clone(),
        };
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("vmux-simulator-clipboard".into())
            .spawn(move || runner.run(receiver))?;
        Ok(Self { sender })
    }

    fn dispatch(&self, input: SimulatorClipboardInput) {
        if self.sender.send(input).is_err() {
            error!("simulator clipboard worker stopped");
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SimulatorClipboardInput {
    Copy,
    Cut,
    Paste,
    SelectAll,
}

struct SimulatorClipboardRunner {
    axe: PathBuf,
    udid: String,
}

impl SimulatorClipboardRunner {
    fn run(&self, receiver: mpsc::Receiver<SimulatorClipboardInput>) {
        for input in receiver {
            if let Err(error) = self.execute(input) {
                error!("simulator clipboard failed: {error}");
            }
        }
    }

    fn execute(&self, input: SimulatorClipboardInput) -> Result<(), String> {
        match input {
            SimulatorClipboardInput::Copy => {
                self.key_combo(6)?;
                self.sync(&self.udid, "host")
            }
            SimulatorClipboardInput::Cut => {
                self.key_combo(27)?;
                self.sync(&self.udid, "host")
            }
            SimulatorClipboardInput::Paste => {
                self.sync("host", &self.udid)?;
                self.key_combo(25)
            }
            SimulatorClipboardInput::SelectAll => self.key_combo(4),
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

fn touch(
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

fn text(
    trigger: On<UiInput<SimulatorInputTextRequest>>,
    mut requests: MessageWriter<SimulatorInputRequest>,
) {
    requests.write(SimulatorInputRequest {
        view: Some(trigger.event().webview),
        input: SimulatorKeyboardInput::Text(trigger.event().payload.text.clone()),
    });
}

fn key(
    trigger: On<UiInput<SimulatorInputKeyRequest>>,
    mut requests: MessageWriter<SimulatorInputRequest>,
) {
    requests.write(SimulatorInputRequest {
        view: Some(trigger.event().webview),
        input: SimulatorKeyboardInput::Key(trigger.event().payload.code),
    });
}

fn modified_key(
    trigger: On<UiInput<SimulatorInputModifiedKeyRequest>>,
    mut requests: MessageWriter<SimulatorInputRequest>,
) {
    requests.write(SimulatorInputRequest {
        view: Some(trigger.event().webview),
        input: SimulatorKeyboardInput::ModifiedKey {
            code: trigger.event().payload.code,
            modifiers: trigger.event().payload.modifiers,
        },
    });
}

fn hardware_button(
    trigger: On<UiInput<SimulatorInputHardwareButtonRequest>>,
    mut requests: MessageWriter<SimulatorInputRequest>,
) {
    requests.write(SimulatorInputRequest {
        view: Some(trigger.event().webview),
        input: SimulatorKeyboardInput::HardwareButton(trigger.event().payload.button),
    });
}

fn clipboard_copy(
    trigger: On<UiInput<SimulatorClipboardCopyRequest>>,
    mut requests: MessageWriter<SimulatorClipboardInputRequest>,
) {
    requests.write(SimulatorClipboardInputRequest {
        view: Some(trigger.event().webview),
        input: SimulatorClipboardInput::Copy,
    });
}

fn clipboard_cut(
    trigger: On<UiInput<SimulatorClipboardCutRequest>>,
    mut requests: MessageWriter<SimulatorClipboardInputRequest>,
) {
    requests.write(SimulatorClipboardInputRequest {
        view: Some(trigger.event().webview),
        input: SimulatorClipboardInput::Cut,
    });
}

fn clipboard_paste(
    trigger: On<UiInput<SimulatorClipboardPasteRequest>>,
    mut requests: MessageWriter<SimulatorClipboardInputRequest>,
) {
    requests.write(SimulatorClipboardInputRequest {
        view: Some(trigger.event().webview),
        input: SimulatorClipboardInput::Paste,
    });
}

fn clipboard_select_all(
    trigger: On<UiInput<SimulatorClipboardSelectAllRequest>>,
    mut requests: MessageWriter<SimulatorClipboardInputRequest>,
) {
    requests.write(SimulatorClipboardInputRequest {
        view: Some(trigger.event().webview),
        input: SimulatorClipboardInput::SelectAll,
    });
}

fn software_keyboard(
    trigger: On<UiInput<SimulatorSoftwareKeyboard>>,
    mut requests: MessageWriter<SimulatorSoftwareKeyboardRequest>,
) {
    requests.write(SimulatorSoftwareKeyboardRequest {
        view: Some(trigger.event().webview),
    });
}

fn handle_button_requests(
    mut requests: MessageReader<HardwareButtonRequest>,
    mut inputs: MessageWriter<SimulatorInputRequest>,
) {
    for request in requests.read() {
        inputs.write(SimulatorInputRequest {
            view: request.view,
            input: SimulatorKeyboardInput::HardwareButton(request.button),
        });
    }
}

fn handle_clipboard_requests(
    mut requests: MessageReader<SimulatorClipboardRequest>,
    mut inputs: MessageWriter<SimulatorClipboardInputRequest>,
) {
    for request in requests.read() {
        let input = match request.operation {
            SimulatorClipboardOperation::Copy => SimulatorClipboardInput::Copy,
            SimulatorClipboardOperation::Cut => SimulatorClipboardInput::Cut,
            SimulatorClipboardOperation::Paste => SimulatorClipboardInput::Paste,
            SimulatorClipboardOperation::SelectAll => SimulatorClipboardInput::SelectAll,
        };
        inputs.write(SimulatorClipboardInputRequest {
            view: request.view,
            input,
        });
    }
}

fn handle_tap_requests(
    mut requests: MessageReader<SimulatorTapRequest>,
    mut responses: MessageWriter<SimulatorControlResponse>,
    active: Query<Entity, With<ActiveSimulatorView>>,
    attachments: ControlAttachments,
) {
    let active = active.iter().next();
    for request in requests.read() {
        let target =
            ActiveSimulatorView::select(active, attachments.iter().map(|(entity, ..)| entity));
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
        let result = match (control_coordinates(points, pixels), hid) {
            (Ok(coordinates), Some(hid)) => {
                hid.dispatch(HidRequest::tap(coordinates.point((request.x, request.y))));
                Ok(format!(
                    "tapped simulator at ({}, {})",
                    request.x, request.y
                ))
            }
            (Err(error), _) => Err(error),
            (_, None) => Err("simulator input is unavailable".to_string()),
        };
        responses.write(SimulatorControlResponse {
            request_id: request.request_id,
            result,
        });
    }
}

fn handle_swipe_requests(
    mut requests: MessageReader<SimulatorSwipeRequest>,
    mut responses: MessageWriter<SimulatorControlResponse>,
    active: Query<Entity, With<ActiveSimulatorView>>,
    attachments: ControlAttachments,
) {
    let active = active.iter().next();
    for request in requests.read() {
        let target =
            ActiveSimulatorView::select(active, attachments.iter().map(|(entity, ..)| entity));
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
        let result = match (control_coordinates(points, pixels), hid) {
            (Ok(coordinates), Some(hid)) => {
                let from = coordinates.point((request.start_x, request.start_y));
                let to = coordinates.point((request.end_x, request.end_y));
                hid.dispatch(HidRequest::swipe(from, to, request.duration_ms));
                Ok(format!(
                    "swiped simulator from ({}, {}) to ({}, {})",
                    request.start_x, request.start_y, request.end_x, request.end_y
                ))
            }
            (Err(error), _) => Err(error),
            (_, None) => Err("simulator input is unavailable".to_string()),
        };
        responses.write(SimulatorControlResponse {
            request_id: request.request_id,
            result,
        });
    }
}

fn handle_type_text_requests(
    mut requests: MessageReader<SimulatorTypeTextRequest>,
    mut responses: MessageWriter<SimulatorControlResponse>,
    mut inputs: MessageWriter<SimulatorInputRequest>,
    active: Query<Entity, With<ActiveSimulatorView>>,
    attachments: ControlAttachments,
) {
    let active = active.iter().next();
    for request in requests.read() {
        let target =
            ActiveSimulatorView::select(active, attachments.iter().map(|(entity, ..)| entity));
        let Some(target) = target else {
            responses.write(SimulatorControlResponse {
                request_id: request.request_id,
                result: Err("no iOS Simulator is attached".to_string()),
            });
            continue;
        };
        inputs.write(SimulatorInputRequest {
            view: Some(target),
            input: SimulatorKeyboardInput::Text(request.text.clone()),
        });
        responses.write(SimulatorControlResponse {
            request_id: request.request_id,
            result: Ok("typed text into simulator".to_string()),
        });
    }
}

fn handle_key_press_requests(
    mut requests: MessageReader<SimulatorKeyPressRequest>,
    mut responses: MessageWriter<SimulatorControlResponse>,
    mut inputs: MessageWriter<SimulatorInputRequest>,
    active: Query<Entity, With<ActiveSimulatorView>>,
    attachments: ControlAttachments,
) {
    let active = active.iter().next();
    for request in requests.read() {
        let target =
            ActiveSimulatorView::select(active, attachments.iter().map(|(entity, ..)| entity));
        let Some(target) = target else {
            responses.write(SimulatorControlResponse {
                request_id: request.request_id,
                result: Err("no iOS Simulator is attached".to_string()),
            });
            continue;
        };
        inputs.write(SimulatorInputRequest {
            view: Some(target),
            input: SimulatorKeyboardInput::Key(u16::from(request.keycode)),
        });
        responses.write(SimulatorControlResponse {
            request_id: request.request_id,
            result: Ok(format!("pressed simulator keycode {}", request.keycode)),
        });
    }
}

fn handle_button_press_requests(
    mut requests: MessageReader<SimulatorButtonPressRequest>,
    mut responses: MessageWriter<SimulatorControlResponse>,
    mut inputs: MessageWriter<SimulatorInputRequest>,
    active: Query<Entity, With<ActiveSimulatorView>>,
    attachments: ControlAttachments,
) {
    let active = active.iter().next();
    for request in requests.read() {
        let target =
            ActiveSimulatorView::select(active, attachments.iter().map(|(entity, ..)| entity));
        let Some(target) = target else {
            responses.write(SimulatorControlResponse {
                request_id: request.request_id,
                result: Err("no iOS Simulator is attached".to_string()),
            });
            continue;
        };
        let button = match request.button {
            SimulatorButton::Home => HardwareButton::Home,
            SimulatorButton::Lock => HardwareButton::Lock,
            SimulatorButton::Siri => HardwareButton::Siri,
        };
        inputs.write(SimulatorInputRequest {
            view: Some(target),
            input: SimulatorKeyboardInput::HardwareButton(button),
        });
        responses.write(SimulatorControlResponse {
            request_id: request.request_id,
            result: Ok("pressed simulator hardware button".to_string()),
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

fn send_clipboard_requests(
    mut requests: MessageReader<SimulatorClipboardInputRequest>,
    active: Query<Entity, With<ActiveSimulatorView>>,
    attachments: Query<(Entity, &SimulatorClipboard, &HidBroker, &DeviceTouchSession)>,
) {
    let active = active.iter().next();
    for request in requests.read() {
        let target = request
            .view
            .filter(|entity| attachments.contains(*entity))
            .or_else(|| {
                ActiveSimulatorView::select(active, attachments.iter().map(|(entity, ..)| entity))
            });
        let Some(target) = target else {
            continue;
        };
        let Ok((_, clipboard, hid, touch)) = attachments.get(target) else {
            continue;
        };
        if request.input == SimulatorClipboardInput::SelectAll
            && let Some(point) = touch.focus
        {
            hid.dispatch(HidRequest::triple_tap(point));
            continue;
        }
        clipboard.dispatch(request.input);
    }
}

fn send_key_requests(
    mut requests: MessageReader<SimulatorInputRequest>,
    active: Query<Entity, With<ActiveSimulatorView>>,
    attachments: Query<(Entity, &SimulatorKeyboard)>,
) {
    let active = active.iter().next();
    for request in requests.read() {
        let target = request
            .view
            .filter(|entity| attachments.contains(*entity))
            .or_else(|| {
                ActiveSimulatorView::select(active, attachments.iter().map(|(entity, _)| entity))
            });
        let Some(target) = target else {
            continue;
        };
        let Ok((_, keyboard)) = attachments.get(target) else {
            continue;
        };
        keyboard.dispatch(request.input.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;

    const POINTS: (f32, f32) = (402.0, 874.0);

    struct NativeKeyHarness {
        app: App,
    }

    impl NativeKeyHarness {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<HardwareButtonRequest>()
                .add_message::<SimulatorClipboardRequest>()
                .add_message::<SimulatorSoftwareKeyboardRequest>()
                .add_plugins(SimulatorNativeKeyPlugin);
            app.world_mut().spawn(ActiveSimulatorView);
            app.update();
            Self { app }
        }

        fn claim<T: Component>(&mut self, key: KeyCode) -> Entity {
            let mut query = self
                .app
                .world_mut()
                .query_filtered::<(Entity, &NativeKey), (With<T>, With<Active>)>();
            query
                .iter(self.app.world())
                .find_map(|(entity, binding)| (binding.key == key).then_some(entity))
                .expect("native key binding")
        }

        fn press(&mut self, claim: Entity, key: KeyCode) {
            self.app
                .world_mut()
                .resource_mut::<Messages<NativeKeyInput>>()
                .write(NativeKeyInput {
                    key: Some(key),
                    native_code: 0,
                    text: String::new(),
                    modifiers: KeyModifiers {
                        super_key: true,
                        ..default()
                    },
                    repeat: false,
                    captured: false,
                    claim: Some(claim),
                    pressed_at_ms: 0,
                });
            self.app.update();
        }
    }

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
            inputs: vec![
                SimulatorKeyboardInput::Text("a".into()),
                SimulatorKeyboardInput::Text("\"".into()),
                SimulatorKeyboardInput::Key(42),
                SimulatorKeyboardInput::Text("\\".into()),
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

    #[test]
    fn native_hardware_key_is_dispatched_by_its_binding_entity() {
        let mut harness = NativeKeyHarness::new();
        let claim = harness.claim::<SimulatorHardwareKey>(KeyCode::KeyH);

        harness.press(claim, KeyCode::KeyH);

        let requests = harness
            .app
            .world_mut()
            .resource_mut::<Messages<HardwareButtonRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(
            requests,
            vec![HardwareButtonRequest {
                view: None,
                button: HardwareButton::Home,
            }]
        );
    }
}
