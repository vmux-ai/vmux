//! Terminal page: spawns and drives shell processes through the background service and
//! renders them in a CEF + Dioxus terminal webview.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
#[cfg(web)]
pub mod matrix_rain;
#[cfg(web)]
pub mod page;
pub mod render_model;

#[cfg(not(web))]
pub mod clipboard;
#[cfg(not(web))]
pub mod component;
#[cfg(not(web))]
pub mod launch;
#[cfg(not(web))]
mod link;
#[cfg(not(web))]
pub mod pid;

#[cfg(not(web))]
pub use component::{AgentRunTerminal, ProcessExited, PtyExited, RetainOnProcessExit, Terminal};
#[cfg(not(web))]
pub mod plugin;
#[cfg(not(web))]
pub mod processes_monitor;
#[cfg(not(web))]
pub mod shell_env;
#[cfg(not(web))]
pub mod shell_input;
#[cfg(not(web))]
pub mod snapshot_updater;
#[cfg(not(web))]
pub mod target;

#[cfg(not(web))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "terminal",
    title: "Terminal",
    keywords: &["shell", "console"],
    icon: Some(vmux_core::BuiltinIcon::Terminal),
    command_bar: true,
};

#[cfg(not(web))]
pub use plugin::*;
