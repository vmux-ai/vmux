pub use crate::remote::authorization::{
    AuthorizationOutcome, AuthorizedDevice, RelayToken, RemoteAuthorizationStore,
};
pub use crate::remote::pairing;
pub use vmux_transport::DeviceId;

pub mod acp;
pub mod agent;
pub mod agent_broker;
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
mod query;
pub mod registry;
pub mod run_marker;
pub mod runner;
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
    title_message_id: Some("services-title"),
    replaces_command: Some("service_open"),
    keywords: &["processes", "monitor"],
    icon: Some(vmux_core::BuiltinIcon::Activity),
    command_bar: true,
};

mod daemon;
mod paths;
pub use daemon::*;
pub use paths::*;
