pub mod event;
pub mod render_model;

#[cfg(not(target_arch = "wasm32"))]
pub mod component;
#[cfg(not(target_arch = "wasm32"))]
pub mod launch;
#[cfg(not(target_arch = "wasm32"))]
pub mod pid;

#[cfg(not(target_arch = "wasm32"))]
pub use component::{ProcessExited, PtyExited, Terminal};
#[cfg(not(target_arch = "wasm32"))]
pub use launch::{TerminalKind, TerminalLaunch};

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
