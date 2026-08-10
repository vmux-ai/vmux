//! Browsing history: records visits, prunes old entries, serves history queries and
//! command-bar suggestions, and renders the history webview.

pub use vmux_wire::history as event;
#[cfg(ui)]
pub mod page;
pub mod ranking;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
