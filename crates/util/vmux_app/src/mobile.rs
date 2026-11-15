use bevy_app::{App, Plugin};

pub struct VmuxMobilePlugin;

impl Plugin for VmuxMobilePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_start::roster::RosterPlugin,
            vmux_team::roster::RosterPlugin,
        ));
    }
}
