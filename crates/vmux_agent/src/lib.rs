//! Agent integration: pluggable CLI agent strategies (vibe, claude, codex), session
//! lifecycle and discovery, and the ECS messaging that lets agents drive screenshots,
//! recordings, browser snapshots, and layout commands.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod event;

#[cfg(frontend)]
pub mod ui;
pub mod vibe;

#[cfg(native)]
pub mod plugin;
#[cfg(native)]
pub use plugin::*;
