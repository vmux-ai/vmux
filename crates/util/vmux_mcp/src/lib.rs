#![allow(clippy::type_complexity)]

pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");
pub(crate) type Feature = McpCliPlugin;

mod cli;
pub mod host_quote;
pub mod protocol;

pub use cli::McpCliPlugin;
