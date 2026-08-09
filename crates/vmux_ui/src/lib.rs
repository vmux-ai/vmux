//! Dioxus WASM UI library: [`components`] and [`hooks`], over the [`host`] seam that carries CEF
//! IPC.
//!
//! Bevy-side hosting, embedded `dist/` serving, and GPU/UI tokens live in **`vmux_ui_native`**.

pub mod agent_accent;

pub mod favicon;

pub mod file_icon;

pub mod icon;

mod i18n_catalogs {
    include!(concat!(env!("OUT_DIR"), "/i18n_catalogs.rs"));
}

pub mod i18n;

pub mod prompt_ghost;

pub mod theme;

mod listener_guard;

pub mod host;

#[cfg(web)]
pub mod key_stroke;

pub mod list_nav;

pub mod hooks;

pub mod components;

pub mod platform;

pub mod util;

pub mod dioxus_ext {
    pub use dioxus_primitives::dioxus_attributes::attributes;
    pub use dioxus_primitives::merge_attributes;
}
