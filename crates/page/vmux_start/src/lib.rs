#![allow(non_snake_case, clippy::too_many_arguments, clippy::type_complexity)]

pub mod event;

#[cfg(ui)]
pub mod page;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::StartPlugin;

pub mod roster;

pub use vmux_wire::agent::supports_inline_agent_transition;

#[cfg(host)]
#[derive(bevy::prelude::Component, Clone, Copy, Debug)]
pub struct StartInlineTransition {
    pub webview: bevy::prelude::Entity,
}

#[cfg(host)]
#[derive(bevy::prelude::Component)]
pub struct StartInlineTransitionView;

pub const START_PAGE_URL: &str = "vmux://start/";

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
