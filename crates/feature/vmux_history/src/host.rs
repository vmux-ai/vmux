use bevy::prelude::*;
pub use query::HistoryOpenIntent;
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::page::HostedPage;
use vmux_ecs::persistence::PersistenceAppExt;
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
            .register_persisted::<Visit>()
            .register_persisted::<Url>()
            .register_persisted::<VisitCount>()
            .register_persisted::<LastVisitedAt>()
            .register_persisted::<VisitedUrl>()
            .register_persisted::<TransitionType>();
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
