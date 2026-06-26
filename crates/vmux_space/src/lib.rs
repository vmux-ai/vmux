#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod model;
#[cfg(target_arch = "wasm32")]
pub mod page;

pub use vmux_core::event::space as event;

#[cfg(not(target_arch = "wasm32"))]
pub mod cwd;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod snapshot_updater;
#[cfg(not(target_arch = "wasm32"))]
pub mod spaces;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "spaces",
    title: "Spaces",
    keywords: &["space"],
    icon: Some(vmux_core::BuiltinIcon::Layers),
    command_bar: true,
};

#[cfg(not(target_arch = "wasm32"))]
pub use plugin::{SaveSpaceRequest, SpaceCommandRequest, SpacePlugin};
#[cfg(not(target_arch = "wasm32"))]
pub use spaces::{ActiveSpace, Spaces};
