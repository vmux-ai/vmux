#![allow(non_snake_case, clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(host)]
pub use host::StartPlugin;

#[cfg(host)]
use bevy::prelude::{Component, Entity};

pub(crate) struct Feature;

impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod event;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;

#[cfg(host)]
mod branches;

#[cfg(host)]
mod workspace;

pub mod roster;

#[cfg(host)]
#[derive(Component, Clone, Copy, Debug)]
pub struct StartInlineTransition {
    pub webview: Entity,
    pub navigation: Option<vmux_ecs::PageOpenId>,
}

#[cfg(host)]
#[derive(Component)]
pub struct StartInlineTransitionView;
