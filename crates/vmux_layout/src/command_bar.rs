#[cfg(web)]
pub mod page;
#[cfg(web)]
pub mod palette;

pub use vmux_start::{keyboard, results, style};

pub mod size;

#[cfg(not(web))]
pub mod handler;
#[cfg(not(web))]
pub mod key;
#[cfg(not(web))]
pub mod panel;
#[cfg(not(web))]
pub mod state;
#[cfg(not(web))]
pub mod work_snapshot;
