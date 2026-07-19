//! The `vmux://start/` launcher page: page manifest, the [`event::StartDataRequest`]
//! data feed, the Dioxus page component, and [`StartPlugin`].

pub mod event;

/// Whether an agent page can replace the launcher inside its existing webview.
pub fn supports_inline_agent_transition(url: &str) -> bool {
    url.starts_with("vmux://agent/") && !url.contains("/cli") && !url.contains("/setup")
}

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::StartPlugin;

#[cfg(not(target_arch = "wasm32"))]
#[derive(bevy::prelude::Component, Clone, Copy, Debug)]
pub struct StartAgentTransition {
    pub webview: bevy::prelude::Entity,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(bevy::prelude::Component)]
pub struct StartAgentTransitionView;

/// Canonical URL of the start launcher page.
#[cfg(not(target_arch = "wasm32"))]
pub const START_PAGE_URL: &str = "vmux://start/";

/// Page manifest for the `vmux://start/` launcher (also reachable from the Cmd+K command bar).
#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "start",
    title: "Start",
    keywords: &["start", "home", "new tab", "launcher"],
    icon: Some(vmux_core::icon::BuiltinIcon::Sparkles),
    command_bar: true,
};
