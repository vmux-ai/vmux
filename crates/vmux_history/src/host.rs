//! Recording and querying visits, which only the desktop does.

pub mod prune;
pub mod query;
pub mod spawn;
pub mod transition;

use bevy::prelude::*;
use vmux_core::page::PrewarmPage;

pub use vmux_core::{CreatedAt, LastActivatedAt, Visit, now_millis};

/// Wires the history domain: visit spawning, change broadcasts, timed pruning, and
/// history query, open, and suggestion bridges.
pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn((
            crate::PAGE_MANIFEST,
            PrewarmPage {
                host: "history",
                url: "vmux://history/",
                title: "History",
                pool_size: 1,
            },
        ));
        vmux_core::register_host_spawn(app, "history");
        app.add_plugins((
            crate::spawn::HistorySpawnPlugin,
            crate::query::HistoryQueryPlugin,
            crate::prune::HistoryPrunePlugin,
        ));
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "history",
    title: "History",
    keywords: &["recent", "visited"],
    icon: Some(vmux_core::BuiltinIcon::Clock),
    command_bar: true,
};
