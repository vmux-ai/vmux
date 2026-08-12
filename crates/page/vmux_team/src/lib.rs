//! Team and profiles view: represents the user and the agents in the active space as
//! team members and renders the team webview.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(ui)]
pub mod page;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
