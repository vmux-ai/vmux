#![allow(non_snake_case, clippy::too_many_arguments, clippy::type_complexity)]

pub mod event;

#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::StartPlugin;

pub mod roster;

pub use vmux_api::agent::supports_inline_agent_transition;

#[cfg(host)]
#[derive(bevy::prelude::Component, Clone, Copy, Debug)]
pub struct StartInlineTransition {
    pub webview: bevy::prelude::Entity,
}

#[cfg(host)]
#[derive(bevy::prelude::Component)]
pub struct StartInlineTransitionView;

pub const START_PAGE_URL: &str = "vmux://start/";
