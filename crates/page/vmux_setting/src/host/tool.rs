use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{AgentCommand, AgentQuery, JsonValue};
use vmux_core::JsonArguments;
use vmux_mcp::tool::{
    AddedTool, McpToolPlugin, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolQuery,
    ToolRequestSet,
};

pub struct SettingToolPlugin;

impl Plugin for SettingToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<SettingTool>::new(include_str!("tool.ron")))
            .add_systems(Update, parse.in_set(ToolRequestSet))
            .add_systems(
                Update,
                (get_settings, update_settings).in_set(ToolDispatchSet),
            );
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SettingTool {
    GetSettings,
    UpdateSettings,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateSettingsArgs {
    path: String,
    value: serde_json::Value,
}

fn parse(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments, &SettingTool), AddedTool<SettingTool>>,
) {
    for (request, name, arguments, tool) in &calls {
        if *tool == SettingTool::UpdateSettings {
            match arguments.parse::<UpdateSettingsArgs>(name.as_str()) {
                Ok(args) => {
                    commands.entity(request).insert(args);
                }
                Err(message) => {
                    commands
                        .entity(request)
                        .insert(ToolDispatchError::new(message));
                }
            }
        }
    }
}

fn get_settings(
    mut commands: Commands,
    calls: Query<(Entity, &SettingTool), AddedTool<SettingTool>>,
) {
    for (request, tool) in &calls {
        if *tool != SettingTool::GetSettings {
            continue;
        }
        commands
            .entity(request)
            .insert(ToolQuery(Ok(AgentQuery::GetSettings)));
    }
}

fn update_settings(
    mut commands: Commands,
    requests: Query<(Entity, &UpdateSettingsArgs), AddedTool<UpdateSettingsArgs>>,
) {
    for (entity, args) in &requests {
        let command = if args.path.trim().is_empty() {
            Err("update_settings.path is empty".to_string())
        } else {
            Ok(AgentCommand::UpdateSettings {
                path: args.path.clone(),
                value: JsonValue::from(args.value.clone()),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}
