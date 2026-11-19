use bevy_app::App;

#[cfg(target_os = "macos")]
mod credentials;
pub(crate) mod transport;
#[cfg(not(target_os = "macos"))]
mod unavailable;

pub struct AccountCloudPlugin;

impl bevy_app::Plugin for AccountCloudPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "profile-sync")]
        app.add_plugins(crate::sync_runtime::ProfileSyncPlugin);
        app.add_plugins((
            crate::availability::AvailabilityPlugin,
            transport::TransportPlugin,
            crate::control::ControlBrokerPlugin,
        ));
        #[cfg(target_os = "macos")]
        app.add_plugins(credentials::CredentialsPlugin);
        #[cfg(not(target_os = "macos"))]
        app.add_plugins(unavailable::UnavailablePlugin);
    }
}
