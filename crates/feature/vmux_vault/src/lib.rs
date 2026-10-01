#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(host)]
mod agent;
#[cfg(host)]
mod host;
mod state;
#[cfg(ui)]
mod ui;

#[cfg(host)]
pub use agent::VaultToolPlugin;
#[cfg(host)]
pub use host::VaultPlugin;
