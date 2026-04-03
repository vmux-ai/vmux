//! Bevy-native UI: embedded `dist/` serving, CEF-hosted Dioxus bundles, and [`native`] design tokens.
//!
//! The Dioxus WASM component library lives in **`vmux_ui`**. This crate wires those bundles into the
//! vmux desktop stack ([`UiPlugin`], [`UiLibraryPlugin`], [`hosted`]).
//!
//! Use [`extract_embedded_dist_to_temp`] with [`rust_embed::RustEmbed`] for release bundles served from
//! temp dirs. Register [`register_ui_plugin_dioxus_warmup`] after [`vmux_server::ServerPlugin`].

pub const UI_LIBRARY_DIST_DIR_NAME: &str = "dist";

pub mod embedded_dist;
pub mod hosted;
pub mod native;

pub use embedded_dist::extract_embedded_dist_to_temp;
pub use hosted::{
    UiLibraryServerPlugin, UiLibraryUiPlugin, UiLibraryUrlReceiver, UiPlugin,
    register_ui_plugin_dioxus_warmup,
};
pub use vmux_server::{
    ServePlugin, push_dioxus_warmup_descriptor, register_serve_plugin_dioxus_warmup,
};

#[derive(Default)]
pub struct UiLibraryPlugin;

impl bevy::prelude::Plugin for UiLibraryPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins((UiLibraryServerPlugin, UiLibraryUiPlugin));
    }
}

pub mod prelude {
    pub use crate::native::utils::color;
}
