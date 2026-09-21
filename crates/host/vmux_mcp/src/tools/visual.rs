use super::{DispatchTarget, ToolCall, ToolDefinition, ToolRegistration};
use bevy_app::{App, Plugin};
use serde::Deserialize;
use vmux_client::protocol::{AgentQuery, SimulatorAction, SimulatorButton};

pub(super) struct VisualToolsPlugin;

impl Plugin for VisualToolsPlugin {
    fn build(&self, app: &mut App) {
        ToolRegistration::from_definition(screenshot_definition()).local(app, screenshot);
        ToolRegistration::from_definition(simulator_screenshot_definition())
            .local(app, simulator_screenshot);
        ToolRegistration::from_definition(simulator_tap_definition()).local(app, simulator_tap);
        ToolRegistration::from_definition(simulator_swipe_definition()).local(app, simulator_swipe);
        ToolRegistration::from_definition(simulator_type_definition()).local(app, simulator_type);
        ToolRegistration::from_definition(simulator_key_definition()).local(app, simulator_key);
        ToolRegistration::from_definition(simulator_button_definition())
            .local(app, simulator_button);
        ToolRegistration::from_definition(browser_snapshot_definition())
            .local(app, browser_snapshot);
        ToolRegistration::from_definition(browser_scroll_definition()).local(app, browser_scroll);
        ToolRegistration::from_definition(record_start_definition()).local(app, record_start);
        ToolRegistration::from_definition(record_stop_definition()).local(app, record_stop);
    }
}

#[derive(Deserialize)]
struct ScreenshotArgs {
    pane: Option<String>,
}

#[derive(Deserialize)]
struct SimulatorTapArgs {
    x: u32,
    y: u32,
}

#[derive(Deserialize)]
struct SimulatorSwipeArgs {
    start_x: u32,
    start_y: u32,
    end_x: u32,
    end_y: u32,
    duration_ms: Option<u64>,
}

#[derive(Deserialize)]
struct SimulatorTypeArgs {
    text: Option<String>,
}

#[derive(Deserialize)]
struct SimulatorKeyArgs {
    keycode: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum SimulatorButtonArg {
    Home,
    Lock,
    Siri,
}

impl From<SimulatorButtonArg> for SimulatorButton {
    fn from(value: SimulatorButtonArg) -> Self {
        match value {
            SimulatorButtonArg::Home => Self::Home,
            SimulatorButtonArg::Lock => Self::Lock,
            SimulatorButtonArg::Siri => Self::Siri,
        }
    }
}

#[derive(Deserialize)]
struct SimulatorButtonArgs {
    button: SimulatorButtonArg,
}

#[derive(Deserialize)]
struct BrowserSnapshotArgs {
    target: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ScrollTarget {
    Top,
    Bottom,
}

impl ScrollTarget {
    fn into_string(self) -> String {
        match self {
            Self::Top => "top".to_string(),
            Self::Bottom => "bottom".to_string(),
        }
    }
}

#[derive(Deserialize)]
struct BrowserScrollArgs {
    to: Option<ScrollTarget>,
    delta: Option<i64>,
    target: Option<String>,
}

#[derive(Deserialize)]
struct RecordStartArgs {
    #[serde(default)]
    gif: bool,
    max_secs: Option<u64>,
    pane: Option<String>,
}

#[derive(Deserialize)]
struct RecordStopArgs {
    dir: Option<String>,
    name: Option<String>,
}

pub(super) fn screenshot(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: ScreenshotArgs = call.parse("screenshot")?;
    Ok(DispatchTarget::Query(AgentQuery::Screenshot {
        pane: OptionalText::trim(args.pane),
    }))
}

pub(super) fn simulator_screenshot(_call: &ToolCall) -> Result<DispatchTarget, String> {
    Ok(DispatchTarget::Query(AgentQuery::SimulatorScreenshot))
}

pub(super) fn simulator_tap(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: SimulatorTapArgs = call.parse("simulator_tap")?;
    Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
        action: SimulatorAction::Tap {
            x: args.x,
            y: args.y,
        },
    }))
}

pub(super) fn simulator_swipe(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: SimulatorSwipeArgs = call.parse("simulator_swipe")?;
    let duration_ms = args.duration_ms.unwrap_or(300);
    if !(1..=10_000).contains(&duration_ms) {
        return Err("simulator_swipe.duration_ms must be between 1 and 10000".to_string());
    }
    Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
        action: SimulatorAction::Swipe {
            start_x: args.start_x,
            start_y: args.start_y,
            end_x: args.end_x,
            end_y: args.end_y,
            duration_ms: duration_ms as u32,
        },
    }))
}

pub(super) fn simulator_type(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: SimulatorTypeArgs = call.parse("simulator_type")?;
    let text = args
        .text
        .filter(|text| !text.is_empty())
        .ok_or("simulator_type.text is empty")?;
    Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
        action: SimulatorAction::TypeText(text),
    }))
}

