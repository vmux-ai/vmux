use super::{
    DispatchTarget, ToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery, JsonValue};

pub(super) struct SettingToolPlugin;

impl Plugin for SettingToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Setting))
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateSettingsArgs {
    path: String,
    value: serde_json::Value,
}

impl UpdateSettingsArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("update_settings")?;
        if args.path.trim().is_empty() {
            return Err("update_settings.path is empty".to_string());
        }
        Ok(DispatchTarget::Command(AgentCommand::UpdateSettings {
            path: args.path,
            value: JsonValue::from(args.value),
        }))
    }
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<SettingTool>::from_ron(include_str!("setting.ron"));
    tools.spawn_manifest(manifest);
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

fn update_settings(mut commands: Commands, calls: ToolCalls<SettingTool>) {
    for (request, call, _) in calls.matching(SettingTool::UpdateSettings) {
        call.finish_dispatch(request, &mut commands, UpdateSettingsArgs::target(call));
    }
}
