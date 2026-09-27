use bevy::prelude::*;
use vmux_api::protocol::{AgentCommand, AgentCommandResult, SharedAgentCommand};
use vmux_core::agent::{AgentCommandRequest, AgentCommandResponse, AgentReply};

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
    mut requests: MessageReader<AgentCommandRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        if matches!(&request.command, AgentCommand::FileTouched(_)) {
            responses.write(AgentReply::new(request.request_id).ok());
        }
    }
}

fn acknowledge_file_search(
    mut requests: MessageReader<AgentCommandRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        if matches!(&request.command, AgentCommand::FileSearch(_)) {
            responses.write(AgentReply::new(request.request_id).ok());
        }
    }
}

fn acknowledge_turn_ended(
    mut requests: MessageReader<AgentCommandRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        if matches!(request.command, AgentCommand::TurnEnded(_)) {
            responses.write(AgentReply::new(request.request_id).ok());
        }
    }
}

fn new_chat(
    mut requests: MessageReader<AgentCommandRequest>,
    contributed_pages: Query<&vmux_command::snapshot::ContributedPage>,
    mut new_tabs: MessageWriter<vmux_layout::NewTabRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let AgentCommand::Shared(SharedAgentCommand::NewAgentChat {
            prompt, agent_url, ..
        }) = &request.command
        else {
            continue;
        };
        let result = match vmux_command::snapshot::ContributedPage::prompt_url(
            &contributed_pages,
            agent_url.as_deref(),
        ) {
            Some(url) => {
                new_tabs.write(vmux_layout::NewTabRequest {
                    url,
                    pending_prompt: Some(prompt.clone()),
                });
                AgentCommandResult::Ok
            }
            None => AgentCommandResult::Error("no agent is installed".to_string()),
        };
        responses.write(AgentReply::new(request.request_id).response(result));
    }
}

fn list_agents(
    mut requests: MessageReader<AgentCommandRequest>,
    command_bar: Res<vmux_command::snapshot::CommandBarProjection>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        if !matches!(
            &request.command,
            AgentCommand::Shared(SharedAgentCommand::ListAgents)
        ) {
            continue;
        }
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
