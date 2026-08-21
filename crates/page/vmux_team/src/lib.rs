//! Team and profiles view: represents the user and the agents in the active space as
//! team members and renders the team webview.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(ui)]
pub mod page;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;

/// The roster where there is no desktop underneath, only a relay.
///
/// Not gated to iOS, though only the phone adds it: plain ECS over wire types, so leaving it
/// unconditional keeps a projection nobody can exercise locally inside the test suite.
pub mod roster;
