use vmux_core::page::PrewarmPage;

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
