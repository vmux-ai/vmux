//! Serving `vmux://chat/` out of the world.
//!
//! The same split as [`start_page`](crate::start_page): [`vmux_chat::room`] keeps the snapshot
//! current and knows nothing about how a page is reached, and the id it is delivered under lives
//! here, in the app that owns the pages.

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_chat::event::CHAT_SNAPSHOT_EVENT;
use vmux_chat::prompt::{Attachments, ChatPromptPlugin, PromptProjection};
use vmux_chat::room::{ChatRoomPlugin, RoomProjection, Snapshot};
use vmux_wire::prompt_media::{CHAT_ATTACHMENTS_EVENT, ChatAttachments};

use crate::world::PageEmit;

/// The open conversation, model and delivery both.
pub struct ChatPagePlugin;

impl Plugin for ChatPagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((ChatRoomPlugin, ChatPromptPlugin))
            .add_systems(
                Update,
                (
                    emit_snapshot
                        .after(RoomProjection)
                        .run_if(resource_changed::<Snapshot>),
                    emit_attachments
                        .after(PromptProjection)
                        .run_if(resource_changed::<Attachments>),
                ),
            );
    }
}

/// Push the pending attachments to the composer, if one is listening.
///
/// An empty pile is not pushed. The page clears its own pills when it submits, so saying so would
/// only repeat what it already did — and on mount an empty payload would be noise.
fn emit_attachments(attachments: Res<Attachments>, mut emits: MessageWriter<PageEmit>) {
    if attachments.0.is_empty() {
        return;
    }
    let payload = ChatAttachments {
        attachments: attachments.0.clone(),
    };
    let Some(bytes) = crate::page_host::encode(&payload) else {
        return;
    };
    emits.write(PageEmit {
        id: CHAT_ATTACHMENTS_EVENT,
        bytes,
    });
}

/// Push a rebuilt snapshot to the page, if one is listening.
fn emit_snapshot(snapshot: Res<Snapshot>, mut emits: MessageWriter<PageEmit>) {
    let Some(bytes) = crate::page_host::encode(&snapshot.0) else {
        return;
    };
    emits.write(PageEmit {
        id: CHAT_SNAPSHOT_EVENT,
        bytes,
    });
}
