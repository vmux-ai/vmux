use bevy::prelude::*;
use vmux_api::protocol::{AgentCommand, AgentSpaceCreate, AgentSpaceDelete, AgentSpaceRename};
use vmux_core::agent::{AgentCommandRequest, AgentCommandResponse, AgentReply};

pub(super) struct SpaceAgentPlugin;

impl Plugin for SpaceAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentCommandRequest>()
            .add_message::<AgentCommandResponse>()
            .add_message::<AgentSpaceCreateRequest>()
            .add_message::<AgentSpaceRenameRequest>()
            .add_message::<AgentSpaceDeleteRequest>()
            .add_systems(
                Update,
                (
                    route_space_commands,
                    (create_space, rename_space, delete_space),
                )
                    .chain(),
            );
    }
}

#[derive(Message, Clone)]
struct AgentSpaceCreateRequest {
    reply: AgentReply,
    payload: AgentSpaceCreate,
}

#[derive(Message, Clone)]
struct AgentSpaceRenameRequest {
    reply: AgentReply,
    payload: AgentSpaceRename,
}

#[derive(Message, Clone)]
struct AgentSpaceDeleteRequest {
    reply: AgentReply,
    payload: AgentSpaceDelete,
}

fn route_space_commands(
    mut commands: MessageReader<AgentCommandRequest>,
    mut create: MessageWriter<AgentSpaceCreateRequest>,
    mut rename: MessageWriter<AgentSpaceRenameRequest>,
    mut delete: MessageWriter<AgentSpaceDeleteRequest>,
) {
    for request in commands.read() {
        let reply = AgentReply::new(request.request_id);
        match &request.command {
            AgentCommand::SpaceCreate(payload) => {
                create.write(AgentSpaceCreateRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            AgentCommand::SpaceRename(payload) => {
                rename.write(AgentSpaceRenameRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            AgentCommand::SpaceDelete(payload) => {
                delete.write(AgentSpaceDeleteRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            _ => {}
        }
    }
}

fn create_space(
    mut requests: MessageReader<AgentSpaceCreateRequest>,
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
    mut requests: MessageReader<AgentSpaceRenameRequest>,
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
    mut requests: MessageReader<AgentSpaceDeleteRequest>,
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
