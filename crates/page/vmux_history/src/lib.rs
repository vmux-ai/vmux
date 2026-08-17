//! Browsing history: records visits, prunes old entries, serves history queries and
//! command-bar suggestions, and renders the history webview.

pub use vmux_wire::history as event;
#[cfg(ui)]
pub mod page;
pub mod ranking;

/// The url this page answers, named once so the host and the view cannot disagree about it.
pub const PAGE_URL: &str = "vmux://history/";

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
