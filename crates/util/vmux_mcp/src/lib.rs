#![allow(clippy::type_complexity)]

pub use cli::McpCliPlugin;

pub(crate) struct Feature;

impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

mod cli;
pub mod host_quote;
pub mod protocol;
