#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub(crate) const FEATURE_MANIFEST: &str = include_str!("feature.ron");
#[cfg(host)]
pub(crate) type Feature = TeamToolPlugin;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::{ProfileSwitchRequested, TeamPlugin};

mod projection;
pub mod roster;
#[cfg(host)]
mod tool;
#[cfg(host)]
pub use tool::TeamToolPlugin;
