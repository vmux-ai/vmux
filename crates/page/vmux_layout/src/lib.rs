//! The window and layout shell: spaces, tabs, panes, stacks, focus ring, header and
//! side-sheet, command-bar input, and the single CEF layout webview that composes every page.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
pub mod protocol;
pub mod reconcile;
pub mod start;

#[cfg(ui)]
pub mod debug_page;
#[cfg(ui)]
pub mod tools_page;

#[cfg(web)]
pub mod page;

// `web`, not `ui`: each of these still reaches the DOM directly and is only ever served into the
// CEF webview. They were written when `ui` and `web` were the same thing, which stopped being
// true when iOS arrived — saying `web` records what they are instead of implying a phone can
// render them.
#[cfg(web)]
pub mod error_page;
#[cfg(web)]
pub mod extensions_page;
#[cfg(web)]
pub mod vault_page;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
