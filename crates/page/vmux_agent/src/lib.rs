//! Agent integration: pluggable CLI agent strategies (vibe, claude, codex), session
//! lifecycle and discovery, and the ECS messaging that lets agents drive screenshots,
//! recordings, browser snapshots, and layout commands.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod event;

#[cfg(ui)]
pub mod page;
pub mod vibe;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
