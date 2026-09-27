use bevy::prelude::*;
use vmux_api::protocol::{AgentCommandResult, AgentUpdateSettings};
use vmux_core::agent::{AgentCommandResponse, AgentReply, AgentRequestInput};

use super::{AppSettings, SettingsWriteRequest};

pub(super) struct AgentSettingsPlugin;

impl Plugin for AgentSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentRequestInput>()
            .add_message::<AgentCommandResponse>()
            .add_systems(Update, update_settings);
    }
}

fn update_settings(
    mut requests: MessageReader<AgentRequestInput>,
    mut settings: ResMut<AppSettings>,
    mut write: MessageWriter<SettingsWriteRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let Ok(Some(payload)) = request.decode::<AgentUpdateSettings>() else {
            continue;
        };
        if payload.path.trim().is_empty() {
            responses.write(AgentReply::new(request.request_id).response(
                AgentCommandResult::Error("update_settings.path is empty".to_string()),
            ));
            continue;
        }
        let result = match serde_json::Value::try_from(&payload.value) {
            Ok(value) => {
                let mut updated = (*settings).clone();
                match updated.apply_update(&payload.path, value) {
                    Ok(ron_bytes) => {
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
                            write.write(SettingsWriteRequest { ron_bytes });
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
        responses.write(AgentReply::new(request.request_id).response(result));
    }
}
