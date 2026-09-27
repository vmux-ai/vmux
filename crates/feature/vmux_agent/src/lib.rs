#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod setup;

#[cfg(host)]
mod cli;
#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use cli::AgentCliPlugin;
#[cfg(host)]
pub use host::*;
