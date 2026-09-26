use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{
    AgentQuery, SimulatorButton, SimulatorButtonPress, SimulatorKeyPress, SimulatorSwipe,
    SimulatorTap, SimulatorTypeText,
};
use vmux_core::JsonArguments;
use vmux_tool::{
    AddedTool, ToolDispatchError, ToolDispatchSet, ToolKindManifestPlugin, ToolQuery,
    ToolRequestSet,
};

pub struct SimulatorToolPlugin;

impl Plugin for SimulatorToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolKindManifestPlugin::<SimulatorTool>::new(include_str!(
            "tool.ron"
        )))
        .add_systems(Update, parse.in_set(ToolRequestSet))
        .add_systems(
            Update,
            (screenshot, tap, swipe, type_text, key, button).in_set(ToolDispatchSet),
        );
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
enum SimulatorTool {
    #[serde(rename = "simulator_screenshot")]
    Screenshot,
    #[serde(rename = "simulator_tap")]
    Tap,
    #[serde(rename = "simulator_swipe")]
    Swipe,
    #[serde(rename = "simulator_type")]
    Type,
    #[serde(rename = "simulator_key")]
    Key,
    #[serde(rename = "simulator_button")]
    Button,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct TapArgs {
    x: u32,
    y: u32,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SwipeArgs {
    start_x: u32,
    start_y: u32,
    end_x: u32,
    end_y: u32,
    duration_ms: Option<u32>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct TypeArgs {
    text: String,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyArgs {
    keycode: u8,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ButtonArg {
    Home,
    Lock,
    Siri,
}

impl From<ButtonArg> for SimulatorButton {
    fn from(value: ButtonArg) -> Self {
        match value {
            ButtonArg::Home => Self::Home,
            ButtonArg::Lock => Self::Lock,
            ButtonArg::Siri => Self::Siri,
        }
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ButtonArgs {
    button: ButtonArg,
}

fn parse(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments, &SimulatorTool), AddedTool<SimulatorTool>>,
) {
    for (entity, name, arguments, tool) in &calls {
        let parsed = match tool {
            SimulatorTool::Screenshot => continue,
            SimulatorTool::Tap => arguments.parse::<TapArgs>(name.as_str()).map(|args| {
                commands.entity(entity).insert(args);
            }),
            SimulatorTool::Swipe => arguments.parse::<SwipeArgs>(name.as_str()).map(|args| {
                commands.entity(entity).insert(args);
            }),
            SimulatorTool::Type => arguments.parse::<TypeArgs>(name.as_str()).map(|args| {
                commands.entity(entity).insert(args);
            }),
            SimulatorTool::Key => arguments.parse::<KeyArgs>(name.as_str()).map(|args| {
                commands.entity(entity).insert(args);
            }),
            SimulatorTool::Button => arguments.parse::<ButtonArgs>(name.as_str()).map(|args| {
                commands.entity(entity).insert(args);
            }),
        };
        if let Err(message) = parsed {
            commands
                .entity(entity)
                .insert(ToolDispatchError::new(message));
        }
    }
}

fn screenshot(
    mut commands: Commands,
    calls: Query<(Entity, &SimulatorTool), AddedTool<SimulatorTool>>,
) {
    for (entity, tool) in &calls {
        if *tool == SimulatorTool::Screenshot {
            commands
                .entity(entity)
                .insert(ToolQuery(Ok(AgentQuery::SimulatorScreenshot)));
        }
    }
}

fn tap(mut commands: Commands, requests: Query<(Entity, &TapArgs), AddedTool<TapArgs>>) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::SimulatorTap(SimulatorTap {
                x: args.x,
                y: args.y,
            }))));
    }
}

fn swipe(mut commands: Commands, requests: Query<(Entity, &SwipeArgs), AddedTool<SwipeArgs>>) {
    for (entity, args) in &requests {
        let duration_ms = args.duration_ms.unwrap_or(300);
        let query = if (1..=10_000).contains(&duration_ms) {
            Ok(AgentQuery::SimulatorSwipe(SimulatorSwipe {
                start_x: args.start_x,
                start_y: args.start_y,
                end_x: args.end_x,
                end_y: args.end_y,
                duration_ms,
            }))
        } else {
            Err("simulator_swipe.duration_ms must be between 1 and 10000".to_string())
        };
        commands.entity(entity).insert(ToolQuery(query));
    }
}

fn type_text(mut commands: Commands, requests: Query<(Entity, &TypeArgs), AddedTool<TypeArgs>>) {
    for (entity, args) in &requests {
        let query = if args.text.is_empty() {
            Err("simulator_type.text is empty".to_string())
        } else {
            Ok(AgentQuery::SimulatorTypeText(SimulatorTypeText {
                text: args.text.clone(),
            }))
        };
        commands.entity(entity).insert(ToolQuery(query));
    }
}

fn key(mut commands: Commands, requests: Query<(Entity, &KeyArgs), AddedTool<KeyArgs>>) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::SimulatorKeyPress(
                SimulatorKeyPress {
                    keycode: args.keycode,
                },
            ))));
    }
}

