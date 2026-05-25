use std::path::PathBuf;
use std::time::Duration;

use bevy::time::common_conditions::on_timer;
use bevy_cef::prelude::BinEventEmitterPlugin;
use vmux_core::{
    CefPageAttachRequest, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask,
};
use vmux_server::{PageConfig, Server};

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
use crate::spawn::spawn_visits;

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().resource_mut::<Server>().register(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            &PageConfig::with_custom_host("history"),
        );
        app.add_systems(Update, (spawn_visits, broadcast_history_changed).chain());
        app.add_systems(Update, handle_history_page_open.in_set(PageOpenSet::HandleKnownPages));
        app.add_systems(
            Update,
            prune_history.run_if(on_timer(Duration::from_secs(3600))),
        );
        app.add_systems(Startup, prune_history);

        app.add_plugins(
            BinEventEmitterPlugin::<(
                HistoryQueryRequest,
                HistoryDeleteRequest,
                HistoryClearAllRequest,
                HistoryOpenRequest,
                HistorySuggestionsRequest,
                HistoryChangedEvent,
            )>::default(),
        );

        app.add_observer(on_history_query_request);
        app.add_observer(on_history_delete_request);
        app.add_observer(on_history_clear_all_request);
        app.add_observer(on_history_open_request);
        app.add_observer(on_history_suggestions_request);

        app.add_message::<HistoryOpenIntent>();
        app.add_message::<CefPageAttachRequest>();
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handle_history_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    mut attach_writer: MessageWriter<CefPageAttachRequest>,
    mut commands: Commands,
) {
    for (entity, task) in &tasks {
        if task.url != "vmux://history/" {
            continue;
        }
        attach_writer.write(CefPageAttachRequest {
            stack: task.stack,
            url: task.url.clone(),
            title: "History".to_string(),
            bg_color: None,
        });
        commands.entity(entity).insert(PageOpenHandled);
    }
}
