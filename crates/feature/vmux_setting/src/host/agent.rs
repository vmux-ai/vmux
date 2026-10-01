use bevy::prelude::*;
use vmux_api::protocol::{AgentCommandResult, JsonValue};
use vmux_ecs::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
};

use super::{AppSettings, SettingsSaveRequest};

#[vmux_api::agent]
pub(super) struct AgentUpdateSettings {
    pub path: String,
    pub value: JsonValue,
}

#[vmux_api::agent(Copy, Eq)]
pub(super) struct AgentGetSettings;

pub(super) struct AgentSettingsPlugin;

impl Plugin for AgentSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_agent_request::<AgentUpdateSettings>()
            .add_systems(Update, update_settings.after(AgentRequestRouteSet));
    }
}

fn update_settings(
    mut requests: MessageReader<AgentRequestMessage<AgentUpdateSettings>>,
    mut settings: ResMut<AppSettings>,
    mut save: MessageWriter<SettingsSaveRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        if request.payload.path.trim().is_empty() {
            responses.write(request.reply.response(AgentCommandResult::Error(
                "update_settings.path is empty".to_string(),
            )));
            continue;
        }
        let result = match serde_json::Value::try_from(&request.payload.value) {
            Ok(value) => {
                let mut updated = (*settings).clone();
                match updated.apply_update(&request.payload.path, value) {
                    Ok(()) => {
                        if request.origin.is_agent()
                            && updated.agent.allow_run_placement_override
                                != settings.agent.allow_run_placement_override
                        {
                            AgentCommandResult::Error(
                                "update_settings: agent.allow_run_placement_override can only be changed in Settings"
                                    .to_string(),
                            )
                        } else {
                            *settings = updated;
                            save.write(SettingsSaveRequest);
                            AgentCommandResult::Ok
                        }
                    }
                    Err(message) => AgentCommandResult::Error(message),
                }
            }
            Err(error) => {
                AgentCommandResult::Error(format!("update_settings: invalid JSON value: {error}"))
            }
        };
        responses.write(request.reply.response(result));
    }
}
