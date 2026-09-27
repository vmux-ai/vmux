use bevy::prelude::*;
use vmux_api::protocol::{
    AgentBookmarkAdd, AgentBookmarkFolderCreate, AgentBookmarkPin, AgentBookmarkPinUrl,
    AgentBookmarkRemove, AgentBookmarkUnpin, AgentCommandResult, AgentFocusPane, AgentUpdateLayout,
};
use vmux_core::agent::{AgentCommandResponse, AgentReply, AgentRequestInput};

use crate::stack::FocusedStack;

pub(super) struct LayoutAgentPlugin;

impl Plugin for LayoutAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentRequestInput>()
            .add_message::<AgentCommandResponse>()
            .add_message::<AgentBookmarkAddRequest>()
            .add_message::<AgentBookmarkRemoveRequest>()
            .add_message::<AgentBookmarkPinRequest>()
            .add_message::<AgentBookmarkPinUrlRequest>()
            .add_message::<AgentBookmarkUnpinRequest>()
            .add_message::<AgentBookmarkFolderCreateRequest>()
            .add_message::<AgentFocusPaneRequest>()
            .add_message::<AgentUpdateLayoutRequest>()
            .add_message::<FocusPaneRequest>()
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
            )
            .add_systems(
                Update,
                (
                    route_layout_commands,
                    request_focus,
                    focus_pane,
                    update_layout,
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

#[derive(Message, Clone)]
struct AgentFocusPaneRequest {
    reply: AgentReply,
    allowed: bool,
    payload: AgentFocusPane,
}

#[derive(Message, Clone)]
struct AgentUpdateLayoutRequest {
    reply: AgentReply,
    from_agent: bool,
    payload: AgentUpdateLayout,
}

#[derive(Message, Clone)]
struct FocusPaneRequest {
    pane: String,
}

#[derive(Clone, Copy)]
struct CurrentFocus {
    tab: Option<Entity>,
    pane: Option<Entity>,
    stack: Option<Entity>,
}

impl CurrentFocus {
    fn of(focus: &FocusedStack) -> Self {
        Self {
            tab: focus.tab,
            pane: focus.pane,
            stack: focus.stack,
        }
    }

    fn preserve_in(self, snapshot: &mut vmux_api::protocol::layout::LayoutSnapshot) {
        snapshot.focused = vmux_api::protocol::layout::Focus {
            tab: self.id(crate::protocol::NodeKind::Tab, self.tab),
            pane: self.id(crate::protocol::NodeKind::Pane, self.pane),
            stack: self.id(crate::protocol::NodeKind::Stack, self.stack),
        };
        if let Some(tab) = snapshot.focused.tab.as_deref() {
            for item in &mut snapshot.tabs {
                item.is_active = item.id.as_deref() == Some(tab);
            }
        }
    }

    fn id(self, kind: crate::protocol::NodeKind, entity: Option<Entity>) -> Option<String> {
        entity.map(|entity| crate::protocol::format_id(kind, entity.to_bits()))
    }
}

fn route_bookmark_commands(
    mut commands: MessageReader<AgentRequestInput>,
    mut add: MessageWriter<AgentBookmarkAddRequest>,
    mut remove: MessageWriter<AgentBookmarkRemoveRequest>,
    mut pin: MessageWriter<AgentBookmarkPinRequest>,
    mut pin_url: MessageWriter<AgentBookmarkPinUrlRequest>,
    mut unpin: MessageWriter<AgentBookmarkUnpinRequest>,
    mut create_folder: MessageWriter<AgentBookmarkFolderCreateRequest>,
) {
    for request in commands.read() {
        let reply = AgentReply::new(request.request_id);
        if let Ok(Some(payload)) = request.decode::<AgentBookmarkAdd>() {
            add.write(AgentBookmarkAddRequest { reply, payload });
        } else if let Ok(Some(payload)) = request.decode::<AgentBookmarkRemove>() {
            remove.write(AgentBookmarkRemoveRequest { reply, payload });
        } else if let Ok(Some(payload)) = request.decode::<AgentBookmarkPin>() {
            pin.write(AgentBookmarkPinRequest { reply, payload });
        } else if let Ok(Some(payload)) = request.decode::<AgentBookmarkPinUrl>() {
            pin_url.write(AgentBookmarkPinUrlRequest { reply, payload });
        } else if let Ok(Some(payload)) = request.decode::<AgentBookmarkUnpin>() {
            unpin.write(AgentBookmarkUnpinRequest { reply, payload });
        } else if let Ok(Some(payload)) = request.decode::<AgentBookmarkFolderCreate>() {
            create_folder.write(AgentBookmarkFolderCreateRequest { reply, payload });
        }
    }
}

fn route_layout_commands(
    mut commands: MessageReader<AgentRequestInput>,
    mut focus: MessageWriter<AgentFocusPaneRequest>,
    mut update_layout: MessageWriter<AgentUpdateLayoutRequest>,
) {
    for request in commands.read() {
        let reply = AgentReply::new(request.request_id);
        if let Ok(Some(payload)) = request.decode::<AgentFocusPane>() {
            focus.write(AgentFocusPaneRequest {
                reply,
                allowed: !request.origin.is_agent(),
                payload,
            });
        } else if let Ok(Some(payload)) = request.decode::<AgentUpdateLayout>() {
            update_layout.write(AgentUpdateLayoutRequest {
                reply,
                from_agent: request.origin.is_agent(),
                payload,
            });
        }
    }
}

fn request_focus(
    mut requests: MessageReader<AgentFocusPaneRequest>,
    mut focus: MessageWriter<FocusPaneRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let result = if request.allowed {
            focus.write(FocusPaneRequest {
                pane: request.payload.pane.clone(),
            });
            AgentCommandResult::Ok
        } else {
            AgentCommandResult::Error("focus_pane is disabled for agents".to_string())
        };
        responses.write(request.reply.response(result));
    }
}

fn focus_pane(mut requests: MessageReader<FocusPaneRequest>, mut commands: Commands) {
    for request in requests.read() {
        let Ok((_, bits)) = crate::protocol::parse_id(&request.pane) else {
            continue;
        };
        commands.trigger(vmux_core::ActivateRequest {
            entity: Entity::from_bits(bits),
        });
    }
}

fn update_layout(
    mut requests: MessageReader<AgentUpdateLayoutRequest>,
    focus: Res<FocusedStack>,
    mut apply: MessageWriter<crate::apply::LayoutApplyRequest>,
) {
    for request in requests.read() {
        let mut snapshot = request.payload.layout.clone();
        if request.from_agent {
            CurrentFocus::of(&focus).preserve_in(&mut snapshot);
        }
        apply.write(crate::apply::LayoutApplyRequest {
            request_id: request.reply.request_id.0,
            snapshot,
        });
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
