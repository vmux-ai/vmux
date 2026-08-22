//! The plugin groups the phone's world composes.
//!
//! One entry per page the world answers, named in one place rather than assembled inline where the
//! world is built — the same reason `vmux_desktop::plugins` exists, and the same shape.

use bevy_app::{PluginGroup, PluginGroupBuilder};

use crate::chat_page::ChatPagePlugin;
use crate::start_page::StartPagePlugin;
use crate::team_page::TeamPagePlugin;

/// Every page the world keeps current.
///
/// Order is not load-bearing: each owns its own resources and emits under its own id, and none
/// reads another's.
pub(crate) struct PagePlugins;

impl PluginGroup for PagePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(StartPagePlugin)
            .add(TeamPagePlugin)
            .add(ChatPagePlugin)
    }
}
