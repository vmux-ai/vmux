//! The `vmux://command-bar/` page, the command vocabulary behind it, and the wire protocol
//! between the two: the `AppCommand` type, the issue/read system-set ordering, and the snapshots
//! the launcher searches over.
//!
//! The page half renders the launcher; the host half decides what a chosen row means. What acts on
//! a choice lives in the crate that owns the capability, reached through
//! [`snapshot::Contributions`] — the page never learns what any row is for.

#![cfg_attr(ui, allow(non_snake_case))]

#[cfg(ui)]
pub mod page;
#[cfg(ui)]
pub mod panel;

// `web`, not `ui`: the modal exists to make a webview behave like one, so it measures its own
// shell and talks to the host about native window size. The palette it wraps is portable; this is
// not, and the surface a user actually opens is `panel::CommandBarPanel` instead.
#[cfg(web)]
pub mod modal;

pub mod event;
pub mod size;
pub use vmux_wire::open_target;
pub use vmux_wire::prompt_media;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
