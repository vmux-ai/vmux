use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::AgentRequest;
use vmux_core::host::manifest::FeaturePlugin;

use super::{
    AgentSimulatorButtonPress, AgentSimulatorKeyPress, AgentSimulatorScreenshot,
    AgentSimulatorSwipe, AgentSimulatorTap, AgentSimulatorTypeText, SimulatorButton,
};
use vmux_tool::{AddedTool, ToolAppExt, ToolDispatchSet, ToolQuery};

pub struct SimulatorToolPlugin;

impl Plugin for SimulatorToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .bind_tool::<SimulatorScreenshotArgs>()
            .bind_tool::<SimulatorTapArgs>()
            .bind_tool::<SimulatorSwipeArgs>()
            .bind_tool::<SimulatorTypeArgs>()
            .bind_tool::<SimulatorKeyArgs>()
            .bind_tool::<SimulatorButtonArgs>()
            .add_systems(
                Update,
                (screenshot, tap, swipe, type_text, key, button).in_set(ToolDispatchSet),
            );
    }
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorScreenshotArgs {}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorTapArgs {
    x: u32,
    y: u32,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorSwipeArgs {
    start_x: u32,
    start_y: u32,
    end_x: u32,
    end_y: u32,
    duration_ms: Option<u32>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorTypeArgs {
    text: String,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorKeyArgs {
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

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulatorButtonArgs {
    button: ButtonArg,
}

fn screenshot(mut commands: Commands, calls: Query<Entity, AddedTool<SimulatorScreenshotArgs>>) {
    for entity in &calls {
        commands
            .entity(entity)
            .insert(ToolQuery(AgentRequest::encode(&AgentSimulatorScreenshot)));
    }
}

fn tap(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorTapArgs), AddedTool<SimulatorTapArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(AgentRequest::encode(&AgentSimulatorTap {
                x: args.x,
                y: args.y,
            })));
    }
}

fn swipe(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorSwipeArgs), AddedTool<SimulatorSwipeArgs>>,
) {
    for (entity, args) in &requests {
        let duration_ms = args.duration_ms.unwrap_or(300);
        let query = if (1..=10_000).contains(&duration_ms) {
            AgentRequest::encode(&AgentSimulatorSwipe {
                start_x: args.start_x,
                start_y: args.start_y,
                end_x: args.end_x,
                end_y: args.end_y,
                duration_ms,
            })
        } else {
            Err("simulator_swipe.duration_ms must be between 1 and 10000".to_string())
        };
        commands.entity(entity).insert(ToolQuery(query));
    }
}

fn type_text(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorTypeArgs), AddedTool<SimulatorTypeArgs>>,
) {
    for (entity, args) in &requests {
        let query = if args.text.is_empty() {
            Err("simulator_type.text is empty".to_string())
        } else {
            AgentRequest::encode(&AgentSimulatorTypeText {
                text: args.text.clone(),
            })
        };
        commands.entity(entity).insert(ToolQuery(query));
    }
}

fn key(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorKeyArgs), AddedTool<SimulatorKeyArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(AgentRequest::encode(&AgentSimulatorKeyPress {
                keycode: args.keycode,
            })));
    }
}

fn button(
    mut commands: Commands,
    requests: Query<(Entity, &SimulatorButtonArgs), AddedTool<SimulatorButtonArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolQuery(AgentRequest::encode(
                &AgentSimulatorButtonPress {
                    button: args.button.into(),
                },
            )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::JsonArguments;
    use vmux_tool::{ToolCatalog, ToolCatalogRequest, ToolDispatchError, ToolInvocation};

    struct SimulatorToolFixture;

    impl SimulatorToolFixture {
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

        fn dispatch(name: &str, arguments: serde_json::Value) -> Result<AgentRequest, String> {
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
            SimulatorToolFixture::definitions(),
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
        let screenshot =
            SimulatorToolFixture::dispatch("simulator_screenshot", serde_json::json!({})).unwrap();
        assert_eq!(
            screenshot.decode::<AgentSimulatorScreenshot>().unwrap(),
            Some(AgentSimulatorScreenshot)
        );
        let tap = SimulatorToolFixture::dispatch(
            "simulator_tap",
            serde_json::json!({"x": 120, "y": 240}),
        )
        .unwrap();
        assert_eq!(
            tap.decode::<AgentSimulatorTap>().unwrap(),
            Some(AgentSimulatorTap { x: 120, y: 240 })
        );
        let swipe = SimulatorToolFixture::dispatch(
            "simulator_swipe",
            serde_json::json!({
                "start_x": 100,
                "start_y": 700,
                "end_x": 100,
                "end_y": 200,
            }),
        )
        .unwrap();
        assert_eq!(
            swipe.decode::<AgentSimulatorSwipe>().unwrap(),
            Some(AgentSimulatorSwipe {
                start_x: 100,
                start_y: 700,
                end_x: 100,
                end_y: 200,
                duration_ms: 300,
            })
        );
        let text =
            SimulatorToolFixture::dispatch("simulator_type", serde_json::json!({"text": "hello"}))
                .unwrap();
        assert_eq!(
            text.decode::<AgentSimulatorTypeText>().unwrap(),
            Some(AgentSimulatorTypeText {
                text: "hello".to_string(),
            })
        );
        let key =
            SimulatorToolFixture::dispatch("simulator_key", serde_json::json!({"keycode": 40}))
                .unwrap();
        assert_eq!(
            key.decode::<AgentSimulatorKeyPress>().unwrap(),
            Some(AgentSimulatorKeyPress { keycode: 40 })
        );
        let button = SimulatorToolFixture::dispatch(
            "simulator_button",
            serde_json::json!({"button": "home"}),
        )
        .unwrap();
        assert_eq!(
            button.decode::<AgentSimulatorButtonPress>().unwrap(),
            Some(AgentSimulatorButtonPress {
                button: SimulatorButton::Home,
            })
        );
    }

    #[test]
    fn simulator_controls_reject_invalid_values() {
        assert!(
            SimulatorToolFixture::dispatch("simulator_type", serde_json::json!({"text": ""}))
                .is_err()
        );
        assert!(
            SimulatorToolFixture::dispatch(
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
