use bevy_ecs::prelude::*;
use vmux_feature::CliCommandManifest;

#[derive(Component)]
pub(super) struct CliCatalog {
    pub(super) default: String,
    pub(super) commands: Vec<CliCommandManifest>,
}
