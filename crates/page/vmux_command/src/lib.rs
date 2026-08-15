//! The `vmux://command-bar/` page, the command vocabulary behind it, and the wire protocol
//! between the two: the `AppCommand` type, the issue/read system-set ordering, and the snapshots
//! the launcher searches over.
//!
//! The page half renders the launcher; the host half decides what a chosen row means. What acts on
//! a choice lives in the crate that owns the capability, reached through
//! [`snapshot::Contributions`] — the page never learns what any row is for.

#![cfg_attr(web, allow(non_snake_case))]

#[cfg(web)]
pub mod page;
#[cfg(web)]
pub mod panel;

pub mod event;
pub mod size;
pub use vmux_wire::open_target;
pub use vmux_wire::prompt_media;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