fn button(mut commands: Commands, requests: Query<(Entity, &ButtonArgs), AddedTool<ButtonArgs>>) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(Ok(AgentQuery::SimulatorButtonPress(
                SimulatorButtonPress {
                    button: args.button.into(),
                },
            ))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_tool::{ToolCatalog, ToolCatalogRequest, ToolDispatchError, ToolInvocation};

    impl SimulatorTool {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(SimulatorToolPlugin);
            app.update();
            app
        }

        fn definitions() -> Vec<String> {
            let mut app = Self::app();
            let request = app.world_mut().spawn(ToolCatalogRequest).id();
            app.update();
            app.world_mut()
                .entity_mut(request)
                .take::<ToolCatalog>()
                .unwrap()
                .0
                .into_iter()
                .map(|definition| definition.name)
                .collect()
        }

        fn dispatch(name: &str, arguments: serde_json::Value) -> Result<AgentQuery, String> {
            let mut app = Self::app();
            let request = app
                .world_mut()
                .spawn((
                    Name::new(name.to_string()),
                    JsonArguments(arguments),
                    ToolInvocation,
                ))
                .id();
            app.update();
            if let Some(query) = app.world_mut().entity_mut(request).take::<ToolQuery>() {
                return query.0;
            }
            let error = app
                .world_mut()
                .entity_mut(request)
                .take::<ToolDispatchError>()
                .unwrap();
            Err(error.message().to_string())
        }
    }

    #[test]
    fn manifest_registers_simulator_tools() {
        assert_eq!(
            SimulatorTool::definitions(),
            [
                "simulator_screenshot",
                "simulator_tap",
                "simulator_swipe",
                "simulator_type",
                "simulator_key",
                "simulator_button",
            ]
        );
    }

    #[test]
    fn simulator_controls_dispatch_typed_queries() {
        assert_eq!(
            SimulatorTool::dispatch("simulator_screenshot", serde_json::json!({})),
            Ok(AgentQuery::SimulatorScreenshot)
        );
        assert_eq!(
            SimulatorTool::dispatch("simulator_tap", serde_json::json!({"x": 120, "y": 240}),),
            Ok(AgentQuery::SimulatorTap(SimulatorTap { x: 120, y: 240 }))
        );
        assert_eq!(
            SimulatorTool::dispatch(
                "simulator_swipe",
                serde_json::json!({
                    "start_x": 100,
                    "start_y": 700,
                    "end_x": 100,
                    "end_y": 200,
                }),
            ),
            Ok(AgentQuery::SimulatorSwipe(SimulatorSwipe {
                start_x: 100,
                start_y: 700,
                end_x: 100,
                end_y: 200,
                duration_ms: 300,
            }))
        );
        assert_eq!(
            SimulatorTool::dispatch("simulator_type", serde_json::json!({"text": "hello"})),
            Ok(AgentQuery::SimulatorTypeText(SimulatorTypeText {
                text: "hello".to_string(),
            }))
        );
        assert_eq!(
            SimulatorTool::dispatch("simulator_key", serde_json::json!({"keycode": 40})),
            Ok(AgentQuery::SimulatorKeyPress(SimulatorKeyPress {
                keycode: 40,
            }))
        );
        assert_eq!(
            SimulatorTool::dispatch("simulator_button", serde_json::json!({"button": "home"}),),
            Ok(AgentQuery::SimulatorButtonPress(SimulatorButtonPress {
                button: SimulatorButton::Home,
            }))
        );
    }

    #[test]
    fn simulator_controls_reject_invalid_values() {
        assert!(
            SimulatorTool::dispatch("simulator_type", serde_json::json!({"text": ""})).is_err()
        );
        assert!(
            SimulatorTool::dispatch(
                "simulator_swipe",
                serde_json::json!({
                    "start_x": 0,
                    "start_y": 0,
                    "end_x": 1,
                    "end_y": 1,
                    "duration_ms": 0,
                }),
            )
            .is_err()
        );
    }
}
