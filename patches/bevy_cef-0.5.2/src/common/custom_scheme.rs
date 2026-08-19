use bevy::prelude::*;

pub mod asset_loader;
pub(crate) mod responser;

use crate::common::custom_scheme::asset_loader::LocalSchemeAssetLoaderPlugin;

/// A plugin that adds support for handling local scheme requests in Bevy applications.
pub(crate) struct CustomSchemePlugin;

impl Plugin for CustomSchemePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((responser::ResponserPlugin, LocalSchemeAssetLoaderPlugin));
    }
}
