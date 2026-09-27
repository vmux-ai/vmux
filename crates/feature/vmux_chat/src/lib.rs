#![allow(non_snake_case)]

pub mod activity;
pub mod composer;
pub mod event;
#[cfg(host)]
mod key;
pub mod state;
pub mod tab;
pub mod transcript;

pub mod model;
pub mod prompt;
pub mod room;
pub mod selector;

#[cfg(host)]
pub use key::ChatKeyPlugin;

#[cfg(any(test, ui))]
pub mod format;

#[cfg(ui)]
pub mod ui;
