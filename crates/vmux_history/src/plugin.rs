use std::path::PathBuf;

use bevy_cef::prelude::*;
use vmux_page::{PageReady, PageConfig, PageRegistry};

use crate::event::{HISTORY_EVENT, HistoryEvent};
use vmux_core::PageMetadata;

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<PageRegistry>()
            .register(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                &PageConfig::with_custom_host("history"),
            );
        app.add_systems(Update, push_history_via_host_emit);
    }
}

#[derive(Component, Clone, Copy, Debug)]
struct Sent(#[allow(dead_code)] i64);

#[allow(clippy::type_complexity)]
fn push_history_via_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    ready: Query<Entity, (With<WebviewSource>, With<PageReady>, Without<Sent>)>,
    history_q: Query<(&PageMetadata, &CreatedAt), With<Visit>>,
) {
    for wv in ready.iter() {
        if !browsers.has_browser(wv) || !browsers.host_emit_ready(&wv) {
            continue;
        }
        let mut rows: Vec<(&PageMetadata, &CreatedAt)> = history_q.iter().collect();
        rows.sort_by_key(|(_, created)| std::cmp::Reverse(created.0));
        let history: Vec<String> = rows
            .into_iter()
            .map(|(meta, _)| meta.url.clone())
            .collect();
        let url = history.join(", ");
        let payload = HistoryEvent { url, history };
        commands.trigger(BinHostEmitEvent::from_rkyv(wv, HISTORY_EVENT, &payload));
        commands.entity(wv).insert(Sent(now_millis()));
    }
}
