use bevy::prelude::*;
use vmux_api::protocol::{
    AgentRenameProfile, AgentSpaceCreate, AgentSpaceDelete, AgentSpaceRename,
};
use vmux_core::agent::{AgentCommandResponse, AgentReply, AgentRequestInput};

pub(super) struct SpaceAgentPlugin;

impl Plugin for SpaceAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentRequestInput>()
            .add_message::<AgentCommandResponse>()
            .add_message::<AgentSpaceCreateRequest>()
            .add_message::<AgentSpaceRenameRequest>()
            .add_message::<AgentSpaceDeleteRequest>()
            .add_message::<AgentRenameProfileRequest>()
            .add_message::<RenameProfileRequest>()
            .add_systems(
                Update,
                (
                    route_space_commands,
                    (
                        create_space,
                        rename_space,
                        delete_space,
                        request_profile_rename,
                    ),
                    rename_profile,
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

#[derive(Message, Clone)]
struct AgentRenameProfileRequest {
    reply: AgentReply,
    payload: AgentRenameProfile,
}

#[derive(Message, Clone)]
struct RenameProfileRequest {
    name: String,
}

fn route_space_commands(
    mut commands: MessageReader<AgentRequestInput>,
    mut create: MessageWriter<AgentSpaceCreateRequest>,
    mut rename: MessageWriter<AgentSpaceRenameRequest>,
    mut delete: MessageWriter<AgentSpaceDeleteRequest>,
    mut rename_profile: MessageWriter<AgentRenameProfileRequest>,
) {
    for request in commands.read() {
        let reply = AgentReply::new(request.request_id);
        if let Ok(Some(payload)) = request.decode::<AgentSpaceCreate>() {
            create.write(AgentSpaceCreateRequest { reply, payload });
        } else if let Ok(Some(payload)) = request.decode::<AgentSpaceRename>() {
            rename.write(AgentSpaceRenameRequest { reply, payload });
        } else if let Ok(Some(payload)) = request.decode::<AgentSpaceDelete>() {
            delete.write(AgentSpaceDeleteRequest { reply, payload });
        } else if let Ok(Some(payload)) = request.decode::<AgentRenameProfile>() {
            rename_profile.write(AgentRenameProfileRequest { reply, payload });
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

fn request_profile_rename(
    mut requests: MessageReader<AgentRenameProfileRequest>,
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
        match vmux_core::profile::set_display_name(name) {
            Ok(()) => {
                if let Ok(mut profile) = profiles.single_mut() {
                    profile.name = name.to_string();
                }
            }
            Err(error) => warn!("rename_profile: failed to persist display name: {error}"),
        }
    }
}
