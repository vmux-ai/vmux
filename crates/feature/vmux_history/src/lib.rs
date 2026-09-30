pub use vmux_api::history as event;
pub mod ranking;
pub mod state;
#[cfg(ui)]
pub mod ui;

pub const PAGE_URL: &str = "vmux://history/";
#[cfg(host)]
pub use vmux_core::{CreatedAt, LastActivatedAt, Visit, now_millis};

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::{HistoryOpenIntent, HistoryPlugin};
