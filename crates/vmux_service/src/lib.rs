//! Background daemon that hosts persistent terminal processes across app restarts, plus
//! the processes-monitor webview page.

#[cfg(native)]
pub use vmux_service_client::{
    pairing, read_message, read_message_blocking, write_message, write_message_blocking,
};

pub mod chat;
pub use vmux_wire::service as event;

#[cfg(frontend)]
pub mod page;

#[cfg(native)]
pub mod acp;
#[cfg(native)]
pub mod agent;
#[cfg(native)]
pub mod agent_broker;
#[cfg(native)]
pub mod agent_events;
#[cfg(native)]
pub mod bundle;
#[cfg(native)]
pub mod cleanup;
#[cfg(native)]
pub mod cli;
#[cfg(native)]
pub mod client;
#[cfg(native)]
pub mod framing;
#[cfg(native)]
pub mod http;
#[cfg(all(target_os = "macos", native))]
pub mod launchd;
pub mod message;
#[cfg(native)]
mod osc133;
#[cfg(native)]
pub mod plugin;
#[cfg(native)]
pub mod process;
pub mod protocol;
#[cfg(native)]
pub mod providers;
#[cfg(native)]
pub mod registry;
pub mod remote;
#[cfg(native)]
pub mod run_marker;
#[cfg(native)]
pub mod server;
#[cfg(native)]
pub mod service;
#[cfg(native)]
mod shell_integration;
#[cfg(all(target_os = "macos", native))]
pub mod sm_app_service;
#[cfg(native)]
pub mod stream;
#[cfg(native)]
pub mod supervisor;

#[cfg(native)]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "services",
    title: "Services",
    keywords: &["processes", "monitor"],
    icon: Some(vmux_core::BuiltinIcon::Activity),
    command_bar: true,
};

#[cfg(native)]
mod paths;
#[cfg(native)]
pub use paths::*;
