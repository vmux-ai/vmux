#![allow(clippy::too_many_arguments, clippy::type_complexity)]

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::TeamPlugin;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "team",
    title: "Team",
    keywords: &["team", "agents", "profile"],
    icon: Some(vmux_core::BuiltinIcon::Users),
    command_bar: true,
};
