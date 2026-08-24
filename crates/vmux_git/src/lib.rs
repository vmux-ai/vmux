pub mod event;
pub mod view;

pub const FILES_HOST: &str = "files";

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
