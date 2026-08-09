//! Everything the team view needs a real machine for.

pub mod plugin;
pub use plugin::TeamPlugin;

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "team",
    title: "Team",
    keywords: &["team", "agents", "profile"],
    icon: Some(vmux_core::BuiltinIcon::Users),
    command_bar: true,
};
