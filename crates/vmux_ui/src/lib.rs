//! Dioxus WASM UI library: [`components`] and [`hooks`], over the [`transport`] seam that carries
//! CEF IPC.
//!
//! Bevy-side hosting, embedded `dist/` serving, and GPU/UI tokens live in **`vmux_ui_native`**.

pub mod agent_accent;

pub mod caret;

pub mod clipboard;

pub mod favicon;

pub mod file_icon;

pub mod focus;

pub mod icon;

mod i18n_catalogs {
    include!(concat!(env!("OUT_DIR"), "/i18n_catalogs.rs"));
}

pub mod i18n;
pub mod launcher;

pub mod prompt_ghost;

pub mod theme;

mod listener_guard;

pub mod transport;

#[cfg(ui)]
pub mod key_stroke;

pub mod list_nav;

pub mod hooks;

pub mod back;

pub mod components;

pub mod platform;

#[cfg(ui)]
pub mod media;

pub mod scroll;

#[cfg(ui)]
pub mod text_run;

pub mod util;

pub mod dioxus_ext {
    pub use dioxus_primitives::dioxus_attributes::attributes;
    pub use dioxus_primitives::merge_attributes;
}
