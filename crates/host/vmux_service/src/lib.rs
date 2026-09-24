pub mod chat;
pub use vmux_api::service as event;

#[cfg(ui)]
pub mod ui;

pub mod message;
pub mod protocol;
pub mod remote;

pub const PAGE_URL: &str = "vmux://services/";

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
