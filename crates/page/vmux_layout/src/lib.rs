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

#[cfg(ui)]
pub mod tools_page;

#[cfg(ui)]
pub mod page;

#[cfg(ui)]
pub mod error_page;
#[cfg(ui)]
pub mod extensions_page;

// `web`, not `ui`: this still reaches the DOM directly and is only ever served into the CEF
// webview. It was written when `ui` and `web` were the same thing, which stopped being true when
// iOS arrived — saying `web` records what it is instead of implying a phone can render it.
#[cfg(web)]
pub mod vault_page;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
