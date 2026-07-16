//! Lightweight access to the vmux background service.

pub mod bundle;
pub mod cli;
pub mod client;
pub mod framing;
#[cfg(target_os = "macos")]
pub mod launchd;
pub mod paths;

pub mod protocol {
    pub use vmux_wire::protocol::*;
}

pub use paths::*;
