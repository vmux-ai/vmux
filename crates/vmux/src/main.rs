//! vmux binary entrypoint.

use bevy::prelude::*;
use bevy_cef::prelude::*;

use vmux::{VmuxPlugin, cef_root_cache_path};

fn main() {
    #[cfg(not(target_os = "macos"))]
    bevy_cef::prelude::early_exit_if_subprocess();

    let cef_plugin = CefPlugin {
        command_line_config: CommandLineConfig {
            switches: vec![],
            switch_values: vec![],
        },
        root_cache_path: cef_root_cache_path(),
        ..Default::default()
    };

    App::new()
        .add_plugins((DefaultPlugins, cef_plugin, VmuxPlugin::default()))
        .run();
}
