//! Spaces (workspace groupings): CRUD over named spaces, per-space startup URL and
//! directory, active-space tracking, and the spaces list webview.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod model;
#[cfg(frontend)]
pub mod page;

pub use vmux_wire::space as event;

#[cfg(native)]
pub mod cwd;
#[cfg(native)]
pub mod plugin;
#[cfg(native)]
pub mod snapshot_updater;
#[cfg(native)]
pub mod spaces;

#[cfg(native)]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "spaces",
    title: "Spaces",
    keywords: &["space"],
    icon: Some(vmux_core::BuiltinIcon::Layers),
    command_bar: true,
};

#[cfg(native)]
pub use plugin::{SaveSpaceRequest, SpaceCommandRequest, SpacePlugin};
#[cfg(native)]
pub use spaces::{ActiveSpace, Spaces};
