//! Background daemon that hosts persistent terminal processes across app restarts, plus
//! the processes-monitor webview page.

pub mod chat;
pub use vmux_wire::service as event;

#[cfg(ui)]
pub mod page;

pub mod message;
pub mod protocol;
pub mod remote;

#[cfg(host)]
mod native;
#[cfg(host)]
pub use native::*;
