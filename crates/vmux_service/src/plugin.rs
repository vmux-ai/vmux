use std::path::PathBuf;

use bevy::prelude::*;
use vmux_page::{PageConfig, PageRegistry};

pub struct ServicePlugin;

impl Plugin for ServicePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().resource_mut::<PageRegistry>().register(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            &PageConfig::with_custom_host("services"),
        );
    }
}
