#![allow(clippy::type_complexity)]

pub(crate) struct Feature;

impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

mod cli;
pub mod host_quote;
pub mod protocol;

pub use cli::McpCliPlugin;
