use bevy::prelude::*;
use vmux_api::protocol::{AgentCommandResult, AgentFileSearch, AgentFileTouched, AgentTurnEnded};
use vmux_core::agent::AgentCommandResponse;

use super::{AgentReply, CommandSet};

pub(super) struct AgentOperationPlugin;

impl Plugin for AgentOperationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentFileTouchedRequest>()
            .add_message::<AgentFileSearchRequest>()
            .add_message::<AgentTurnEndedRequest>()
            .add_message::<AgentNewChatRequest>()
            .add_message::<AgentListRequest>()
            .add_systems(
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

#[derive(Message, Clone)]
pub(super) struct AgentFileTouchedRequest {
    pub(super) reply: AgentReply,
    pub(super) _payload: AgentFileTouched,
}

#[derive(Message, Clone)]
pub(super) struct AgentFileSearchRequest {
    pub(super) reply: AgentReply,
    pub(super) _payload: AgentFileSearch,
}

#[derive(Message, Clone)]
pub(super) struct AgentTurnEndedRequest {
    pub(super) reply: AgentReply,
    pub(super) _payload: AgentTurnEnded,
}

#[derive(Message, Clone)]
pub(super) struct AgentNewChatRequest {
    pub(super) reply: AgentReply,
    pub(super) prompt: String,
    pub(super) agent_url: Option<String>,
}

#[derive(Message, Clone)]
pub(super) struct AgentListRequest {
    pub(super) reply: AgentReply,
}

fn acknowledge_file_touched(
    mut requests: MessageReader<AgentFileTouchedRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        responses.write(request.reply.ok());
    }
}

fn acknowledge_file_search(
    mut requests: MessageReader<AgentFileSearchRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        responses.write(request.reply.ok());
    }
}

fn acknowledge_turn_ended(
    mut requests: MessageReader<AgentTurnEndedRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        responses.write(request.reply.ok());
    }
}

fn new_chat(
    mut requests: MessageReader<AgentNewChatRequest>,
    contributed_pages: Query<&vmux_command::snapshot::ContributedPage>,
    mut new_tabs: MessageWriter<vmux_layout::NewTabRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let result = match vmux_command::snapshot::ContributedPage::prompt_url(
            &contributed_pages,
            request.agent_url.as_deref(),
        ) {
            Some(url) => {
                new_tabs.write(vmux_layout::NewTabRequest {
                    url,
                    pending_prompt: Some(request.prompt.clone()),
                });
                AgentCommandResult::Ok
            }
            None => AgentCommandResult::Error("no agent is installed".to_string()),
        };
        responses.write(request.reply.response(result));
    }
}

fn list_agents(
    mut requests: MessageReader<AgentListRequest>,
    command_bar: Res<vmux_command::snapshot::CommandBarProjection>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
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
        responses.write(request.reply.response(result));
    }
}
