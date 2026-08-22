//! The plugin groups the phone's world composes.
//!
//! One entry per page the world answers, named in one place rather than assembled inline where the
//! world is built — the same reason `vmux_desktop::plugins` exists, and the same shape.
//!
//! Each of these is the page's own crate. Nothing here adapts them: a page crate keeps its payload
//! current and says which id it goes out under, and this app supplies only the transport that
//! carries it. That is why the list is names and nothing else.

use bevy_app::{PluginGroup, PluginGroupBuilder};
use vmux_chat::model::ChatModelPlugin;
use vmux_chat::prompt::ChatPromptPlugin;
use vmux_chat::room::ChatRoomPlugin;
use vmux_start::roster::StartRosterPlugin;
use vmux_team::roster::TeamRosterPlugin;

/// Every page the world keeps current.
///
/// Order is not load-bearing: each owns its own resources and emits under its own id, and none
/// reads another's.
pub(crate) struct PagePlugins;

impl PluginGroup for PagePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(StartRosterPlugin)
            .add(TeamRosterPlugin)
            .add(ChatRoomPlugin)
            .add(ChatPromptPlugin)
            .add(ChatModelPlugin)
    }
}
