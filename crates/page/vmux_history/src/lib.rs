pub use vmux_api::history as event;
pub mod ranking;
#[cfg(ui)]
pub mod ui;

pub const PAGE_URL: &str = "vmux://history/";

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
