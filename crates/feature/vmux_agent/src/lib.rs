#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(all(host, feature = "app"))]
pub use host::AgentPlugin;

#[cfg(all(host, feature = "app"))]
pub(crate) struct Feature;

#[cfg(all(host, feature = "app"))]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(all(host, feature = "service"))]
pub mod acp;
#[cfg(all(host, feature = "service"))]
pub mod broker;

#[cfg(all(host, feature = "app"))]
mod host;
#[cfg(all(host, feature = "app"))]
mod managed_mcp;
#[cfg(all(host, feature = "app"))]
mod mcp;
#[cfg(all(host, feature = "app"))]
mod policy;
#[cfg(host)]
pub(crate) mod route;
