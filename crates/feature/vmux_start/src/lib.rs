#![allow(non_snake_case, clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(host)]
pub use host::StartPlugin;

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod event;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;

pub mod roster;

#[cfg(host)]
#[derive(bevy::prelude::Component, Clone, Copy, Debug)]
pub struct StartInlineTransition {
    pub webview: bevy::prelude::Entity,
}

#[cfg(host)]
#[derive(bevy::prelude::Component)]
pub struct StartInlineTransitionView;
