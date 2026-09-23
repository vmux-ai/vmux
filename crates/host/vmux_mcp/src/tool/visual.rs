use super::{
    DispatchTarget, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet, ToolSpawner,
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
    fn query(self) -> AgentQuery {
        AgentQuery::Screenshot {
            pane: OptionalText::trim(self.pane),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorTapArgs {
    x: u32,
    y: u32,
}

impl SimulatorTapArgs {
    fn query(self) -> AgentQuery {
        AgentQuery::SimulatorControl {
            action: SimulatorAction::Tap {
                x: self.x,
                y: self.y,
            },
        }
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
    fn query(self) -> Result<AgentQuery, String> {
        let duration_ms = self.duration_ms.unwrap_or(300);
        if !(1..=10_000).contains(&duration_ms) {
            return Err("simulator_swipe.duration_ms must be between 1 and 10000".to_string());
        }
        Ok(AgentQuery::SimulatorControl {
            action: SimulatorAction::Swipe {
                start_x: self.start_x,
                start_y: self.start_y,
                end_x: self.end_x,
                end_y: self.end_y,
                duration_ms,
            },
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorTypeArgs {
    text: String,
}

impl SimulatorTypeArgs {
    fn query(self) -> Result<AgentQuery, String> {
        if self.text.is_empty() {
            return Err("simulator_type.text is empty".to_string());
        }
        Ok(AgentQuery::SimulatorControl {
            action: SimulatorAction::TypeText(self.text),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorKeyArgs {
    keycode: u8,
}

impl SimulatorKeyArgs {
    fn query(self) -> AgentQuery {
        AgentQuery::SimulatorControl {
            action: SimulatorAction::Key(self.keycode),
        }
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
    fn query(self) -> AgentQuery {
        AgentQuery::SimulatorControl {
            action: SimulatorAction::Button(self.button.into()),
        }
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
    fn query(self) -> AgentQuery {
        AgentQuery::RecordStart {
            gif: self.gif,
            max_secs: self.max_secs.unwrap_or(600),
            pane: OptionalText::trim(self.pane),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordStopArgs {
    dir: Option<String>,
    name: Option<String>,
}

impl RecordStopArgs {
    fn query(self) -> AgentQuery {
        AgentQuery::RecordStop {
            dir: OptionalText::trim(self.dir),
            name: OptionalText::trim(self.name),
        }
    }
}

fn screenshot(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::Screenshot) {
        let target = call
            .parse::<ScreenshotArgs>("screenshot")
            .map(ScreenshotArgs::query)
            .map(DispatchTarget::Query);
        call.finish_dispatch(request, &mut commands, target);
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
        let target = call
            .parse::<SimulatorTapArgs>("simulator_tap")
            .map(SimulatorTapArgs::query)
            .map(DispatchTarget::Query);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn simulator_swipe(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::SimulatorSwipe) {
        let target = call
            .parse::<SimulatorSwipeArgs>("simulator_swipe")
            .and_then(SimulatorSwipeArgs::query)
            .map(DispatchTarget::Query);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn simulator_type(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::SimulatorType) {
        let target = call
            .parse::<SimulatorTypeArgs>("simulator_type")
            .and_then(SimulatorTypeArgs::query)
            .map(DispatchTarget::Query);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn simulator_key(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::SimulatorKey) {
        let target = call
            .parse::<SimulatorKeyArgs>("simulator_key")
            .map(SimulatorKeyArgs::query)
            .map(DispatchTarget::Query);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn simulator_button(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::SimulatorButton) {
        let target = call
            .parse::<SimulatorButtonArgs>("simulator_button")
            .map(SimulatorButtonArgs::query)
            .map(DispatchTarget::Query);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn record_start(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::RecordStart) {
        let target = call
            .parse::<RecordStartArgs>("record_start")
            .map(RecordStartArgs::query)
            .map(DispatchTarget::Query);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn record_stop(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, _) in calls.matching(VisualTool::RecordStop) {
        let target = call
            .parse::<RecordStopArgs>("record_stop")
            .map(RecordStopArgs::query)
            .map(DispatchTarget::Query);
        call.finish_dispatch(request, &mut commands, target);
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
