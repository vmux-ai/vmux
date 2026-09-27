#![allow(non_snake_case)]

pub mod activity;
pub mod composer;
pub mod event;
#[cfg(host)]
pub mod host;
#[cfg(host)]
mod key;
#[cfg(host)]
pub mod media;
pub mod state;
pub mod tab;
pub mod transcript;

pub mod model;
pub mod prompt;
pub mod room;
pub mod selector;

#[cfg(host)]
pub use key::ChatKeyPlugin;
#[cfg(host)]
pub use media::ChatMediaPlugin;

#[cfg(any(test, ui))]
pub mod format;

#[cfg(ui)]
pub mod ui;
