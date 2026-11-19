use bevy_app::{App, Plugin};

pub struct SessionHostRuntimePlugin;

impl Plugin for SessionHostRuntimePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "release-flags")]
        app.insert_resource(crate::feature_flags::WriterRole::daemon());
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
