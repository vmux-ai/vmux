use super::{
    DispatchTarget, NextToolOrder, RegisterTools, ToolCall, ToolCalls, ToolDispatchResult,
    ToolDispatchSet, ToolManifest, ToolRequestSet,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentQuery, SimulatorAction, SimulatorButton};

pub(super) struct VisualToolPlugin;

impl Plugin for VisualToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(RegisterTools))
            .add_systems(Update, parse.in_set(ToolRequestSet))
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

fn register(mut commands: Commands, mut next_order: ResMut<NextToolOrder>) {
    ToolManifest::<VisualTool>::from_ron(include_str!("visual.ron"))
        .spawn(&mut commands, &mut next_order);
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScreenshotArgs {
    pane: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorTapArgs {
    x: u32,
    y: u32,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorSwipeArgs {
    start_x: u32,
    start_y: u32,
    end_x: u32,
    end_y: u32,
    duration_ms: Option<u32>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorTypeArgs {
    text: String,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorKeyArgs {
    keycode: u8,
}

#[derive(Clone, Copy, Deserialize)]
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

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorButtonArgs {
    button: SimulatorButtonArg,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordStartArgs {
    #[serde(default)]
    gif: bool,
    max_secs: Option<u32>,
    pane: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordStopArgs {
    dir: Option<String>,
    name: Option<String>,
}

fn parse(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, call, tool) in calls.iter() {
        match tool {
            VisualTool::Screenshot => call.parse_into::<ScreenshotArgs>(request, &mut commands),
            VisualTool::SimulatorScreenshot => {}
            VisualTool::SimulatorTap => call.parse_into::<SimulatorTapArgs>(request, &mut commands),
            VisualTool::SimulatorSwipe => {
                call.parse_into::<SimulatorSwipeArgs>(request, &mut commands)
            }
            VisualTool::SimulatorType => {
                call.parse_into::<SimulatorTypeArgs>(request, &mut commands)
            }
            VisualTool::SimulatorKey => call.parse_into::<SimulatorKeyArgs>(request, &mut commands),
            VisualTool::SimulatorButton => {
                call.parse_into::<SimulatorButtonArgs>(request, &mut commands)
            }
            VisualTool::RecordStart => call.parse_into::<RecordStartArgs>(request, &mut commands),
            VisualTool::RecordStop => call.parse_into::<RecordStopArgs>(request, &mut commands),
        }
    }
}

fn screenshot(
    mut commands: Commands,
    requests: Query<(Entity, &ScreenshotArgs), (With<ToolCall>, Added<ScreenshotArgs>)>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolDispatchResult(Ok(DispatchTarget::Query(
                AgentQuery::Screenshot {
                    pane: OptionalText::trim(args.pane.clone()),
                },
            ))));
    }
}

fn simulator_screenshot(mut commands: Commands, calls: ToolCalls<VisualTool>) {
    for (request, _, _) in calls.matching(VisualTool::SimulatorScreenshot) {
        commands
            .entity(request)
            .insert(ToolDispatchResult(Ok(DispatchTarget::Query(
                AgentQuery::SimulatorScreenshot,
            ))));
    }
}

fn simulator_tap(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorTapArgs), (With<ToolCall>, Added<SimulatorTapArgs>)>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolDispatchResult(Ok(DispatchTarget::Query(
                AgentQuery::SimulatorControl {
                    action: SimulatorAction::Tap {
                        x: args.x,
                        y: args.y,
                    },
                },
            ))));
    }
}

fn simulator_swipe(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorSwipeArgs), (With<ToolCall>, Added<SimulatorSwipeArgs>)>,
) {
    for (entity, args) in &requests {
        let duration_ms = args.duration_ms.unwrap_or(300);
        let target = if (1..=10_000).contains(&duration_ms) {
            Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
                action: SimulatorAction::Swipe {
                    start_x: args.start_x,
                    start_y: args.start_y,
                    end_x: args.end_x,
                    end_y: args.end_y,
                    duration_ms,
                },
            }))
        } else {
            Err("simulator_swipe.duration_ms must be between 1 and 10000".to_string())
        };
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}

fn simulator_type(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorTypeArgs), (With<ToolCall>, Added<SimulatorTypeArgs>)>,
) {
    for (entity, args) in &requests {
        let text = &args.text;
        let target = if text.is_empty() {
            Err("simulator_type.text is empty".to_string())
        } else {
            Ok(DispatchTarget::Query(AgentQuery::SimulatorControl {
                action: SimulatorAction::TypeText(text.clone()),
            }))
        };
        commands.entity(entity).insert(ToolDispatchResult(target));
    }
}

fn simulator_key(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorKeyArgs), (With<ToolCall>, Added<SimulatorKeyArgs>)>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolDispatchResult(Ok(DispatchTarget::Query(
                AgentQuery::SimulatorControl {
                    action: SimulatorAction::Key(args.keycode),
                },
            ))));
    }
}

fn simulator_button(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorButtonArgs), (With<ToolCall>, Added<SimulatorButtonArgs>)>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolDispatchResult(Ok(DispatchTarget::Query(
                AgentQuery::SimulatorControl {
                    action: SimulatorAction::Button(args.button.into()),
                },
            ))));
    }
}

fn record_start(
    mut commands: Commands,
    requests: Query<(Entity, &RecordStartArgs), (With<ToolCall>, Added<RecordStartArgs>)>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolDispatchResult(Ok(DispatchTarget::Query(
                AgentQuery::RecordStart {
                    gif: args.gif,
                    max_secs: args.max_secs.unwrap_or(600),
                    pane: OptionalText::trim(args.pane.clone()),
                },
            ))));
    }
}

fn record_stop(
    mut commands: Commands,
    requests: Query<(Entity, &RecordStopArgs), (With<ToolCall>, Added<RecordStopArgs>)>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolDispatchResult(Ok(DispatchTarget::Query(
                AgentQuery::RecordStop {
                    dir: OptionalText::trim(args.dir.clone()),
                    name: OptionalText::trim(args.name.clone()),
                },
            ))));
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
