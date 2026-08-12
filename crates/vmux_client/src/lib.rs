//! Locating, starting, and talking to the vmux background service.
//!
//! Split out from `vmux_service` for weight rather than for layering: this crate pulls about a
//! hundred dependencies where the daemon pulls closer to eight hundred, so `vmux` and the MCP
//! server can reach the service without linking Bevy, CEF, quinn or the agent protocol.
//! `vmux_service` re-exports every module here, so code on the daemon side sees one namespace.

pub mod bundle;
pub mod cli;
pub mod client;
pub mod daemon;
pub mod framing;
#[cfg(target_os = "macos")]
pub mod launchd;
pub mod pairing;
pub mod paths;

pub mod protocol {
    pub use vmux_wire::protocol::*;
}

pub use daemon::*;
pub use paths::*;
