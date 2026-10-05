use bevy::prelude::*;

mod popup;
mod web_store;

pub(super) struct ManagerPagePlugin;

impl Plugin for ManagerPagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((popup::PopupPlugin, web_store::WebStorePlugin));
    }
}
