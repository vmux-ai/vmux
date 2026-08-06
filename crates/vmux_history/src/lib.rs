//! Browsing history: records visits, prunes old entries, serves history queries and
//! command-bar suggestions, and renders the history webview.

pub use vmux_wire::history as event;
#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
pub mod page;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod prune;
pub mod query;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod spawn;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub mod transition;

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub use vmux_core::{CreatedAt, LastActivatedAt, Visit, now_millis};

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
use bevy::prelude::*;

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "history",
    title: "History",
    keywords: &["recent", "visited"],
    icon: Some(vmux_core::BuiltinIcon::Clock),
    command_bar: true,
};

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
include!("plugin.rs");
