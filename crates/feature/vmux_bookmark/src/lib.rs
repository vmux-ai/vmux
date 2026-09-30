#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub(crate) struct Feature;

impl vmux_core::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

mod menu;
mod persistence;
mod tool;

pub use menu::BookmarkPlugin;
pub use tool::BookmarkToolPlugin;
