pub mod cwd;
mod key;
pub mod plugin;
pub mod project;
pub mod snapshot_updater;
pub mod spaces;

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "spaces",
    title: "Spaces",
    title_message_id: Some("spaces-title"),
    replaces_command: None,
    keywords: &["space"],
    icon: Some(vmux_core::BuiltinIcon::Layers),
    command_bar: true,
};

pub use plugin::{SaveSpaceRequest, SpaceCommandRequest, SpacePlugin};
pub use spaces::{ActiveSpace, Spaces};
