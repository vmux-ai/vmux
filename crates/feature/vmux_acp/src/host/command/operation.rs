use bevy::prelude::*;
use vmux_api::protocol::{
    AgentCommandResult, AgentFileSearch, AgentFileTouched, AgentListAgents, AgentNewChat,
    AgentTurnEnded,
};
use vmux_ecs::agent::{AgentCommandResponse, AgentReply, AgentRequestReady};
use vmux_ecs::{Operation, PageOpenTarget};
use vmux_session::host::StartRequest;
use vmux_session::{Created, SessionId};

use super::CommandSet;
use crate::host::acp::installed::InstalledAgent;
use crate::host::acp::registry::RegistryAgent;
use vmux_session::Route as SessionRoute;

pub(super) struct OperationPlugin;

impl bevy::prelude::Plugin for OperationPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.add_message::<vmux_session::mutation::Rejected>()
            .add_observer(open_created_session)
            .add_systems(
                Update,
                (
                    acknowledge_file_touched,
                    acknowledge_file_search,
                    acknowledge_turn_ended,
                    create_session,
                    reject_created_session,
                    list_agents,
                )
                    .in_set(CommandSet::Commands)
                    .after(crate::host::acp::installed::InstalledSet)
                    .after(vmux_ecs::agent::AgentRequestDispatchSet),
            );
    }
}

#[derive(Component)]
struct PendingSessionCreation {
    request_id: vmux_api::protocol::AgentRequestId,
}

fn open_created_session(
    trigger: On<Created>,
    pending: Query<(Entity, &SessionId, &PendingSessionCreation)>,
    mut responses: MessageWriter<AgentCommandResponse>,
    mut commands: Commands,
) {
    let created_id = &trigger.event().id;
    for (entity, session_id, creation) in &pending {
        if session_id != created_id {
            continue;
        }
        responses.write(AgentReply::new(creation.request_id).ok());
        commands.entity(entity).despawn();
    }
}

fn reject_created_session(
    mut rejected: MessageReader<vmux_session::mutation::Rejected>,
    pending: Query<(Entity, &SessionId, &PendingSessionCreation)>,
    mut responses: MessageWriter<AgentCommandResponse>,
    mut commands: Commands,
) {
    for rejection in rejected.read() {
        for (entity, session_id, creation) in &pending {
            if session_id != &rejection.session {
                continue;
            }
            responses.write(AgentReply::new(creation.request_id).response(
                AgentCommandResult::Error(format!(
                    "create session rejected: {:?}",
                    rejection.reason
                )),
            ));
            commands.entity(entity).despawn();
        }
    }
}

fn acknowledge_file_touched(
    mut requests: MessageReader<AgentRequestReady>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        if matches!(request.decode::<AgentFileTouched>(), Ok(Some(_))) {
            responses.write(AgentReply::new(request.request_id).ok());
        }
    }
}

fn acknowledge_file_search(
    mut requests: MessageReader<AgentRequestReady>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        if matches!(request.decode::<AgentFileSearch>(), Ok(Some(_))) {
            responses.write(AgentReply::new(request.request_id).ok());
        }
    }
}

fn acknowledge_turn_ended(
    mut requests: MessageReader<AgentRequestReady>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        if matches!(request.decode::<AgentTurnEnded>(), Ok(Some(_))) {
            responses.write(AgentReply::new(request.request_id).ok());
        }
    }
}

fn create_session(
    mut requests: MessageReader<AgentRequestReady>,
    contributed_pages: vmux_command::ContributedPages,
    mut session_requests: MessageWriter<StartRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
    mut commands: Commands,
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
                let create = StartRequest::from_prompt(
                    payload.prompt,
                    std::path::PathBuf::new(),
                    agent,
                    PageOpenTarget::NewStack,
                    Vec::new(),
                );
                let id = create.id().clone();
                session_requests.write(create);
                commands.spawn((
                    Operation::bundle("acp.command.create-session")
                        .unwrap()
                        .running(),
                    id,
                    PendingSessionCreation {
                        request_id: request.request_id,
                    },
                ));
                continue;
            }
            None => AgentCommandResult::Error("no agent is installed".to_string()),
        };
        responses.write(AgentReply::new(request.request_id).response(result));
    }
}

fn list_agents(
    mut requests: MessageReader<AgentRequestReady>,
    catalog: Query<&RegistryAgent, With<InstalledAgent>>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let Ok(Some(AgentListAgents)) = request.decode::<AgentListAgents>() else {
            continue;
        };
        let mut agents = Vec::new();
        for agent in &catalog {
            agents.push(vmux_api::session::RemoteAgent {
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