pub(super) fn simulator_key(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: SimulatorKeyArgs = call.parse("simulator_key")?;
    let keycode = u8::try_from(args.keycode)
        .map_err(|_| "simulator_key.keycode must be between 0 and 255".to_string())?;
    Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
        action: SimulatorAction::Key(keycode),
    }))
}

pub(super) fn simulator_button(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: SimulatorButtonArgs = call.parse("simulator_button")?;
    Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
        action: SimulatorAction::Button(args.button.into()),
    }))
}

pub(super) fn browser_snapshot(call: &ToolCall) -> Result<DispatchTarget, String> {
    if call
        .arguments
        .get("target")
        .is_some_and(|value| !value.is_null() && !value.is_string())
    {
        return Err("browser_snapshot.target must be a string".to_string());
    }
    let args: BrowserSnapshotArgs = call.parse("browser_snapshot")?;
    Ok(DispatchTarget::Query(AgentQuery::BrowserSnapshot {
        pane: OptionalText::trim(args.target),
        anchor: call.anchor,
    }))
}

pub(super) fn browser_scroll(call: &ToolCall) -> Result<DispatchTarget, String> {
    if call
        .arguments
        .get("delta")
        .is_some_and(|value| !value.is_null() && value.as_i64().is_none())
    {
        return Err("browser_scroll.delta must be an integer".to_string());
    }
    let args: BrowserScrollArgs = call.parse("browser_scroll")?;
    let delta = args
        .delta
        .map(i32::try_from)
        .transpose()
        .map_err(|_| "browser_scroll.delta is out of range".to_string())?;
    let to = args.to.map(ScrollTarget::into_string);
    if to.is_some() == delta.is_some() {
        return Err("browser_scroll requires exactly one of `to` or `delta`".to_string());
    }
    Ok(DispatchTarget::Query(AgentQuery::BrowserScroll {
        pane: OptionalText::trim(args.target),
        to,
        delta,
        anchor: call.anchor,
    }))
}

pub(super) fn record_start(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: RecordStartArgs = call.parse("record_start")?;
    Ok(DispatchTarget::Query(AgentQuery::RecordStart {
        gif: args.gif,
        max_secs: args.max_secs.unwrap_or(600) as u32,
        pane: OptionalText::trim(args.pane),
    }))
}

pub(super) fn record_stop(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: RecordStopArgs = call.parse("record_stop")?;
    Ok(DispatchTarget::Query(AgentQuery::RecordStop {
        dir: OptionalText::trim(args.dir),
        name: OptionalText::trim(args.name),
    }))
}

struct OptionalText;

impl OptionalText {
    fn trim(value: Option<String>) -> Option<String> {
        value.and_then(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        })
    }
}

pub(super) fn screenshot_definition() -> ToolDefinition {
    ToolDefinition {
        name: "screenshot".into(),
        description: "Capture the vmux window as a PNG and return it inline so you can SEE the current UI \
(use it to verify your own UI changes). Captures the whole window exactly as it appears on screen - all \
visible panes (browser, terminal, editor) and layout chrome. Optionally pass `pane` (a pane:<id> or \
stack:<id> from read_layout) to crop to just that region. The full-resolution image is saved under \
the active vmux profile's recording directory and a downscaled copy is returned inline. macOS only; the first call may prompt for \
Screen Recording permission - grant it in System Settings > Privacy & Security > Screen Recording, then \
call again."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "pane": {
                    "type": "string",
                    "description": "Optional pane:<id> or stack:<id> to crop to; whole window if omitted."
                }
            }
        }),
    }
}

pub(super) fn simulator_screenshot_definition() -> ToolDefinition {
    ToolDefinition {
        name: "simulator_screenshot".into(),
        description: "Capture the attached iOS Simulator screen and return it inline. Use the returned pixel dimensions and image coordinates with simulator_tap and simulator_swipe."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {}
        }),
    }
}

pub(super) fn simulator_tap_definition() -> ToolDefinition {
    ToolDefinition {
        name: "simulator_tap".into(),
        description:
            "Tap the attached iOS Simulator at x,y pixel coordinates from simulator_screenshot."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["x", "y"],
            "additionalProperties": false,
            "properties": {
                "x": {"type": "integer", "minimum": 0},
                "y": {"type": "integer", "minimum": 0}
            }
        }),
    }
}

pub(super) fn simulator_swipe_definition() -> ToolDefinition {
    ToolDefinition {
        name: "simulator_swipe".into(),
        description: "Swipe the attached iOS Simulator between pixel coordinates from simulator_screenshot. duration_ms defaults to 300."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["start_x", "start_y", "end_x", "end_y"],
            "additionalProperties": false,
            "properties": {
                "start_x": {"type": "integer", "minimum": 0},
                "start_y": {"type": "integer", "minimum": 0},
                "end_x": {"type": "integer", "minimum": 0},
                "end_y": {"type": "integer", "minimum": 0},
                "duration_ms": {"type": "integer", "minimum": 1, "maximum": 10000}
            }
        }),
    }
}

