pub use vmux_wire::history as event;
#[cfg(ui)]
pub mod page;
pub mod ranking;

pub const PAGE_URL: &str = "vmux://history/";

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
