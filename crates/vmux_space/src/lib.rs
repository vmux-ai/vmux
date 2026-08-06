//! Spaces (workspace groupings): CRUD over named spaces, per-space startup URL and
//! directory, active-space tracking, and the spaces list webview.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod model;
#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
pub mod page;

pub use vmux_wire::space as event;

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod cwd;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod plugin;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod snapshot_updater;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod spaces;

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "spaces",
    title: "Spaces",
    keywords: &["space"],
    icon: Some(vmux_core::BuiltinIcon::Layers),
    command_bar: true,
};

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use plugin::{SaveSpaceRequest, SpaceCommandRequest, SpacePlugin};
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use spaces::{ActiveSpace, Spaces};