pub(super) fn simulator_type_definition() -> ToolDefinition {
    ToolDefinition {
        name: "simulator_type".into(),
        description: "Type text into the focused control in the attached iOS Simulator.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["text"],
            "additionalProperties": false,
            "properties": {
                "text": {"type": "string", "minLength": 1}
            }
        }),
    }
}

pub(super) fn simulator_key_definition() -> ToolDefinition {
    ToolDefinition {
        name: "simulator_key".into(),
        description: "Press one HID keycode in the attached iOS Simulator. Common codes: Enter 40, Backspace 42, Tab 43, Space 44."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["keycode"],
            "additionalProperties": false,
            "properties": {
                "keycode": {"type": "integer", "minimum": 0, "maximum": 255}
            }
        }),
    }
}

pub(super) fn simulator_button_definition() -> ToolDefinition {
    ToolDefinition {
        name: "simulator_button".into(),
        description: "Press a hardware button on the attached iOS Simulator.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["button"],
            "additionalProperties": false,
            "properties": {
                "button": {"enum": ["home", "lock", "siri"]}
            }
        }),
    }
}

pub(super) fn browser_snapshot_definition() -> ToolDefinition {
    ToolDefinition {
        name: "browser_snapshot".into(),
        description:
            "Read the current page's DOM as a compact semantic snapshot. Returns JSON with \
the page url/title and a list of interactive elements, each with a stable `ref`, `role`, `name`, \
`value`, `bbox` ([x,y,w,h] in CSS px), and `state` flags. Use the `ref` values to target later \
interaction tools. Pass `target` = a pane:<id> or stack:<id> from read_layout to pick a \
specific page; defaults to the focused page."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Optional pane:<id> or stack:<id>; if omitted, an agent caller's own browser pane (resolved via anchor), else the focused page."
                }
            }
        }),
    }
}

pub(super) fn browser_scroll_definition() -> ToolDefinition {
    ToolDefinition {
        name: "browser_scroll".into(),
        description:
            "Scroll the visible browser page so the user can watch, then return the post-scroll \
snapshot (same shape as browser_snapshot, including viewport + inViewport flags). Pass exactly one \
of `to` (\"top\" or \"bottom\") or `delta` (pixels; positive = down, e.g. one screen is about the \
snapshot's viewport.height). Pass `target` = pane:<id> or stack:<id> to pick a page; defaults to \
the focused page. Prefer scrolling to read long pages instead of assuming off-screen content."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "to": {"enum": ["top", "bottom"], "description": "Scroll to page top or bottom. Pass exactly one of `to` or `delta`."},
                "delta": {
                    "type": "integer",
                    "minimum": i32::MIN,
                    "maximum": i32::MAX,
                    "description": "Scroll by pixels; positive = down. Pass exactly one of `to` or `delta`."
                },
                "target": {"type": "string", "description": "Optional pane:<id> or stack:<id>; if omitted, an agent caller's own browser pane (resolved via anchor), else the focused page."}
            }
        }),
    }
}

pub(super) fn record_start_definition() -> ToolDefinition {
    ToolDefinition {
        name: "record_start".into(),
        description: "Start recording the vmux window to an mp4 video (optionally also a GIF). \
Returns immediately so you can drive the UI with other tools to demonstrate a feature, then call \
record_stop. Record in ONE live take: start, perform the few actions you want to show, then \
stop. Do NOT rehearse, build elaborate layouts, or take screenshots to verify - just capture the \
live interaction in a single pass. Auto-stops after `max_secs` (default 600) as a safety cap. Only \
one recording at a time. macOS only; the first call may prompt for Screen Recording permission - \
grant it in System Settings > Privacy & Security > Screen Recording, then call again."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "gif": {"type": "boolean", "description": "Also emit a GIF next to the mp4 (default false)."},
                "max_secs": {"type": "integer", "description": "Auto-stop cap in seconds (default 600)."},
                "pane": {"type": "string", "description": "Optional pane:<id> or stack:<id> to crop to; whole window if omitted."}
            }
        }),
    }
}

pub(super) fn record_stop_definition() -> ToolDefinition {
    ToolDefinition {
        name: "record_stop".into(),
        description: "Stop the active recording and write the file(s). Returns the mp4 path, duration, \
and size (plus the GIF path if one was requested). By default saves to the active vmux profile's recording directory; pass `dir` \
(absolute) and `name` (basename, no extension) to save elsewhere - e.g. dir=<repo>/docs/recording, \
name=<feature> to drop a demo straight into the repo."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "dir": {"type": "string", "description": "Absolute output directory (default: active vmux profile recording directory)."},
                "name": {"type": "string", "description": "Output basename without extension (default vmux-<timestamp>)."}
            }
        }),
    }
}
