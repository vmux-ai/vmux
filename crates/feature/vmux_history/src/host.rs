pub mod prune;
pub mod query;
pub mod spawn;
mod state;
pub mod transition;

use bevy::prelude::*;
use vmux_ecs::host::manifest::FeaturePlugin;
use vmux_ecs::host::page::NativelyHosted;

pub use query::HistoryOpenIntent;

#[vmux_native::page]
pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        #[cfg(ui)]
        app.add_plugins(crate::ui::HistoryPage::plugin());
        app.add_plugins(
            Self::MANIFEST
                .plugin()
                .hosted(NativelyHosted::page(Self::URL, "History")),
        )
        .add_plugins((
            spawn::HistorySpawnPlugin,
            state::StatePlugin,
            query::HistoryQueryPlugin,
            prune::HistoryPrunePlugin,
        ));
    }
}
