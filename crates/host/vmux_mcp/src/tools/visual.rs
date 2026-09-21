use super::{DispatchTarget, ToolCall, ToolManifest};
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::{Commands, On};
use serde::Deserialize;
use vmux_client::protocol::{AgentQuery, SimulatorAction, SimulatorButton};

pub(super) struct VisualToolsPlugin;

impl Plugin for VisualToolsPlugin {
    fn build(&self, app: &mut App) {
        let mut tools = ToolManifest::from_ron(include_str!("visual.ron"));
        tools.observe(app, "screenshot", screenshot);
        tools.observe(app, "simulator_screenshot", simulator_screenshot);
        tools.observe(app, "simulator_tap", simulator_tap);
        tools.observe(app, "simulator_swipe", simulator_swipe);
        tools.observe(app, "simulator_type", simulator_type);
        tools.observe(app, "simulator_key", simulator_key);
        tools.observe(app, "simulator_button", simulator_button);
        tools.observe(app, "browser_snapshot", browser_snapshot);
        tools.observe(app, "browser_scroll", browser_scroll);
        tools.observe(app, "record_start", record_start);
        tools.observe(app, "record_stop", record_stop);
        tools.finish();
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

fn screenshot(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: ScreenshotArgs = call.parse("screenshot")?;
        Ok(DispatchTarget::Query(AgentQuery::Screenshot {
            pane: OptionalText::trim(args.pane),
        }))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn simulator_screenshot(trigger: On<ToolCall>, mut commands: Commands) {
    trigger.finish_dispatch(
        &mut commands,
        Ok(DispatchTarget::Query(AgentQuery::SimulatorScreenshot)),
    );
}

fn simulator_tap(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: SimulatorTapArgs = call.parse("simulator_tap")?;
        Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
            action: SimulatorAction::Tap {
                x: args.x,
                y: args.y,
            },
        }))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn simulator_swipe(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
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

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn simulator_type(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: SimulatorTypeArgs = call.parse("simulator_type")?;
        let text = args
            .text
            .filter(|text| !text.is_empty())
            .ok_or("simulator_type.text is empty")?;
        Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
            action: SimulatorAction::TypeText(text),
        }))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn simulator_key(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: SimulatorKeyArgs = call.parse("simulator_key")?;
        let keycode = u8::try_from(args.keycode)
            .map_err(|_| "simulator_key.keycode must be between 0 and 255".to_string())?;
        Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
            action: SimulatorAction::Key(keycode),
        }))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn simulator_button(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: SimulatorButtonArgs = call.parse("simulator_button")?;
        Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
            action: SimulatorAction::Button(args.button.into()),
        }))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn browser_snapshot(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
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

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn browser_scroll(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
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

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn record_start(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: RecordStartArgs = call.parse("record_start")?;
        Ok(DispatchTarget::Query(AgentQuery::RecordStart {
            gif: args.gif,
            max_secs: args.max_secs.unwrap_or(600) as u32,
            pane: OptionalText::trim(args.pane),
        }))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn record_stop(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: RecordStopArgs = call.parse("record_stop")?;
        Ok(DispatchTarget::Query(AgentQuery::RecordStop {
            dir: OptionalText::trim(args.dir),
            name: OptionalText::trim(args.name),
        }))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
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
