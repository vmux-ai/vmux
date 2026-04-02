//! UI helpers: **[`native`]** (Bevy / GPU tokens), **hosted Dioxus UIs** ([`UiPlugin`], [`UiLibraryPlugin`], [`hosted`]), and **[`webview`]** (Dioxus WASM).
//!
//! - **Native:** [`native`] — [`native::utils`] (e.g. linear accent for shaders).
//! - **Hosted UI (native):** [`UiPlugin`] + [`ServePlugin`] supply [`register_ui_plugin_dioxus_warmup`] / [`register_serve_plugin_dioxus_warmup`] (after [`vmux_server::ServerPlugin`]); [`UiLibraryPlugin`] registers [`UiLibraryServerPlugin`] + [`UiLibraryUiPlugin`] (bundle from `dist/`; see `build.rs`). Use [`extract_embedded_dist_to_temp`] for `rust-embed` → temp dir.
//! - **Webview:** [`webview`] — [`webview::components`], [`webview::cef_bridge`], [`webview::hooks`], [`webview::web_color`].
//! - **CEF listen / host emit (WASM):** [`hooks`] — [`crate::hooks::use_event_listener`] for RON payloads from Bevy.

/// Directory name for the UI library bundle (`build.rs` writes here on debug native builds).
#[cfg(all(not(target_arch = "wasm32"), feature = "bevy"))]
pub const UI_LIBRARY_DIST_DIR_NAME: &str = "dist";

#[cfg(all(not(target_arch = "wasm32"), feature = "bevy"))]
pub mod embedded_dist;

#[cfg(all(not(target_arch = "wasm32"), feature = "bevy"))]
pub mod hosted;

#[cfg(feature = "bevy")]
pub mod native;

#[cfg(target_arch = "wasm32")]
pub mod hooks;

#[cfg(target_arch = "wasm32")]
pub mod webview;

/// Re-exports for composing [`webview::components`] (`Button`, `Input`, …) with `attributes!` / [`merge_attributes`].
#[cfg(target_arch = "wasm32")]
pub mod dioxus_ext {
    pub use dioxus_primitives::dioxus_attributes::attributes;
    pub use dioxus_primitives::merge_attributes;
}

#[cfg(all(not(target_arch = "wasm32"), feature = "bevy"))]
pub use embedded_dist::extract_embedded_dist_to_temp;

#[cfg(all(not(target_arch = "wasm32"), feature = "bevy"))]
pub use hosted::{
    UiLibraryServerPlugin, UiLibraryUiPlugin, UiLibraryUrlReceiver, UiPlugin,
    register_ui_plugin_dioxus_warmup,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "bevy"))]
pub use vmux_server::{
    ServePlugin, push_dioxus_warmup_descriptor, register_serve_plugin_dioxus_warmup,
};

/// Registers [`UiLibraryServerPlugin`] and [`UiLibraryUiPlugin`] (native, non-wasm; serves `dist/` when present).
#[cfg(all(not(target_arch = "wasm32"), feature = "bevy"))]
#[derive(Default)]
pub struct UiLibraryPlugin;

#[cfg(all(not(target_arch = "wasm32"), feature = "bevy"))]
impl bevy::prelude::Plugin for UiLibraryPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins((UiLibraryServerPlugin, UiLibraryUiPlugin));
    }
}

#[cfg(feature = "bevy")]
pub mod prelude {
    pub use crate::native::utils::color;
}
