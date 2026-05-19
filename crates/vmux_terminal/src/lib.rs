pub mod event;
pub mod render_model;

#[cfg(not(target_arch = "wasm32"))]
pub mod launch;

#[cfg(not(target_arch = "wasm32"))]
pub use launch::{TerminalKind, TerminalLaunch};

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
