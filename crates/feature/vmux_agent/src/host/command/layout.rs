use bevy::prelude::*;
use vmux_api::protocol::{
    AgentBookmarkAdd, AgentBookmarkFolderCreate, AgentBookmarkPin, AgentBookmarkPinUrl,
    AgentBookmarkRemove, AgentBookmarkUnpin, AgentCommand as ServiceAgentCommand, AgentSpaceCreate,
    AgentSpaceDelete, AgentSpaceRename,
};
use vmux_service::client::ServiceRequest;

use crate::event::AgentCommandRequest;

use super::{AgentReply, CommandSet};

pub(super) struct LayoutCommandPlugin;

impl Plugin for LayoutCommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentSpaceCreateRequest>()
            .add_message::<AgentSpaceRenameRequest>()
            .add_message::<AgentSpaceDeleteRequest>()
            .add_message::<AgentBookmarkAddRequest>()
            .add_message::<AgentBookmarkRemoveRequest>()
            .add_message::<AgentBookmarkPinRequest>()
            .add_message::<AgentBookmarkPinUrlRequest>()
            .add_message::<AgentBookmarkUnpinRequest>()
            .add_message::<AgentBookmarkFolderCreateRequest>()
            .add_systems(
                Update,
                (
                    create_space,
                    rename_space,
                    delete_space,
                    add_bookmark,
                    remove_bookmark,
                    pin_bookmark,
                    pin_bookmark_url,
                    unpin_bookmark,
                    create_bookmark_folder,
                )
                    .in_set(CommandSet::Commands),
            )
            .add_systems(Update, route_layout_commands.in_set(CommandSet::Dispatch));
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
struct AgentBookmarkAddRequest {
    reply: AgentReply,
    payload: AgentBookmarkAdd,
}

#[derive(Message, Clone)]
struct AgentBookmarkRemoveRequest {
    reply: AgentReply,
    payload: AgentBookmarkRemove,
}

#[derive(Message, Clone)]
struct AgentBookmarkPinRequest {
    reply: AgentReply,
    payload: AgentBookmarkPin,
}

#[derive(Message, Clone)]
struct AgentBookmarkPinUrlRequest {
    reply: AgentReply,
    payload: AgentBookmarkPinUrl,
}

#[derive(Message, Clone)]
struct AgentBookmarkUnpinRequest {
    reply: AgentReply,
    payload: AgentBookmarkUnpin,
}

#[derive(Message, Clone)]
struct AgentBookmarkFolderCreateRequest {
    reply: AgentReply,
    payload: AgentBookmarkFolderCreate,
}

