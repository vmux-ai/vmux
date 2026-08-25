pub mod prune;
pub mod query;
pub mod spawn;
pub mod transition;

use bevy::prelude::*;
use vmux_core::host::page::NativelyHosted;

pub use vmux_core::{CreatedAt, LastActivatedAt, Visit, now_millis};

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn((
            crate::PAGE_MANIFEST,
            NativelyHosted {
                url: crate::PAGE_URL,
                title: "History",
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
    title_message_id: Some("history-title"),
    replaces_command: Some("browser_open_history"),
    keywords: &["recent", "visited"],
    icon: Some(vmux_core::BuiltinIcon::Clock),
    command_bar: true,
};
