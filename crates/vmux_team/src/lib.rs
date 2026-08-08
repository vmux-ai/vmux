//! Team and profiles view: represents the user and the agents in the active space as
//! team members and renders the team webview.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(frontend)]
pub mod page;

#[cfg(native)]
pub mod plugin;
#[cfg(native)]
pub use plugin::TeamPlugin;

#[cfg(native)]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "team",
    title: "Team",
    keywords: &["team", "agents", "profile"],
    icon: Some(vmux_core::BuiltinIcon::Users),
    command_bar: true,
};
