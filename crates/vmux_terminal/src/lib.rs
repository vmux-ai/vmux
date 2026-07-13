//! Terminal page: spawns and drives shell processes through the background service and
//! renders them in a CEF + Dioxus terminal webview.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
#[cfg(target_arch = "wasm32")]
pub mod matrix_rain;
#[cfg(target_arch = "wasm32")]
pub mod page;
pub mod render_model;

#[cfg(not(target_arch = "wasm32"))]
pub mod clipboard;
#[cfg(not(target_arch = "wasm32"))]
pub mod component;
#[cfg(not(target_arch = "wasm32"))]
pub mod launch;
#[cfg(not(target_arch = "wasm32"))]
mod link;
#[cfg(not(target_arch = "wasm32"))]
pub mod pid;

#[cfg(not(target_arch = "wasm32"))]
pub use component::{AgentRunTerminal, ProcessExited, PtyExited, RetainOnProcessExit, Terminal};
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod processes_monitor;
#[cfg(not(target_arch = "wasm32"))]
pub mod shell_env;
#[cfg(not(target_arch = "wasm32"))]
pub mod shell_input;
#[cfg(not(target_arch = "wasm32"))]
pub mod snapshot_updater;
#[cfg(not(target_arch = "wasm32"))]
pub mod target;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "terminal",
    title: "Terminal",
    keywords: &["shell", "console"],
    icon: Some(vmux_core::BuiltinIcon::Terminal),
    command_bar: true,
};

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::*;
