use bevy::prelude::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

pub struct NativePageRuntimePlugin;

impl Plugin for NativePageRuntimePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(target_os = "macos")]
        app.add_plugins(macos::NativePageMacosPlugin);
        #[cfg(not(target_os = "macos"))]
        app.add_plugins(other::NativePageOtherPlugin);
    }
}
