#[cfg(web)]
pub mod page;
#[cfg(web)]
pub mod palette;

pub use vmux_start::{keyboard, results, style};

pub mod size;

#[cfg(host)]
pub mod handler;
#[cfg(host)]
pub mod key;
#[cfg(host)]
pub mod panel;
#[cfg(host)]
pub mod state;
#[cfg(host)]
pub mod work_snapshot;
