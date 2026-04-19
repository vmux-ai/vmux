use std::path::PathBuf;

use bevy::prelude::*;
use vmux_webview_app::{WebviewAppConfig, WebviewAppRegistry};

pub struct CommandPaletteWebviewPlugin;

impl Plugin for CommandPaletteWebviewPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<WebviewAppRegistry>()
            .register(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                &WebviewAppConfig::with_custom_host("command-palette"),
            );
    }
}
