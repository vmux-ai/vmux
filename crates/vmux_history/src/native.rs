//! Recording and querying visits, which only the desktop does.

pub mod prune;
pub mod query;
pub mod spawn;
pub mod transition;
use bevy::prelude::*;
pub use vmux_core::{CreatedAt, LastActivatedAt, Visit, now_millis};

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "history",
    title: "History",
    keywords: &["recent", "visited"],
    icon: Some(vmux_core::BuiltinIcon::Clock),
    command_bar: true,
};

include!("plugin.rs");
