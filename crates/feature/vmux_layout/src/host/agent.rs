use bevy::prelude::*;
use vmux_api::protocol::{
    AgentBookmarkAdd, AgentBookmarkFolderCreate, AgentBookmarkPin, AgentBookmarkPinUrl,
    AgentBookmarkRemove, AgentBookmarkUnpin, AgentCommandResult, AgentFocusPane, AgentUpdateLayout,
};
use vmux_core::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
};

use crate::stack::FocusedStack;

pub(super) struct LayoutAgentPlugin;

impl Plugin for LayoutAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_agent_request::<AgentBookmarkAdd>()
            .add_agent_request::<AgentBookmarkRemove>()
            .add_agent_request::<AgentBookmarkPin>()
            .add_agent_request::<AgentBookmarkPinUrl>()
            .add_agent_request::<AgentBookmarkUnpin>()
            .add_agent_request::<AgentBookmarkFolderCreate>()
            .add_agent_request::<AgentFocusPane>()
            .add_agent_request::<AgentUpdateLayout>()
            .add_message::<FocusPaneRequest>()
            .add_systems(
                Update,
                (
                    add_bookmark,
                    remove_bookmark,
                    pin_bookmark,
                    pin_bookmark_url,
                    unpin_bookmark,
                    create_bookmark_folder,
                )
                    .after(AgentRequestRouteSet),
            )
            .add_systems(
                Update,
                (request_focus, focus_pane, update_layout)
                    .chain()
                    .after(AgentRequestRouteSet),
            );
    }
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
    fn of(focus: &crate::active_pane::ActiveStack) -> Self {
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

fn request_focus(
    mut requests: MessageReader<AgentRequestMessage<AgentFocusPane>>,
    mut focus: MessageWriter<FocusPaneRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let result = if !request.origin.is_agent() {
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
    mut requests: MessageReader<AgentRequestMessage<AgentUpdateLayout>>,
    focus: FocusedStack,
    mut apply: MessageWriter<crate::apply::LayoutApplyRequest>,
) {
    for request in requests.read() {
        let mut snapshot = request.payload.layout.clone();
        if request.origin.is_agent() {
            CurrentFocus::of(&focus).preserve_in(&mut snapshot);
        }
        apply.write(crate::apply::LayoutApplyRequest {
            request_id: request.reply.request_id.0,
            snapshot,
        });
    }
}

fn add_bookmark(
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkAdd>>,
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
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkRemove>>,
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
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkPin>>,
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
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkPinUrl>>,
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
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkUnpin>>,
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
    mut requests: MessageReader<AgentRequestMessage<AgentBookmarkFolderCreate>>,
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
