#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub use menu::BookmarkPlugin;
pub use tool::BookmarkToolPlugin;

pub(crate) struct Feature;

impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

mod menu;
mod persistence;
mod tool;
