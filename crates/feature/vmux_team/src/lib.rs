#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(host)]
pub use host::{AgentRenameProfile, ProfileSwitchRequested, TeamPlugin};
#[cfg(host)]
pub use tool::TeamToolPlugin;

pub(crate) struct Feature;

impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;

#[cfg(host)]
mod cloud;

mod projection;
pub mod roster;
#[cfg(host)]
mod tool;
