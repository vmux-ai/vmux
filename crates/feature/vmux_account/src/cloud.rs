use bevy_app::App;

#[cfg(target_os = "macos")]
mod credentials;
mod transport;
#[cfg(not(target_os = "macos"))]
mod unavailable;

pub struct AccountCloudPlugin;

impl bevy_app::Plugin for AccountCloudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::availability::AvailabilityPlugin,
            transport::TransportPlugin,
        ));
        #[cfg(target_os = "macos")]
        app.add_plugins(credentials::CredentialsPlugin);
        #[cfg(not(target_os = "macos"))]
        app.add_plugins(unavailable::UnavailablePlugin);
    }
}
