//! The daemon itself: process supervision, transports and the ACP bridge.

pub use vmux_client::{
    pairing, read_message, read_message_blocking, write_message, write_message_blocking,
};

pub mod acp;
pub mod agent;
pub mod agent_broker;
pub mod agent_events;
pub mod bundle;
pub mod cleanup;
pub mod cli;
pub mod client;
pub mod framing;
pub mod http;
#[cfg(target_os = "macos")]
pub mod launchd;
mod osc133;
pub mod plugin;
pub mod process;
pub mod providers;
pub mod registry;
pub mod run_marker;
pub mod server;
pub mod service;
mod shell_integration;
#[cfg(target_os = "macos")]
pub mod sm_app_service;
pub mod stream;
pub mod supervisor;

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "services",
    title: "Services",
    keywords: &["processes", "monitor"],
    icon: Some(vmux_core::BuiltinIcon::Activity),
    command_bar: true,
};

mod daemon;
mod paths;
pub use daemon::*;
pub use paths::*;
