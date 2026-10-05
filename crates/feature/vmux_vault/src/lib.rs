#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(host)]
pub use agent::VaultToolPlugin;
#[cfg(host)]
pub use host::VaultPlugin;

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(host)]
mod agent;
#[cfg(host)]
mod host;
mod state;
#[cfg(host)]
mod storage;
#[cfg(ui)]
mod ui;
