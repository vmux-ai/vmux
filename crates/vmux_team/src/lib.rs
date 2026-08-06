//! Team and profiles view: represents the user and the agents in the active space as
//! team members and renders the team webview.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
pub mod page;

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod plugin;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use plugin::TeamPlugin;

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "team",
    title: "Team",
    keywords: &["team", "agents", "profile"],
    icon: Some(vmux_core::BuiltinIcon::Users),
    command_bar: true,
};
