use super::{
    McpToolPlugin, ToolCall, ToolCalls, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolQuery,
    ToolRequestSet,
};
use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery, JsonValue};

pub(super) struct SettingToolPlugin;

impl Plugin for SettingToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<SettingTool>::new(include_str!(
            "setting.ron"
        )))
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

fn parse(mut commands: Commands, calls: ToolCalls<SettingTool>) {
    for (request, call, tool) in calls.iter() {
        if *tool == SettingTool::UpdateSettings {
            match call.parse::<UpdateSettingsArgs>() {
                Ok(args) => {
                    commands.entity(request).insert(args);
                }
                Err(message) => {
                    commands.entity(request).insert(ToolDispatchError(message));
                }
            }
        }
    }
}

fn get_settings(mut commands: Commands, calls: ToolCalls<SettingTool>) {
    for (request, _, _) in calls.matching(SettingTool::GetSettings) {
        commands
            .entity(request)
            .insert(ToolQuery(Ok(AgentQuery::GetSettings)));
    }
}

fn update_settings(
    mut commands: Commands,
    requests: Query<(Entity, &UpdateSettingsArgs), (With<ToolCall>, Added<UpdateSettingsArgs>)>,
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
