pub mod event;
#[cfg(target_arch = "wasm32")]
pub mod page;
#[cfg(not(target_arch = "wasm32"))]
pub mod prune;
pub mod query;
#[cfg(not(target_arch = "wasm32"))]
pub mod spawn;
#[cfg(not(target_arch = "wasm32"))]
pub mod transition;

#[cfg(not(target_arch = "wasm32"))]
pub use vmux_core::{CreatedAt, LastActivatedAt, Visit, now_millis};

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest =
    vmux_core::page::PageManifest { host: "history" };

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
