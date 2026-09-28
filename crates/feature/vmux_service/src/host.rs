pub use crate::remote::authorization::{
    AuthorizationOutcome, AuthorizedDevice, RelayToken, RemoteAuthorizationStore,
};
pub use crate::remote::pairing;
pub use vmux_transport::DeviceId;

pub mod bundle;
pub mod cleanup;
pub mod cli;
pub use cli::ServiceCliPlugin;
pub mod client;
mod launch_agent;
#[cfg(target_os = "macos")]
pub mod launchd;
pub mod plugin;
pub mod query;
pub mod registry;
pub mod runner;
pub mod server;
#[cfg(target_os = "macos")]
pub mod sm_app_service;
pub mod supervisor;

mod daemon;
pub use daemon::*;
pub use launch_agent::LaunchAgent;
pub use vmux_core::service::{RemotePaths, ServicePaths};
