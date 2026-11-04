use bevy::prelude::*;
use vmux_api::Id;
use vmux_api::protocol::{
    AgentCommandResult, AgentFileSearch, AgentFileTouched, AgentListAgents, AgentNewChat,
    AgentTurnEnded,
};
use vmux_ecs::agent::{AgentCommandResponse, AgentReply, AgentRequestInput};
use vmux_ecs::{Operation, OperationKind, OperationState, PageOpenTarget};
use vmux_session::host::StartRequest;
use vmux_session::{Created, Session};

use super::CommandSet;
use crate::host::acp::registry::RegistryAgent;
use vmux_session::Route as SessionRoute;

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::OperationPlugin>() {
            app.add_plugins(vmux_ecs::OperationPlugin);
        }
        app.add_observer(open_created_session).add_systems(
            Update,
            (
                acknowledge_file_touched,
                acknowledge_file_search,
                acknowledge_turn_ended,
                create_session,
                list_agents,
            )
                .in_set(CommandSet::Commands),
        );
    }
}

#[derive(Component)]
struct PendingSessionCreation {
    session_id: Id<Session>,
    request_id: vmux_api::protocol::AgentRequestId,
}

fn open_created_session(
    trigger: On<Created>,
    mut pending: Query<(Entity, &PendingSessionCreation, &mut OperationState), With<Operation>>,
    mut responses: MessageWriter<AgentCommandResponse>,
    mut commands: Commands,
) {
    let Some((operation, pending, mut state)) = pending.iter_mut().find(|(_, pending, state)| {
        pending.session_id == trigger.event().id
            && matches!(**state, OperationState::Pending | OperationState::Running)
    }) else {
        return;
    };
    responses.write(AgentReply::new(pending.request_id).ok());
    *state = OperationState::Succeeded;
    commands
        .entity(operation)
        .remove::<PendingSessionCreation>();
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

fn create_session(
    mut requests: MessageReader<AgentRequestInput>,
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
                    Operation,
                    Id::<Operation>::new(uuid::Uuid::new_v4().to_string()),
                    OperationKind::new("agent_session_create").unwrap(),
                    OperationState::Running,
                    PendingSessionCreation {
                        session_id: id,
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
            agents.push(vmux_api::session::RemoteAgent {
                id: agent.id.clone(),
                name: agent.name.clone(),
                url: SessionRoute::manager_for_agent(&vmux_session::AgentRef::new(
                    agent.id.clone(),
                )),
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
