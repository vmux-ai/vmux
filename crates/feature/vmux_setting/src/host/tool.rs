use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::{AgentGetSettings, AgentRequest, AgentUpdateSettings, JsonValue};
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
};

pub struct SettingToolPlugin;

impl Plugin for SettingToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::from_feature(
            include_str!("../feature.ron"),
            "default",
        ))
        .register_tool::<GetSettingsArgs>("get_settings")
        .register_tool::<UpdateSettingsArgs>("update_settings")
        .add_systems(
            Update,
            (get_settings, update_settings).in_set(ToolDispatchSet),
        );
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetSettingsArgs {}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateSettingsArgs {
    path: String,
    value: serde_json::Value,
}

fn get_settings(mut commands: Commands, calls: Query<Entity, AddedTool<GetSettingsArgs>>) {
    for request in &calls {
        commands
            .entity(request)
            .insert(ToolQuery(AgentRequest::encode(&AgentGetSettings)));
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
            AgentRequest::encode(&AgentUpdateSettings {
                path: args.path.clone(),
                value: JsonValue::from(args.value.clone()),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}
