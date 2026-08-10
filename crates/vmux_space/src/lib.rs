//! Spaces (workspace groupings): CRUD over named spaces, per-space startup URL and
//! directory, active-space tracking, and the spaces list webview.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod model;
/// The page resolves its keyboard through the keymap, which is a CEF seam: it publishes a
/// `PageKeyContext` and is answered over binary IPC. A touch host has neither, so this is `web`
/// rather than `ui` — the narrower gate is the one that is true.
#[cfg(web)]
pub mod page;

pub use vmux_wire::space as event;

#[cfg(host)]
mod native;
#[cfg(host)]
pub use native::*;
