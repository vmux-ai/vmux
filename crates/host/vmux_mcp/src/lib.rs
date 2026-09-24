#![allow(clippy::type_complexity)]

#[cfg(feature = "connection")]
mod connection;
pub mod host_quote;
pub mod protocol;
pub mod tool;

#[cfg(feature = "connection")]
pub use connection::McpConnectionPlugin;
