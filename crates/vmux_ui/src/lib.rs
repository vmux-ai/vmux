//! Dioxus WASM UI library: [`components`] and [`hooks`] for CEF IPC.
//!
//! Bevy-side hosting, embedded `dist/` serving, and GPU/UI tokens live in **`vmux_ui_native`**.

pub mod theme;

#[cfg(target_arch = "wasm32")]
pub mod hooks;

#[cfg(target_arch = "wasm32")]
pub mod components;

#[cfg(target_arch = "wasm32")]
pub mod util;

#[cfg(target_arch = "wasm32")]
pub mod cef_bridge;

#[cfg(target_arch = "wasm32")]
pub mod dioxus_ext {
    pub use dioxus_primitives::dioxus_attributes::attributes;
    pub use dioxus_primitives::merge_attributes;
}
