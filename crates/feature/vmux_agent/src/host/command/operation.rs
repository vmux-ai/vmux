use bevy::prelude::*;
use vmux_api::protocol::{
    AgentCommandResult, AgentFileSearch, AgentFileTouched, AgentListAgents, AgentNewChat,
    AgentTurnEnded,
};
use vmux_ecs::agent::{AgentCommandResponse, AgentReply, AgentRequestInput};
use vmux_session::{AgentConversationTitle, CreateRequest, Created, SessionId};

use super::CommandSet;
use crate::host::acp::registry::RegistryAgent;
use crate::route::SessionRoute;

pub(super) fn add(app: &mut App) {
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

#[derive(Resource, Default)]
struct PendingNewChats(std::collections::HashMap<SessionId, PendingNewChat>);

struct PendingNewChat {
    request_id: vmux_api::protocol::AgentRequestId,
    prompt: String,
}

fn open_created_chat(
    trigger: On<Created>,
    mut pending: ResMut<PendingNewChats>,
    mut new_tabs: MessageWriter<vmux_layout::NewTabRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    let Some(pending) = pending.0.remove(&trigger.event().id) else {
        return;
    };
    new_tabs.write(vmux_layout::NewTabRequest {
        url: SessionRoute::Session(trigger.event().id.clone()).url(),
        pending_prompt: Some(pending.prompt),
    });
    responses.write(AgentReply::new(pending.request_id).ok());
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
    contributed_pages: vmux_command::ContributedPages,
    mut session_requests: MessageWriter<CreateRequest>,
    mut pending: ResMut<PendingNewChats>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let Ok(Some(payload)) = request.decode::<AgentNewChat>() else {
            continue;
        };
        let result = match contributed_pages
            .prompt_url(payload.agent_url.as_deref())
            .and_then(|url| SessionRoute::requested_agent(&url).map(|agent| (url, agent)))
        {
            Some((_, agent)) => {
                let name = AgentConversationTitle::from_prompt(&payload.prompt)
                    .map(|title| title.0)
                    .unwrap_or_else(|| vmux_ui::i18n::translate("sessions-new"));
                let create =
                    CreateRequest::new(name, String::new(), std::path::PathBuf::new(), Some(agent));
                let id = create.id().clone();
                session_requests.write(create);
                pending.0.insert(
                    id,
                    PendingNewChat {
                        request_id: request.request_id,
                        prompt: payload.prompt,
                    },
                );
                continue;
            }
            None => AgentCommandResult::Error("no agent is installed".to_string()),
        };
        responses.write(AgentReply::new(request.request_id).response(result));
    }
}

fn list_agents(
    mut requests: MessageReader<AgentRequestInput>,
    catalog: Query<&RegistryAgent>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let Ok(Some(AgentListAgents)) = request.decode::<AgentListAgents>() else {
            continue;
        };
        let mut agents = Vec::new();
        for agent in &catalog {
            if !agent.is_installed() {
                continue;
            }
            agents.push(vmux_api::conversation::RemoteAgent {
                id: agent.id.clone(),
                name: agent.name.clone(),
                url: SessionRoute::manager_for_agent(&vmux_session::AgentId(agent.id.clone())),
                icon: agent.icon.clone().unwrap_or_default(),
            });
        }
        let result = match serde_json::to_string(&agents) {
            Ok(json) => AgentCommandResult::Text(json),
            Err(error) => AgentCommandResult::Error(format!("list_agents: {error}")),
        };
        responses.write(AgentReply::new(request.request_id).response(result));
    }
}
