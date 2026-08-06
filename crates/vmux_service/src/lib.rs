//! Background daemon that hosts persistent terminal processes across app restarts, plus
//! the processes-monitor webview page.

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use vmux_service_client::{
    read_message, read_message_blocking, write_message, write_message_blocking,
};

pub mod chat;
pub use vmux_wire::service as event;

#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
pub mod page;

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod acp;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod agent;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod agent_broker;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod agent_events;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod bundle;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod cleanup;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod cli;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod client;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod framing;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod http;
#[cfg(all(
    target_os = "macos",
    not(any(target_arch = "wasm32", target_os = "ios"))
))]
pub mod launchd;
pub mod message;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
mod osc133;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod plugin;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod process;
pub mod protocol;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod providers;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod registry;
pub mod remote;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod run_marker;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod server;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod service;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
mod shell_integration;
#[cfg(all(
    target_os = "macos",
    not(any(target_arch = "wasm32", target_os = "ios"))
))]
pub mod sm_app_service;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod stream;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod supervisor;

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "services",
    title: "Services",
    keywords: &["processes", "monitor"],
    icon: Some(vmux_core::BuiltinIcon::Activity),
    command_bar: true,
};

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
mod paths;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use paths::*;
