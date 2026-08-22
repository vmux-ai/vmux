//! The shared launcher.
//!
//! Ranking and rendering live here so the desktop launcher and the mobile launcher are the
//! same surface; only the result providers and the action sink differ per host.

#![allow(non_snake_case, clippy::too_many_arguments, clippy::type_complexity)]

pub mod event;

#[cfg(ui)]
pub mod page;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::StartPlugin;

/// The launcher's model where there is no desktop underneath, only a relay.
///
/// Not gated to iOS, though only the phone adds it: it is plain ECS over wire types, so leaving it
/// unconditional is what keeps the one projection nobody can exercise locally inside the reach of
/// the test suite.
pub mod roster;

pub use vmux_wire::agent::supports_inline_agent_transition;

/// The view a launcher is turning into an agent page.
///
/// Choosing an agent swaps the launcher for it in place rather than opening a pane beside it, so
/// the stack has to remember which view is mid-transition.
#[cfg(host)]
#[derive(bevy::prelude::Component, Clone, Copy, Debug)]
pub struct StartInlineTransition {
    pub webview: bevy::prelude::Entity,
}

#[cfg(host)]
#[derive(bevy::prelude::Component)]
pub struct StartInlineTransitionView;

/// Canonical URL of the start launcher page.
pub const START_PAGE_URL: &str = "vmux://start/";

/// Page manifest for the launcher, which the command bar also offers.
#[cfg(host)]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "start",
    title: "Start",
    title_message_id: Some("start-title"),
    replaces_command: None,
    keywords: &["start", "home", "new tab", "launcher"],
    icon: Some(vmux_core::icon::BuiltinIcon::Sparkles),
    command_bar: true,
};
