pub mod webview;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod client;
#[cfg(not(target_arch = "wasm32"))]
pub mod framing;
#[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
pub mod launchd;
#[cfg(not(target_arch = "wasm32"))]
pub mod process;
#[cfg(not(target_arch = "wasm32"))]
pub mod protocol;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod service;
#[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
pub mod sm_app_service;
#[cfg(not(target_arch = "wasm32"))]
pub mod supervisor;

#[cfg(not(target_arch = "wasm32"))]
mod paths;
#[cfg(not(target_arch = "wasm32"))]
pub use paths::*;
