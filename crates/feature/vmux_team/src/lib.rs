#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(host)]
pub use host::{ProfileSwitchRequested, TeamPlugin};
#[cfg(host)]
pub use tool::TeamToolPlugin;

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;

mod projection;
pub mod roster;
#[cfg(host)]
mod tool;
