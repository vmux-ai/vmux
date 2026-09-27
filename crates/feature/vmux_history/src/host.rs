pub mod prune;
pub mod query;
pub mod spawn;
mod state;
pub mod transition;

use bevy::prelude::*;
use vmux_core::host::page::NativelyHosted;

pub use vmux_core::{CreatedAt, LastActivatedAt, Visit, now_millis};

#[vmux_native::page]
pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins(crate::ui::HistoryPage::plugin());
        app.add_plugins(
            Self::MANIFEST
                .plugin()
                .hosted(NativelyHosted::page(crate::PAGE_URL, "History")),
        )
        .add_plugins((
            crate::spawn::HistorySpawnPlugin,
            crate::host::state::StatePlugin,
            crate::query::HistoryQueryPlugin,
            crate::prune::HistoryPrunePlugin,
        ));
    }
}
