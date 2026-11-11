#[cfg(host)]
pub use cli::ContainerCliPlugin;

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[vmux_page::page]
pub struct ContainerPlugin;

pub mod event;

#[cfg(host)]
mod cli;
#[cfg(host)]
mod compose;
#[cfg(host)]
mod control;
#[cfg(host)]
mod host;
#[cfg(host)]
mod inventory;
#[cfg(ui)]
pub mod ui;
