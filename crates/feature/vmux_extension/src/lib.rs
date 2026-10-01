#![allow(clippy::type_complexity)]

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(host)]
mod catalog;
#[cfg(host)]
pub mod crx;
#[cfg(host)]
mod download;
#[cfg(host)]
mod host;
#[cfg(host)]
pub mod install;
#[cfg(host)]
pub mod manifest;
#[cfg(host)]
pub mod match_pattern;
#[cfg(host)]
pub mod protocol;
#[cfg(host)]
pub mod store;
#[cfg(ui)]
mod ui;
#[cfg(host)]
pub mod webstore;

#[cfg(host)]
pub use catalog::{ExtensionInstallCompleted, ExtensionInstallRequest, OpenManagerRequest};
#[cfg(host)]
pub use host::ExtensionPlugin;
