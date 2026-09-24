mod catalog;
mod popup;
mod web_store;

use bevy::prelude::*;

pub(crate) use popup::{ExtensionPopup, ExtensionPopupBounds, ExtensionPopupPresented};

#[derive(Component, Default)]
pub struct Extensions;

impl vmux_layout::native_open::HostedPage for Extensions {
    const HOST: &'static str = "extensions";
    const URL: &'static str = vmux_core::event::EXTENSIONS_PAGE_URL;
    const TITLE: &'static str = "Extensions";
}

pub struct ExtensionsPlugin;

impl Plugin for ExtensionsPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.add_plugins((
            vmux_layout::native_open::HostedPagePlugin::<Extensions>::default(),
            catalog::CatalogPlugin,
            popup::PopupPlugin,
            web_store::WebStorePlugin,
        ));
    }
}

const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "extensions",
    title: "Extensions",
    title_message_id: Some("extensions-title"),
    replaces_command: None,
    keywords: &["extension", "extensions", "addon", "install"],
    icon: Some(vmux_core::BuiltinIcon::Puzzle),
    command_bar: true,
};
