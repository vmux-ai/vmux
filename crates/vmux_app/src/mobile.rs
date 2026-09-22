use bevy_app::{App, Plugin};

pub struct VmuxMobilePlugin;

impl Plugin for VmuxMobilePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_start::roster::StartRosterPlugin,
            vmux_team::roster::TeamRosterPlugin,
            vmux_chat::room::ChatRoomPlugin,
            vmux_chat::prompt::ChatPromptPlugin,
            vmux_chat::model::ChatModelPlugin,
        ));
    }
}
