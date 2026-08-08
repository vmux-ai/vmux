//! Browsing history: records visits, prunes old entries, serves history queries and
//! command-bar suggestions, and renders the history webview.

pub use vmux_wire::history as event;
#[cfg(frontend)]
pub mod page;
#[cfg(native)]
pub mod prune;
#[cfg(native)]
pub mod query;
pub mod ranking;
#[cfg(native)]
pub mod spawn;
#[cfg(native)]
pub mod transition;

#[cfg(native)]
pub use vmux_core::{CreatedAt, LastActivatedAt, Visit, now_millis};

#[cfg(native)]
use bevy::prelude::*;

#[cfg(native)]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "history",
    title: "History",
    keywords: &["recent", "visited"],
    icon: Some(vmux_core::BuiltinIcon::Clock),
    command_bar: true,
};

#[cfg(native)]
include!("plugin.rs");
