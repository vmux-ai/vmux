//! The `vmux://start/` launcher page: page manifest, the [`event::StartDataRequest`]
//! data feed, the Dioxus page component, and [`StartPlugin`].

pub mod event;

/// Whether an agent page can replace the launcher inside its existing webview.
pub fn supports_inline_agent_transition(url: &str) -> bool {
    let Some(path) = url.strip_prefix("vmux://agent/") else {
        return false;
    };
    !path
        .split('/')
        .any(|segment| matches!(segment, "cli" | "setup"))
}

#[cfg(web)]
pub mod page;

#[cfg(host)]
mod plugin;
#[cfg(host)]
pub use plugin::StartPlugin;

#[cfg(host)]
#[derive(bevy::prelude::Component, Clone, Copy, Debug)]
pub struct StartInlineTransition {
    pub webview: bevy::prelude::Entity,
}

#[cfg(host)]
#[derive(bevy::prelude::Component)]
pub struct StartInlineTransitionView;

/// Canonical URL of the start launcher page.
#[cfg(host)]
pub const START_PAGE_URL: &str = "vmux://start/";

/// Page manifest for the `vmux://start/` launcher (also reachable from the Cmd+K command bar).
#[cfg(host)]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "start",
    title: "Start",
    keywords: &["start", "home", "new tab", "launcher"],
    icon: Some(vmux_core::icon::BuiltinIcon::Sparkles),
    command_bar: true,
};
