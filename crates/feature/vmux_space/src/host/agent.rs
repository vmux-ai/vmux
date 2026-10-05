use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_ecs::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
};

#[vmux_api::agent]
pub struct AgentSpaceCreate {
    pub name: Option<String>,
}

#[vmux_api::agent]
pub struct AgentSpaceRename {
    pub space_id: String,
    pub name: String,
}

#[vmux_api::agent]
pub struct AgentSpaceDelete {
    pub space_id: String,
}

#[vmux_api::agent(Copy, Eq)]
pub struct AgentCreateWorktree {
    pub anchor: ProcessId,
}

#[vmux_api::agent(Copy, Eq)]
pub struct AgentChooseWorkspace {
    pub anchor: ProcessId,
}

#[vmux_api::agent]
pub struct AgentCreateWorktreeOnBranch {
    pub anchor: ProcessId,
    pub branch: String,
    pub project: Option<String>,
}

#[vmux_api::agent]
pub struct AgentChooseWorkspaceAtPath {
    pub anchor: ProcessId,
    pub path: String,
}

#[vmux_api::agent]
pub struct AgentPrepareWorktree {
    pub anchor: ProcessId,
    pub path: Option<String>,
    pub task: Option<String>,
    pub create: bool,
}

#[vmux_api::agent(Copy, Eq)]
pub struct AgentListSpaces;

pub(super) struct SpaceAgentPlugin;

impl Plugin for SpaceAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            super::agent_workspace::AgentWorkspaceRequestPlugin,
            super::workspace::WorkspaceAgentPlugin,
        ))
        .add_agent_request::<AgentSpaceCreate>()
        .add_agent_request::<AgentSpaceRename>()
        .add_agent_request::<AgentSpaceDelete>()
        .add_systems(Update, (create, rename, delete).after(AgentRequestRouteSet));
    }
}

fn create(
    mut requests: MessageReader<AgentRequestMessage<AgentSpaceCreate>>,
    mut create: MessageWriter<crate::SpaceCreateRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        create.write(crate::SpaceCreateRequest {
            name: request.payload.name.clone().unwrap_or_default(),
        });
        responses.write(request.reply.ok());
    }
}

fn rename(
    mut requests: MessageReader<AgentRequestMessage<AgentSpaceRename>>,
    mut rename: MessageWriter<crate::SpaceRenameRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        rename.write(crate::SpaceRenameRequest {
            space_id: request.payload.space_id.clone(),
            name: request.payload.name.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn delete(
    mut requests: MessageReader<AgentRequestMessage<AgentSpaceDelete>>,
    mut delete: MessageWriter<crate::SpaceDeleteRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        delete.write(crate::SpaceDeleteRequest {
            space_id: request.payload.space_id.clone(),
        });
        responses.write(request.reply.ok());
    }
}
