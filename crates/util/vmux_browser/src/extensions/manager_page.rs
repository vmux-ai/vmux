mod popup;
mod web_store;

use bevy::prelude::*;

pub(crate) use popup::{ExtensionPopup, ExtensionPopupBounds, ExtensionPopupPresented};

pub struct ExtensionBrowserPlugin;

impl Plugin for ExtensionBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((popup::PopupPlugin, web_store::WebStorePlugin));
    }
}
