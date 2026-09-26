use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{AgentCommand, AgentRenameProfile};
use vmux_core::JsonArguments;
use vmux_tool::{
    AddedTool, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolKindManifestPlugin,
    ToolRequestSet,
};

pub struct TeamToolPlugin;

impl Plugin for TeamToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolKindManifestPlugin::<TeamTool>::new(include_str!(
            "tool.ron"
        )))
        .add_systems(Update, parse.in_set(ToolRequestSet))
        .add_systems(Update, rename_profile.in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TeamTool {
    RenameProfile,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameProfileArgs {
    name: String,
}

fn parse(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments), AddedTool<TeamTool>>,
) {
    for (entity, name, arguments) in &calls {
        match arguments.parse::<RenameProfileArgs>(name.as_str()) {
            Ok(args) => {
                commands.entity(entity).insert(args);
            }
            Err(message) => {
                commands
                    .entity(entity)
                    .insert(ToolDispatchError::new(message));
            }
        }
    }
}

fn rename_profile(
    mut commands: Commands,
    requests: Query<(Entity, &RenameProfileArgs), AddedTool<RenameProfileArgs>>,
) {
    for (entity, args) in &requests {
        let name = args.name.trim();
        let command = if name.is_empty() {
            Err("rename_profile.name is empty".to_string())
        } else {
            Ok(AgentCommand::RenameProfile(AgentRenameProfile {
                name: name.to_string(),
            }))
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_tool::{ToolCatalog, ToolCatalogRequest, ToolDispatchError, ToolInvocation};

    impl TeamTool {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(TeamToolPlugin);
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

        fn dispatch(arguments: serde_json::Value) -> Result<AgentCommand, String> {
            let mut app = Self::app();
            let request = app
                .world_mut()
                .spawn((
                    Name::new("rename_profile"),
                    JsonArguments(arguments),
                    ToolInvocation,
                ))
                .id();
            app.update();
            if let Some(command) = app.world_mut().entity_mut(request).take::<ToolCommand>() {
                return command.0;
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
    fn manifest_registers_team_tools() {
        assert_eq!(TeamTool::definitions(), ["rename_profile"]);
    }

    #[test]
    fn rename_profile_dispatches_trimmed_name() {
        assert_eq!(
            TeamTool::dispatch(serde_json::json!({"name": "  Junichi  "})),
            Ok(AgentCommand::RenameProfile(AgentRenameProfile {
                name: "Junichi".to_string(),
            }))
        );
        assert!(TeamTool::dispatch(serde_json::json!({"name": "  "})).is_err());
    }
}
