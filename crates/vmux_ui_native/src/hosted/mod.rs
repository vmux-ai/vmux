//! Hosted Dioxus UIs in CEF: loopback static hosting + Bevy wiring.

mod ui_library_server;
mod ui_library_ui;

pub mod history;
pub mod status_bar;

pub use ui_library_server::{UiLibraryServerPlugin, UiLibraryUrlReceiver};
pub use ui_library_ui::UiLibraryUiPlugin;

use bevy::prelude::{App, Plugin};
use vmux_server::{DioxusWarmupDescriptor, push_dioxus_warmup_descriptor};

/// Bevy plugin for a hosted Dioxus WASM bundle (served over loopback, loaded in CEF).
///
/// Implemented by [`UiLibraryUiPlugin`] and by feature UIs in [`history`] / [`status_bar`].
///
/// For Dioxus bundles that need warmup without a [`vmux_server::ServePlugin`] sibling, implement
/// [`UiPlugin::dioxus_warmup_descriptor`] and call [`register_ui_plugin_dioxus_warmup`] in
/// [`Plugin::build`]. Most stacks register warmup on the [`vmux_server::ServePlugin`] that serves
/// the `dist/` instead.
pub trait UiPlugin: Plugin {
    /// Optional Dioxus warmup; default is [`None`].
    fn dioxus_warmup_descriptor() -> Option<DioxusWarmupDescriptor> {
        None
    }
}

/// Register [`UiPlugin::dioxus_warmup_descriptor`] into [`vmux_server::DioxusWarmupRegistry`].
///
/// Requires [`vmux_server::ServerPlugin`] to run before this plugin.
pub fn register_ui_plugin_dioxus_warmup<P: UiPlugin>(app: &mut App) {
    push_dioxus_warmup_descriptor(app, P::dioxus_warmup_descriptor());
}
