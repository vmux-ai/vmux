use bevy_app::{PluginGroup, PluginGroupBuilder};
use vmux_chat::model::ChatModelPlugin;
use vmux_chat::prompt::ChatPromptPlugin;
use vmux_chat::room::ChatRoomPlugin;
use vmux_start::roster::StartRosterPlugin;
use vmux_team::roster::TeamRosterPlugin;

use crate::nav::NavPlugin;
use crate::screen::Shown;

pub(crate) struct PagePlugins;

impl PluginGroup for PagePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(NavPlugin::<Shown>::default())
            .add(StartRosterPlugin)
            .add(TeamRosterPlugin)
            .add(ChatRoomPlugin)
            .add(ChatPromptPlugin)
            .add(ChatModelPlugin)
    }
}
