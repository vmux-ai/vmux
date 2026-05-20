use std::path::PathBuf;

use bevy::prelude::*;
use vmux_server::{PageConfig, Server};

pub struct CommandBarPagePlugin;

impl Plugin for CommandBarPagePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Server>();
        app.world_mut().resource_mut::<Server>().register(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            &PageConfig::with_custom_host("command-bar"),
        );
    }
}
