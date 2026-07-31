use std::time::Duration;

use bevy::time::common_conditions::on_timer;
use bevy_cef::prelude::BinEventEmitterPlugin;
use vmux_core::page::PrewarmPage;

use crate::event::{
    HistoryChangedEvent, HistoryClearAllRequest, HistoryDeleteRequest, HistoryOpenRequest,
    HistoryQueryRequest, HistorySuggestionsRequest,
};
use crate::prune::prune_history;
use crate::query::{
    HistoryOpenIntent, broadcast_history_changed, on_history_clear_all_request,
    on_history_delete_request, on_history_open_request, on_history_query_request,
    on_history_suggestions_request,
};
use crate::spawn::{record_requested_visits, spawn_visits};

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
        app.add_systems(
            Update,
            (spawn_visits, record_requested_visits, broadcast_history_changed).chain(),
        )
            .add_systems(
                Update,
                prune_history.run_if(on_timer(Duration::from_secs(3600))),
            )
            .add_systems(Startup, prune_history)
            .add_plugins((
                BinEventEmitterPlugin::<(
                    HistoryQueryRequest,
                    HistoryDeleteRequest,
                    HistoryClearAllRequest,
                    HistoryOpenRequest,
                    HistoryChangedEvent,
                )>::for_hosts(&["history"]),
                BinEventEmitterPlugin::<(HistorySuggestionsRequest,)>::for_hosts(&["command-bar", "start"]),
            ))
            .add_observer(on_history_query_request)
            .add_observer(on_history_delete_request)
            .add_observer(on_history_clear_all_request)
            .add_observer(on_history_open_request)
            .add_observer(on_history_suggestions_request)
            .add_message::<HistoryOpenIntent>()
            .add_message::<vmux_core::event::RecordVisitRequest>();
    }
}
