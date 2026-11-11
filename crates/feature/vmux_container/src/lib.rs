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
mod control;
#[cfg(host)]
mod host;
#[cfg(ui)]
pub mod ui;
