//! The chat transcript UI, rendered identically by every vmux client.
//!
//! Hosts differ in how they fetch a conversation — the desktop receives snapshots over CEF
//! binary IPC, mobile streams them over SSE — but both hand the same [`vmux_wire::chat`] model
//! to the same components, so a turn looks the same everywhere.

#![allow(non_snake_case)]

pub mod activity;
