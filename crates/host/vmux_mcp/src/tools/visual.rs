use super::{
    DispatchTarget, ToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentQuery, SimulatorAction, SimulatorButton};

pub(super) struct VisualToolPlugin;

impl Plugin for VisualToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Visual))
            .add_systems(
                Update,
                (
                    screenshot,
                    simulator_screenshot,
                    simulator_tap,
                    simulator_swipe,
                    simulator_type,
                    simulator_key,
                    simulator_button,
                    record_start,
                    record_stop,
                )
                    .in_set(ToolDispatchSet),
            );
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum VisualTool {
    Screenshot,
    SimulatorScreenshot,
    SimulatorTap,
    SimulatorSwipe,
    SimulatorType,
    SimulatorKey,
    SimulatorButton,
    RecordStart,
    RecordStop,
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<VisualTool>::from_ron(include_str!("visual.ron"));
    tools.spawn_manifest(manifest);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenshotArgs {
    pane: Option<String>,
}

impl ScreenshotArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("screenshot")?;
        Ok(DispatchTarget::Query(AgentQuery::Screenshot {
            pane: OptionalText::trim(args.pane),
        }))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorTapArgs {
    x: u32,
    y: u32,
}

impl SimulatorTapArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("simulator_tap")?;
        Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
            action: SimulatorAction::Tap {
                x: args.x,
                y: args.y,
            },
        }))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorSwipeArgs {
    start_x: u32,
    start_y: u32,
    end_x: u32,
    end_y: u32,
    duration_ms: Option<u32>,
}

impl SimulatorSwipeArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("simulator_swipe")?;
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
                duration_ms,
            },
        }))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorTypeArgs {
    text: String,
}

impl SimulatorTypeArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("simulator_type")?;
        if args.text.is_empty() {
            return Err("simulator_type.text is empty".to_string());
        }
        Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
            action: SimulatorAction::TypeText(args.text),
        }))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorKeyArgs {
    keycode: u8,
}

impl SimulatorKeyArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("simulator_key")?;
        Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
            action: SimulatorAction::Key(args.keycode),
        }))
    }
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
#[serde(deny_unknown_fields)]
struct SimulatorButtonArgs {
    button: SimulatorButtonArg,
}

impl SimulatorButtonArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("simulator_button")?;
        Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
            action: SimulatorAction::Button(args.button.into()),
        }))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordStartArgs {
    #[serde(default)]
    gif: bool,
    max_secs: Option<u32>,
    pane: Option<String>,
}

impl RecordStartArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("record_start")?;
        Ok(DispatchTarget::Query(AgentQuery::RecordStart {
            gif: args.gif,
            max_secs: args.max_secs.unwrap_or(600),
            pane: OptionalText::trim(args.pane),
        }))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordStopArgs {
    dir: Option<String>,
    name: Option<String>,
}

impl RecordStopArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("record_stop")?;
        Ok(DispatchTarget::Query(AgentQuery::RecordStop {
            dir: OptionalText::trim(args.dir),
            name: OptionalText::trim(args.name),
        }))
    }
}

fn screenshot(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::Screenshot) {
        call.finish_dispatch(request, &mut commands, ScreenshotArgs::target(call));
    }
}

fn simulator_screenshot(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::SimulatorScreenshot) {
        call.finish_dispatch(
            request,
            &mut commands,
            Ok(DispatchTarget::Query(AgentQuery::SimulatorScreenshot)),
        );
    }
}

fn simulator_tap(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::SimulatorTap) {
        call.finish_dispatch(request, &mut commands, SimulatorTapArgs::target(call));
    }
}

fn simulator_swipe(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::SimulatorSwipe) {
        call.finish_dispatch(request, &mut commands, SimulatorSwipeArgs::target(call));
    }
}

fn simulator_type(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::SimulatorType) {
        call.finish_dispatch(request, &mut commands, SimulatorTypeArgs::target(call));
    }
}

fn simulator_key(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::SimulatorKey) {
        call.finish_dispatch(request, &mut commands, SimulatorKeyArgs::target(call));
    }
}

fn simulator_button(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::SimulatorButton) {
        call.finish_dispatch(request, &mut commands, SimulatorButtonArgs::target(call));
    }
}

fn record_start(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::RecordStart) {
        call.finish_dispatch(request, &mut commands, RecordStartArgs::target(call));
    }
}

fn record_stop(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::RecordStop) {
        call.finish_dispatch(request, &mut commands, RecordStopArgs::target(call));
    }
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
