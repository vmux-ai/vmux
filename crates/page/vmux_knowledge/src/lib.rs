//! Local-first Markdown knowledge tree and agent context.

#[cfg(host)]
mod host;
#[cfg(host)]
pub mod store;
#[cfg(host)]
pub use host::*;
