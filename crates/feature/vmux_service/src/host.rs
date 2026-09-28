pub use crate::remote::authorization::{
    AuthorizationOutcome, AuthorizedDevice, RelayToken, RemoteAuthorizationStore,
};
pub use crate::remote::pairing;
pub use vmux_transport::DeviceId;

pub mod acp;
pub mod bundle;
pub mod cleanup;
pub mod cli;
pub use cli::ServiceCliPlugin;
pub mod client;
mod launch_agent;
#[cfg(target_os = "macos")]
pub mod launchd;
mod osc133;
pub mod plugin;
pub mod process;
pub mod query;
pub mod registry;
pub mod run_marker;
pub mod runner;
pub mod server;
mod shell_integration;
#[cfg(target_os = "macos")]
pub mod sm_app_service;
pub mod supervisor;

mod daemon;
pub use daemon::*;
pub use launch_agent::LaunchAgent;
pub use vmux_core::service::{RemotePaths, ServicePaths};
