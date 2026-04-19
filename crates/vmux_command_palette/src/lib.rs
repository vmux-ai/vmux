pub mod event;

#[cfg(not(target_arch = "wasm32"))]
pub mod bundle;

#[cfg(not(target_arch = "wasm32"))]
pub use bundle::COMMAND_PALETTE_WEBVIEW_URL;

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
