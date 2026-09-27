use bevy::prelude::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

pub struct BrowserPlatformPlugin;

impl Plugin for BrowserPlatformPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(target_os = "macos")]
        app.add_plugins(macos::MacosBrowserPlugin);
        #[cfg(not(target_os = "macos"))]
        app.add_plugins(other::FallbackBrowserPlugin);
    }
}
