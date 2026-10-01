use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{AgentQueryResult, AgentRequest, ClientMessage, JsonValue};
use vmux_ecs::host::manifest::FeaturePlugin;
use vmux_ecs::service::{ServiceMessageSet, ServiceRequest};
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolQuery, ToolQueryHandled,
    ToolQueryRequest, ToolQueryRouteSet,
};

use super::agent::{AgentGetSettings, AgentUpdateSettings};

pub struct SettingToolPlugin;

impl Plugin for SettingToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .bind_tool::<GetSettingsArgs>()
            .bind_tool::<UpdateSettingsArgs>()
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
            .and_then(|_| serde_json::to_string(&*settings).map_err(|error| error.to_string()));
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::text(request.request_id, result),
        )));
    }
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetSettingsArgs {}

#[vmux_tool::input]
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
