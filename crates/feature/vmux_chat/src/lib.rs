#![allow(non_snake_case)]

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod activity;
pub mod event;
mod group;
pub mod host;
mod projection;
pub mod state;
pub mod tab;

pub mod selector;

pub use group::{group_turns_before, group_turns_tail, grouped_item_count};
pub use host::ChatPlugin;

#[cfg(ui)]
pub mod ui;
