#[cfg(feature = "release-flags")]
use bevy_app::PreStartup;
use bevy_app::{App, Plugin};
#[cfg(feature = "release-flags")]
use bevy_ecs::system::Commands;

pub struct SessionHostRuntimePlugin;

impl Plugin for SessionHostRuntimePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "release-flags")]
        app.add_systems(PreStartup, install_daemon_role);
        if !app.is_plugin_added::<vmux_remote::RemoteDomainPlugin>() {
            app.add_plugins(vmux_remote::RemoteDomainPlugin);
        }
        app.add_plugins((
            vmux_session::SessionHostPlugin,
            vmux_service::server::ServiceDaemonPlugin,
            vmux_acp::AcpServicePlugin,
        ));
    }
}

#[cfg(feature = "release-flags")]
fn install_daemon_role(mut commands: Commands) {
    commands.insert_resource(crate::feature_flags::WriterRole::daemon());
}
