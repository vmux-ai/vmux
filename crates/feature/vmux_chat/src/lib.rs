#![allow(non_snake_case)]

pub use host::ChatPlugin;

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod activity;
pub mod event;
pub mod host;
pub mod state;
pub mod tab;

pub mod selector;

#[cfg(ui)]
pub mod ui;
