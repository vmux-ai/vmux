//! Space storage and the plugin that drives it, on the desktop side.

pub mod cwd;
pub mod plugin;
pub mod snapshot_updater;
pub mod spaces;

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "spaces",
    title: "Spaces",
    keywords: &["space"],
    icon: Some(vmux_core::BuiltinIcon::Layers),
    command_bar: true,
};

pub use plugin::{SaveSpaceRequest, SpaceCommandRequest, SpacePlugin};
pub use spaces::{ActiveSpace, Spaces};
