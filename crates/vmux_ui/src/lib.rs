//! UI helpers for vmux: **Bevy-side** tokens in [`utils`] and **WASM-only** Dioxus [`hooks`] / [`components`].
//!
//! - **Native / default:** [`utils`] (requires the `bevy` feature, on by default).
//! - **WASM:** [`hooks`], [`components`] — e.g. [`hooks::use_eval_loop`] for `document::eval` bridges.

#[cfg(feature = "bevy")]
pub mod utils;

#[cfg(feature = "bevy")]
pub mod prelude {
    pub use crate::utils::color;
}

#[cfg(target_arch = "wasm32")]
pub mod components;

#[cfg(target_arch = "wasm32")]
pub mod hooks;
