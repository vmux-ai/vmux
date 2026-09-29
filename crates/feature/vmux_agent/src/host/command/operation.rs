use bevy::prelude::*;
use vmux_api::protocol::{
    AgentCommandResult, AgentFileSearch, AgentFileTouched, AgentListAgents, AgentNewChat,
    AgentTurnEnded,
};
use vmux_core::agent::{AgentCommandResponse, AgentReply, AgentRequestInput};

use super::CommandSet;

pub(super) struct AgentOperationPlugin;

impl Plugin for AgentOperationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                acknowledge_file_touched,
                acknowledge_file_search,
                acknowledge_turn_ended,
                new_chat,
                list_agents,
            )
                .in_set(CommandSet::Commands),
        );
    }
}

fn acknowledge_file_touched(
    mut requests: MessageReader<AgentRequestInput>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        if matches!(request.decode::<AgentFileTouched>(), Ok(Some(_))) {
            responses.write(AgentReply::new(request.request_id).ok());
        }
    }
}

fn acknowledge_file_search(
    mut requests: MessageReader<AgentRequestInput>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        if matches!(request.decode::<AgentFileSearch>(), Ok(Some(_))) {
            responses.write(AgentReply::new(request.request_id).ok());
        }
    }
}

fn acknowledge_turn_ended(
    mut requests: MessageReader<AgentRequestInput>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        if matches!(request.decode::<AgentTurnEnded>(), Ok(Some(_))) {
            responses.write(AgentReply::new(request.request_id).ok());
        }
    }
}

fn new_chat(
    mut requests: MessageReader<AgentRequestInput>,
    contributed_pages: vmux_command::snapshot::ContributedPages,
    mut new_tabs: MessageWriter<vmux_layout::NewTabRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let Ok(Some(payload)) = request.decode::<AgentNewChat>() else {
            continue;
        };
        let result = match contributed_pages.prompt_url(payload.agent_url.as_deref()) {
            Some(url) => {
                new_tabs.write(vmux_layout::NewTabRequest {
                    url,
                    pending_prompt: Some(payload.prompt),
                });
                AgentCommandResult::Ok
            }
            None => AgentCommandResult::Error("no agent is installed".to_string()),
        };
        responses.write(AgentReply::new(request.request_id).response(result));
    }
}

fn list_agents(
    mut requests: MessageReader<AgentRequestInput>,
    command_bar: Single<&vmux_command::snapshot::CommandBarProjection>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let Ok(Some(AgentListAgents)) = request.decode::<AgentListAgents>() else {
            continue;
        };
        let mut agents = Vec::new();
        for agent in &command_bar.agents.acp {
            agents.push(vmux_api::room::RemoteAgent {
                id: agent.id.clone(),
                name: agent.name.clone(),
                url: agent.url.clone(),
                icon: agent.icon.clone(),
            });
        }
        for agent in &command_bar.agents.providers {
            agents.push(vmux_api::room::RemoteAgent {
                id: agent.id.clone(),
                name: format!("{} (CLI)", agent.name),
                url: agent.url.clone(),
                icon: agent.icon.clone(),
            });
        }
        let result = match serde_json::to_string(&agents) {
            Ok(json) => AgentCommandResult::Text(json),
            Err(error) => AgentCommandResult::Error(format!("list_agents: {error}")),
        };
        responses.write(AgentReply::new(request.request_id).response(result));
    }
}
