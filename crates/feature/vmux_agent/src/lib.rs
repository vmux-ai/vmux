#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod setup;

#[cfg(all(host, feature = "provider"))]
pub mod http;
#[cfg(all(host, feature = "provider"))]
pub mod providers;
#[cfg(all(host, feature = "provider"))]
pub mod stream;

#[cfg(all(host, feature = "app"))]
mod cli;
#[cfg(all(host, feature = "app"))]
pub mod host;
#[cfg(all(host, feature = "app"))]
pub use cli::AgentCliPlugin;
#[cfg(all(host, feature = "app"))]
pub use host::*;
