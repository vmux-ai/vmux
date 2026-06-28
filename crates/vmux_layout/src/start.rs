//! The `vmux://start/` launcher page: page manifest, the [`event::StartDataRequest`]
//! data feed, the Dioxus page component, and [`StartPlugin`].

pub mod event;

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::StartPlugin;

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
