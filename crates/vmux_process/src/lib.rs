pub mod event;

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
