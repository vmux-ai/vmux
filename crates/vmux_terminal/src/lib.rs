#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
pub mod render_model;

#[cfg(not(target_arch = "wasm32"))]
pub mod clipboard;
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
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod processes_monitor;
#[cfg(not(target_arch = "wasm32"))]
pub mod shell_input;
#[cfg(not(target_arch = "wasm32"))]
pub mod target;

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::*;