#[allow(clippy::too_many_arguments)]
fn route_layout_commands(
    mut commands: MessageReader<AgentCommandRequest>,
    mut space_create: MessageWriter<AgentSpaceCreateRequest>,
    mut space_rename: MessageWriter<AgentSpaceRenameRequest>,
    mut space_delete: MessageWriter<AgentSpaceDeleteRequest>,
    mut bookmark_add: MessageWriter<AgentBookmarkAddRequest>,
    mut bookmark_remove: MessageWriter<AgentBookmarkRemoveRequest>,
    mut bookmark_pin: MessageWriter<AgentBookmarkPinRequest>,
    mut bookmark_pin_url: MessageWriter<AgentBookmarkPinUrlRequest>,
    mut bookmark_unpin: MessageWriter<AgentBookmarkUnpinRequest>,
    mut bookmark_folder_create: MessageWriter<AgentBookmarkFolderCreateRequest>,
) {
    for request in commands.read() {
        let reply = AgentReply::new(request.request_id);
        match &request.command {
            ServiceAgentCommand::SpaceCreate(payload) => {
                space_create.write(AgentSpaceCreateRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::SpaceRename(payload) => {
                space_rename.write(AgentSpaceRenameRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::SpaceDelete(payload) => {
                space_delete.write(AgentSpaceDeleteRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::BookmarkAdd(payload) => {
                bookmark_add.write(AgentBookmarkAddRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::BookmarkRemove(payload) => {
                bookmark_remove.write(AgentBookmarkRemoveRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::BookmarkPin(payload) => {
                bookmark_pin.write(AgentBookmarkPinRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::BookmarkPinUrl(payload) => {
                bookmark_pin_url.write(AgentBookmarkPinUrlRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::BookmarkUnpin(payload) => {
                bookmark_unpin.write(AgentBookmarkUnpinRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::BookmarkFolderCreate(payload) => {
                bookmark_folder_create.write(AgentBookmarkFolderCreateRequest {
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
    mut create: MessageWriter<vmux_space::SpaceCreateRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        create.write(vmux_space::SpaceCreateRequest {
            name: request.payload.name.clone().unwrap_or_default(),
        });
        responses.write(request.reply.ok());
    }
}

fn rename_space(
    mut requests: MessageReader<AgentSpaceRenameRequest>,
    mut rename: MessageWriter<vmux_space::SpaceRenameRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        rename.write(vmux_space::SpaceRenameRequest {
            space_id: request.payload.space_id.clone(),
            name: request.payload.name.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn delete_space(
    mut requests: MessageReader<AgentSpaceDeleteRequest>,
    mut delete: MessageWriter<vmux_space::SpaceDeleteRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        delete.write(vmux_space::SpaceDeleteRequest {
            space_id: request.payload.space_id.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn add_bookmark(
    mut requests: MessageReader<AgentBookmarkAddRequest>,
    mut add: MessageWriter<vmux_layout::bookmark::AddRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        add.write(vmux_layout::bookmark::AddRequest {
            metadata: vmux_core::PageMetadata {
                title: request.payload.page.title.clone().unwrap_or_default(),
                url: request.payload.page.url.clone(),
                icon: vmux_core::PageIcon::favicon(
                    request.payload.page.favicon_url.clone().unwrap_or_default(),
                ),
                bg_color: None,
            },
            folder: request.payload.folder.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn remove_bookmark(
    mut requests: MessageReader<AgentBookmarkRemoveRequest>,
    mut remove: MessageWriter<vmux_layout::bookmark::RemoveRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        remove.write(vmux_layout::bookmark::RemoveRequest {
            uuid: request.payload.uuid.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn pin_bookmark(
    mut requests: MessageReader<AgentBookmarkPinRequest>,
    mut pin: MessageWriter<vmux_layout::bookmark::PinRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        pin.write(vmux_layout::bookmark::PinRequest {
            uuid: request.payload.uuid.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn pin_bookmark_url(
    mut requests: MessageReader<AgentBookmarkPinUrlRequest>,
    mut pin: MessageWriter<vmux_layout::bookmark::PinUrlRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        pin.write(vmux_layout::bookmark::PinUrlRequest {
            metadata: vmux_core::PageMetadata {
                title: request.payload.page.title.clone().unwrap_or_default(),
                url: request.payload.page.url.clone(),
                icon: vmux_core::PageIcon::favicon(
                    request.payload.page.favicon_url.clone().unwrap_or_default(),
                ),
                bg_color: None,
            },
        });
        responses.write(request.reply.ok());
    }
}

fn unpin_bookmark(
    mut requests: MessageReader<AgentBookmarkUnpinRequest>,
    mut unpin: MessageWriter<vmux_layout::bookmark::UnpinRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        unpin.write(vmux_layout::bookmark::UnpinRequest {
            uuid: request.payload.uuid.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn create_bookmark_folder(
    mut requests: MessageReader<AgentBookmarkFolderCreateRequest>,
    mut create: MessageWriter<vmux_layout::bookmark::CreateFolderRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        create.write(vmux_layout::bookmark::CreateFolderRequest::root(
            request.payload.name.clone(),
        ));
        responses.write(request.reply.ok());
    }
}
