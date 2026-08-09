//! The chat transcript UI, rendered identically by every vmux client.
//!
//! Hosts differ in how they fetch a conversation — the desktop receives snapshots over CEF
//! binary IPC, mobile streams them over SSE — but both hand the same [`vmux_wire::chat`] model
//! to the same components, so a turn looks the same everywhere.
//!
//! The page splits by build target. [`event`] is the vocabulary both halves speak and is
//! ungated; [`format`] shapes that vocabulary for a reader and needs no webview; [`ui`] renders.
//! The half that drives an agent is not here — it owns ECS state in `vmux_agent` and reaches the
//! page through [`event`] alone, which is what lets a conversation belong to a room rather than
//! to one agent.

#![allow(non_snake_case)]

pub mod activity;
pub mod clipboard;
pub mod event;
pub mod platform;
pub mod transcript;

#[cfg(any(test, frontend))]
pub mod format;

#[cfg(frontend)]
pub mod ui;
