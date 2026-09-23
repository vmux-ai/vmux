use super::{
    DispatchTarget, ParsedToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolRequestSet, ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery, JsonValue};

pub(super) struct SettingToolPlugin;

impl Plugin for SettingToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Setting))
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

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<SettingTool>::from_ron(include_str!("setting.ron"));
    tools.spawn_manifest(manifest);
}

fn parse(mut commands: Commands, calls: ToolCalls<SettingTool>) {
    for (request, call, tool) in calls.iter() {
        if *tool == SettingTool::UpdateSettings {
            call.parse_into::<UpdateSettingsArgs>(request, &mut commands);
        }
    }
}

fn get_settings(mut commands: Commands, calls: ToolCalls<SettingTool>) {
    for (request, call, _) in calls.matching(SettingTool::GetSettings) {
        call.finish_dispatch(
            request,
            &mut commands,
            Ok(DispatchTarget::Query(AgentQuery::GetSettings)),
        );
    }
}

fn update_settings(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<UpdateSettingsArgs>),
        Added<ParsedToolCall<UpdateSettingsArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let args = request.args();
        let target = if args.path.trim().is_empty() {
            Err("update_settings.path is empty".to_string())
        } else {
            Ok(DispatchTarget::Command(AgentCommand::UpdateSettings {
                path: args.path.clone(),
                value: JsonValue::from(args.value.clone()),
            }))
        };
        request.finish(entity, &mut commands, target);
    }
}
