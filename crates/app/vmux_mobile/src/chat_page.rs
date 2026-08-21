//! Serving `vmux://chat/` out of the world.
//!
//! The same split as [`start_page`](crate::start_page): [`vmux_chat::room`] keeps the snapshot
//! current and knows nothing about how a page is reached, and the id it is delivered under lives
//! here, in the app that owns the pages.

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_chat::event::CHAT_SNAPSHOT_EVENT;
use vmux_chat::room::{ChatRoomPlugin, RoomProjection, Snapshot};

use crate::world::PageEmit;

/// The open conversation, model and delivery both.
pub struct ChatPagePlugin;

impl Plugin for ChatPagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ChatRoomPlugin).add_systems(
            Update,
            emit_snapshot
                .after(RoomProjection)
                .run_if(resource_changed::<Snapshot>),
        );
    }
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
