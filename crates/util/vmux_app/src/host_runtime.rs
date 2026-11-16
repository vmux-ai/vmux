use bevy_app::{App, Plugin};

pub struct SessionHostRuntimePlugin;

impl Plugin for SessionHostRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_session::SessionHostPlugin,
            vmux_service::server::ServiceDaemonPlugin,
            vmux_acp::AcpServicePlugin,
        ));
    }
}
