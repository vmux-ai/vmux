use bevy::prelude::*;
pub use query::HistoryOpenIntent;
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::page::HostedPage;
use vmux_ecs::store::StoreAppExt;
use vmux_ecs::{LastVisitedAt, TransitionType, Url, Visit, VisitCount, VisitedUrl};

pub mod prune;
pub mod query;
pub mod spawn;
mod state;
pub mod transition;

#[vmux_page::page]
pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .register_stored::<Visit>()
            .register_stored::<Url>()
            .register_stored::<VisitCount>()
            .register_stored::<LastVisitedAt>()
            .register_stored::<VisitedUrl>()
            .register_stored::<TransitionType>();
        #[cfg(ui)]
        app.add_plugins(crate::ui::HistoryPage::plugin());
        app.add_plugins(
            Self::MANIFEST
                .plugin()
                .hosted(HostedPage::page(Self::URL, "History")),
        )
        .add_plugins((
            spawn::HistorySpawnPlugin,
            state::StatePlugin,
            query::HistoryQueryPlugin,
            prune::HistoryPrunePlugin,
        ));
    }
}
