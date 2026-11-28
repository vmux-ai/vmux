#[cfg(host)]
pub use cli::ContainerCliPlugin;

pub(crate) struct Feature;

impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[vmux_page::page(page = "vm")]
pub struct ContainerPlugin;

#[cfg(host)]
#[vmux_page::page(page = "docker")]
pub(crate) struct Docker;

#[cfg(host)]
#[vmux_page::page(page = "containers")]
pub(crate) struct Containers;

pub mod event;

#[cfg(host)]
mod cli;
#[cfg(host)]
mod control;
#[cfg(host)]
mod host;
#[cfg(host)]
mod inventory;
#[cfg(host)]
mod inventory_io;
#[cfg(host)]
mod logs_io;
#[cfg(ui)]
pub mod ui;
