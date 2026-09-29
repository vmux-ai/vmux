use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_core::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
};

#[vmux_api::agent]
pub struct AgentRenameProfile {
    pub name: String,
}

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
        app.add_agent_request::<AgentSpaceCreate>()
            .add_agent_request::<AgentSpaceRename>()
            .add_agent_request::<AgentSpaceDelete>()
            .add_agent_request::<AgentRenameProfile>()
            .add_message::<RenameProfileRequest>()
            .add_systems(
                Update,
                (
                    (
                        create_space,
                        rename_space,
                        delete_space,
                        request_profile_rename,
                    )
                        .after(AgentRequestRouteSet),
                    rename_profile,
                )
                    .chain(),
            );
    }
}

#[derive(Message, Clone)]
struct RenameProfileRequest {
    name: String,
}

fn create_space(
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

fn rename_space(
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

fn delete_space(
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

fn request_profile_rename(
    mut requests: MessageReader<AgentRequestMessage<AgentRenameProfile>>,
    mut rename: MessageWriter<RenameProfileRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        rename.write(RenameProfileRequest {
            name: request.payload.name.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn rename_profile(
    mut requests: MessageReader<RenameProfileRequest>,
    mut profiles: Query<
        &mut vmux_layout::profile::Profile,
        (
            With<vmux_layout::space::Space>,
            With<vmux_layout::space::CurrentSpace>,
        ),
    >,
) {
    for request in requests.read() {
        let name = request.name.trim();
        if name.is_empty() {
            continue;
        }
        match vmux_core::profile::Profile::current().set_display_name(name) {
            Ok(()) => {
                if let Ok(mut profile) = profiles.single_mut() {
                    profile.name = name.to_string();
                }
            }
            Err(error) => warn!("rename_profile: failed to persist display name: {error}"),
        }
    }
}
