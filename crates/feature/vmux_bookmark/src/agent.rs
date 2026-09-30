use bevy::prelude::*;
use vmux_core::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
};
use vmux_core::{PageIcon, PageMetadata};
use vmux_layout::bookmark::{
    AddRequest, CreateFolderRequest, PinRequest, PinUrlRequest, RemoveRequest, UnpinRequest,
};

#[vmux_api::agent]
pub(crate) struct AgentBookmarkAdd {
    pub page: AgentBookmarkPage,
    pub folder: Option<String>,
}

#[vmux_api::agent]
pub(crate) struct AgentBookmarkRemove {
    pub uuid: String,
}

#[vmux_api::agent]
pub(crate) struct AgentBookmarkPin {
    pub uuid: String,
}

#[vmux_api::agent]
pub(crate) struct AgentBookmarkUnpin {
    pub uuid: String,
}

#[vmux_api::agent]
pub(crate) struct AgentBookmarkPinUrl {
    pub page: AgentBookmarkPage,
}

#[vmux_api::agent]
pub(crate) struct AgentBookmarkFolderCreate {
    pub name: String,
}

#[vmux_api::agent(Copy, Eq)]
pub(crate) struct AgentBookmarkList;

#[vmux_api::contract(Eq)]
pub(crate) struct AgentBookmarkPage {
    pub url: String,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
}

pub(crate) struct BookmarkAgentPlugin;

impl Plugin for BookmarkAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_agent_request::<AgentBookmarkAdd>()
            .add_agent_request::<AgentBookmarkRemove>()
            .add_agent_request::<AgentBookmarkPin>()
            .add_agent_request::<AgentBookmarkPinUrl>()
            .add_agent_request::<AgentBookmarkUnpin>()
            .add_agent_request::<AgentBookmarkFolderCreate>()
            .add_systems(
                Update,
                (add, remove, pin, pin_url, unpin, create_folder).after(AgentRequestRouteSet),
            );
    }
}

fn metadata(page: &AgentBookmarkPage) -> PageMetadata {
    PageMetadata {
        title: page.title.clone().unwrap_or_default(),
        url: page.url.clone(),
        icon: PageIcon::favicon(page.favicon_url.clone().unwrap_or_default()),
        bg_color: None,
    }
}

fn add(
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkAdd>>,
    mut add: MessageWriter<AddRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        add.write(AddRequest {
            metadata: metadata(&request.payload.page),
            folder: request.payload.folder.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn remove(
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkRemove>>,
    mut remove: MessageWriter<RemoveRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        remove.write(RemoveRequest {
            uuid: request.payload.uuid.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn pin(
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkPin>>,
    mut pin: MessageWriter<PinRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        pin.write(PinRequest {
            uuid: request.payload.uuid.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn pin_url(
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkPinUrl>>,
    mut pin: MessageWriter<PinUrlRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        pin.write(PinUrlRequest {
            metadata: metadata(&request.payload.page),
        });
        responses.write(request.reply.ok());
    }
}

fn unpin(
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkUnpin>>,
    mut unpin: MessageWriter<UnpinRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        unpin.write(UnpinRequest {
            uuid: request.payload.uuid.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn create_folder(
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkFolderCreate>>,
    mut create: MessageWriter<CreateFolderRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        create.write(CreateFolderRequest::root(request.payload.name.clone()));
        responses.write(request.reply.ok());
    }
}
