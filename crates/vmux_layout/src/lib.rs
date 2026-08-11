//! The window and layout shell: spaces, tabs, panes, stacks, focus ring, header and
//! side-sheet, command-bar input, and the single CEF layout webview that composes every page.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod command_bar;
pub mod event;
pub mod protocol;
pub mod reconcile;
pub mod start;

#[cfg(ui)]
pub mod debug_page;
#[cfg(ui)]
pub mod error_page;
#[cfg(ui)]
pub mod extensions_page;
#[cfg(ui)]
pub mod page;
#[cfg(ui)]
pub mod tools_page;
#[cfg(ui)]
pub mod vault_page;

#[cfg(host)]
pub mod host;
#[cfg(host)]
pub use host::*;
