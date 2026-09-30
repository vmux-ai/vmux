#![allow(non_snake_case)]

pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");
#[cfg(host)]
pub(crate) type Feature = host::ChatToolPlugin;

pub mod activity;
pub mod event;
pub mod host;
pub mod tab;

pub mod selector;

#[cfg(host)]
pub use host::ChatPlugin;

#[cfg(ui)]
pub mod ui;
