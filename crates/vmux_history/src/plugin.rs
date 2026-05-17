use std::path::PathBuf;
use std::time::Duration;

use bevy::time::common_conditions::on_timer;
use bevy_cef::prelude::BinJsEmitEventPlugin;
use vmux_webview_app::{WebviewAppConfig, WebviewAppRegistry};

use crate::event::{
    HistoryClearAllRequest, HistoryDeleteRequest, HistoryOpenRequest, HistoryQueryRequest,
    HistorySuggestionsRequest,
};
use crate::prune::prune_history;
use crate::query::{
    HistoryOpenIntent, on_history_clear_all_request, on_history_delete_request,
    on_history_open_request, on_history_query_request, on_history_suggestions_request,
};
use crate::spawn::spawn_visits;

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<WebviewAppRegistry>()
            .register(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                &WebviewAppConfig::with_custom_host("history"),
            );
        app.add_systems(Update, spawn_visits);
        app.add_systems(
            Update,
            prune_history.run_if(on_timer(Duration::from_secs(3600))),
        );
        app.add_systems(Startup, prune_history);

        app.add_plugins(BinJsEmitEventPlugin::<HistoryQueryRequest>::default());
        app.add_plugins(BinJsEmitEventPlugin::<HistoryDeleteRequest>::default());
        app.add_plugins(BinJsEmitEventPlugin::<HistoryClearAllRequest>::default());
        app.add_plugins(BinJsEmitEventPlugin::<HistoryOpenRequest>::default());
        app.add_plugins(BinJsEmitEventPlugin::<HistorySuggestionsRequest>::default());

        app.add_observer(on_history_query_request);
        app.add_observer(on_history_delete_request);
        app.add_observer(on_history_clear_all_request);
        app.add_observer(on_history_open_request);
        app.add_observer(on_history_suggestions_request);

        app.add_message::<HistoryOpenIntent>();
    }
}
