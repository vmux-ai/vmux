use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{AgentRequest, ClientMessage, JsonValue};
use vmux_core::service::{ServiceMessageSet, ServiceRequest};
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
    ToolQueryHandled, ToolQueryRequest, ToolQueryRouteSet,
};

use super::agent::{AgentGetSettings, AgentUpdateSettings};

pub struct SettingToolPlugin;

impl Plugin for SettingToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::from_feature(
            include_str!("../feature.ron"),
            "default",
        ))
        .register_tool::<GetSettingsArgs>("get_settings")
        .register_tool::<UpdateSettingsArgs>("update_settings")
        .add_message::<ToolQueryRequest>()
        .add_message::<ToolQueryHandled>()
        .add_message::<ServiceRequest>()
        .add_systems(
            Update,
            (get_settings, update_settings).in_set(ToolDispatchSet),
        )
        .add_systems(
            Update,
            answer_settings_queries
                .in_set(ToolQueryRouteSet)
                .after(ServiceMessageSet),
        );
    }
}

fn answer_settings_queries(
    mut queries: MessageReader<ToolQueryRequest>,
    settings: Res<crate::AppSettings>,
    mut handled: MessageWriter<ToolQueryHandled>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in queries.read() {
        if request.query.id != AgentGetSettings::ID {
            continue;
        }
        handled.write(ToolQueryHandled(request.request_id));
        let result = serde_json::from_slice::<AgentGetSettings>(&request.query.body)
            .map_err(|error| error.to_string())
            .and_then(|_| {
                serde_json::to_value(&*settings)
                    .map(JsonValue::from)
                    .map_err(|error| format!("failed to serialize settings: {error}"))
            });
        service_requests.write(ServiceRequest(ClientMessage::AgentSettingsResult {
            request_id: request.request_id,
            result,
        }));
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
