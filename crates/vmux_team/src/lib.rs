//! Team and profiles view: represents the user and the agents in the active space as
//! team members and renders the team webview.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(frontend)]
pub mod page;

#[cfg(native)]
mod native;
#[cfg(native)]
pub use native::*;
