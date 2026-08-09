//! Agent integration: pluggable CLI agent strategies (vibe, claude, codex), session
//! lifecycle and discovery, and the ECS messaging that lets agents drive screenshots,
//! recordings, browser snapshots, and layout commands.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod agents_page;
pub mod chat_page;
pub mod vibe;

#[cfg(native)]
mod native;
#[cfg(native)]
pub use native::*;
