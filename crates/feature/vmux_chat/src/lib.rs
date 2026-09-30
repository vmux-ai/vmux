#![allow(non_snake_case)]

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_core::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod activity;
pub mod event;
pub mod host;
pub mod state;
pub mod tab;

pub mod selector;

#[cfg(host)]
pub use host::ChatPlugin;

#[cfg(ui)]
pub mod ui;
