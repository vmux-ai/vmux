use bevy_app::App;

#[cfg(target_os = "macos")]
mod credentials;
mod transport;
#[cfg(not(target_os = "macos"))]
mod unavailable;

pub struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(transport::Plugin);
        #[cfg(target_os = "macos")]
        app.add_plugins(credentials::Plugin);
        #[cfg(not(target_os = "macos"))]
        app.add_plugins(unavailable::Plugin);
    }
}
