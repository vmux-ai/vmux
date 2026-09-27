use bevy::prelude::*;
use vmux_api::protocol::{
    AgentBookmarkAdd, AgentBookmarkFolderCreate, AgentBookmarkPin, AgentBookmarkPinUrl,
    AgentBookmarkRemove, AgentBookmarkUnpin, AgentCommand,
};
use vmux_core::agent::{AgentCommandRequest, AgentCommandResponse, AgentReply};

pub(super) struct BookmarkAgentPlugin;

impl Plugin for BookmarkAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentCommandRequest>()
            .add_message::<AgentCommandResponse>()
            .add_message::<AgentBookmarkAddRequest>()
            .add_message::<AgentBookmarkRemoveRequest>()
            .add_message::<AgentBookmarkPinRequest>()
            .add_message::<AgentBookmarkPinUrlRequest>()
            .add_message::<AgentBookmarkUnpinRequest>()
            .add_message::<AgentBookmarkFolderCreateRequest>()
            .add_systems(
                Update,
                (
                    route_bookmark_commands,
                    (
                        add_bookmark,
                        remove_bookmark,
                        pin_bookmark,
                        pin_bookmark_url,
                        unpin_bookmark,
                        create_bookmark_folder,
                    ),
                )
                    .chain(),
            );
    }
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

fn route_bookmark_commands(
    mut commands: MessageReader<AgentCommandRequest>,
    mut add: MessageWriter<AgentBookmarkAddRequest>,
    mut remove: MessageWriter<AgentBookmarkRemoveRequest>,
    mut pin: MessageWriter<AgentBookmarkPinRequest>,
    mut pin_url: MessageWriter<AgentBookmarkPinUrlRequest>,
    mut unpin: MessageWriter<AgentBookmarkUnpinRequest>,
    mut create_folder: MessageWriter<AgentBookmarkFolderCreateRequest>,
) {
    for request in commands.read() {
        let reply = AgentReply::new(request.request_id);
        match &request.command {
            AgentCommand::BookmarkAdd(payload) => {
                add.write(AgentBookmarkAddRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            AgentCommand::BookmarkRemove(payload) => {
                remove.write(AgentBookmarkRemoveRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            AgentCommand::BookmarkPin(payload) => {
                pin.write(AgentBookmarkPinRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            AgentCommand::BookmarkPinUrl(payload) => {
                pin_url.write(AgentBookmarkPinUrlRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            AgentCommand::BookmarkUnpin(payload) => {
                unpin.write(AgentBookmarkUnpinRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            AgentCommand::BookmarkFolderCreate(payload) => {
                create_folder.write(AgentBookmarkFolderCreateRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            _ => {}
        }
    }
}

fn add_bookmark(
    mut requests: MessageReader<AgentBookmarkAddRequest>,
    mut add: MessageWriter<crate::bookmark::AddRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        add.write(crate::bookmark::AddRequest {
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
    mut remove: MessageWriter<crate::bookmark::RemoveRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        remove.write(crate::bookmark::RemoveRequest {
            uuid: request.payload.uuid.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn pin_bookmark(
    mut requests: MessageReader<AgentBookmarkPinRequest>,
    mut pin: MessageWriter<crate::bookmark::PinRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        pin.write(crate::bookmark::PinRequest {
            uuid: request.payload.uuid.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn pin_bookmark_url(
    mut requests: MessageReader<AgentBookmarkPinUrlRequest>,
    mut pin: MessageWriter<crate::bookmark::PinUrlRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        pin.write(crate::bookmark::PinUrlRequest {
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
    mut unpin: MessageWriter<crate::bookmark::UnpinRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        unpin.write(crate::bookmark::UnpinRequest {
            uuid: request.payload.uuid.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn create_bookmark_folder(
    mut requests: MessageReader<AgentBookmarkFolderCreateRequest>,
    mut create: MessageWriter<crate::bookmark::CreateFolderRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        create.write(crate::bookmark::CreateFolderRequest::root(
            request.payload.name.clone(),
        ));
        responses.write(request.reply.ok());
    }
}
