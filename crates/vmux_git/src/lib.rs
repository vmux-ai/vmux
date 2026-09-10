pub mod event;
pub mod view;

pub const FILES_HOST: &str = "files";
pub const GIT_PAGE_URL: &str = "vmux://git/";

#[cfg(ui)]
pub mod page;
#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
