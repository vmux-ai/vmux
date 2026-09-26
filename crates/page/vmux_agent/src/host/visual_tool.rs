use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{AgentQuery, SimulatorButton, SimulatorInput};
use vmux_core::JsonArguments;
use vmux_mcp::tool::{
    AddedTool, McpToolPlugin, ToolDispatchError, ToolDispatchSet, ToolQuery, ToolRequestSet,
};

pub struct VisualToolPlugin;

impl Plugin for VisualToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<VisualTool>::new(include_str!(
            "visual_tool.ron"
        )))
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

fn parse(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments, &VisualTool), AddedTool<VisualTool>>,
) {
    for (request, name, arguments, tool) in &calls {
        let parsed =
            match tool {
                VisualTool::Screenshot => {
                    arguments
                        .parse::<ScreenshotArgs>(name.as_str())
                        .map(|args| {
                            commands.entity(request).insert(args);
                        })
                }
                VisualTool::SimulatorScreenshot => continue,
                VisualTool::SimulatorTap => {
                    arguments
                        .parse::<SimulatorTapArgs>(name.as_str())
                        .map(|args| {
                            commands.entity(request).insert(args);
                        })
                }
                VisualTool::SimulatorSwipe => arguments
                    .parse::<SimulatorSwipeArgs>(name.as_str())
                    .map(|args| {
                        commands.entity(request).insert(args);
                    }),
                VisualTool::SimulatorType => arguments
                    .parse::<SimulatorTypeArgs>(name.as_str())
                    .map(|args| {
                        commands.entity(request).insert(args);
                    }),
                VisualTool::SimulatorKey => {
                    arguments
                        .parse::<SimulatorKeyArgs>(name.as_str())
                        .map(|args| {
                            commands.entity(request).insert(args);
                        })
                }
                VisualTool::SimulatorButton => arguments
                    .parse::<SimulatorButtonArgs>(name.as_str())
                    .map(|args| {
                        commands.entity(request).insert(args);
                    }),
                VisualTool::RecordStart => {
                    arguments
                        .parse::<RecordStartArgs>(name.as_str())
                        .map(|args| {
                            commands.entity(request).insert(args);
                        })
                }
                VisualTool::RecordStop => {
                    arguments
                        .parse::<RecordStopArgs>(name.as_str())
                        .map(|args| {
                            commands.entity(request).insert(args);
                        })
                }
            };
        if let Err(message) = parsed {
            commands
                .entity(request)
                .insert(ToolDispatchError::new(message));
        }
    }
}

fn screenshot(
    mut commands: Commands,
    requests: Query<(Entity, &ScreenshotArgs), AddedTool<ScreenshotArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::Screenshot {
                pane: OptionalText::trim(args.pane.clone()),
            })));
    }
}

fn simulator_screenshot(
    mut commands: Commands,
    calls: Query<(Entity, &VisualTool), AddedTool<VisualTool>>,
) {
    for (request, tool) in &calls {
        if *tool != VisualTool::SimulatorScreenshot {
            continue;
        }
        commands
            .entity(request)
            .insert(ToolQuery(Ok(AgentQuery::SimulatorScreenshot)));
    }
}

fn simulator_tap(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorTapArgs), AddedTool<SimulatorTapArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::SimulatorControl {
                input: SimulatorInput::Tap {
                    x: args.x,
                    y: args.y,
                },
            })));
    }
}

fn simulator_swipe(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorSwipeArgs), AddedTool<SimulatorSwipeArgs>>,
) {
    for (entity, args) in &requests {
        let duration_ms = args.duration_ms.unwrap_or(300);
        let query = if (1..=10_000).contains(&duration_ms) {
            Ok(AgentQuery::SimulatorControl {
                input: SimulatorInput::Swipe {
                    start_x: args.start_x,
                    start_y: args.start_y,
                    end_x: args.end_x,
                    end_y: args.end_y,
                    duration_ms,
                },
            })
        } else {
            Err("simulator_swipe.duration_ms must be between 1 and 10000".to_string())
        };
        commands.entity(entity).insert(ToolQuery(query));
    }
}

fn simulator_type(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorTypeArgs), AddedTool<SimulatorTypeArgs>>,
) {
    for (entity, args) in &requests {
        let text = &args.text;
        let query = if text.is_empty() {
            Err("simulator_type.text is empty".to_string())
        } else {
            Ok(AgentQuery::SimulatorControl {
                input: SimulatorInput::TypeText(text.clone()),
            })
        };
        commands.entity(entity).insert(ToolQuery(query));
    }
}

fn simulator_key(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorKeyArgs), AddedTool<SimulatorKeyArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::SimulatorControl {
                input: SimulatorInput::Key(args.keycode),
            })));
    }
}

fn simulator_button(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorButtonArgs), AddedTool<SimulatorButtonArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::SimulatorControl {
                input: SimulatorInput::Button(args.button.into()),
            })));
    }
}

fn record_start(
    mut commands: Commands,
    requests: Query<(Entity, &RecordStartArgs), AddedTool<RecordStartArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::RecordStart {
                gif: args.gif,
                max_secs: args.max_secs.unwrap_or(600),
                pane: OptionalText::trim(args.pane.clone()),
            })));
    }
}

fn record_stop(
    mut commands: Commands,
    requests: Query<(Entity, &RecordStopArgs), AddedTool<RecordStopArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::RecordStop {
                dir: OptionalText::trim(args.dir.clone()),
                name: OptionalText::trim(args.name.clone()),
            })));
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
