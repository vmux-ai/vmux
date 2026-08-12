use bevy::prelude::*;

/// Registers the services/processes-monitor webview page; the persistent-process server
/// itself runs in the `vmux_service` binary.
pub struct ServicePlugin;

impl Plugin for ServicePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
    }
}
