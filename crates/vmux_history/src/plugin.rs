use std::path::PathBuf;

use bevy_cef::prelude::*;
use url::Url;
use vmux_webview_app::{UiReady, WebviewAppConfig, WebviewAppRegistry};

use crate::event::{HISTORY_EVENT, HistoryEvent};

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .resource_mut::<WebviewAppRegistry>()
            .register(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                &WebviewAppConfig::with_custom_host("history"),
            );
        app.add_systems(Startup, spawn_sample_history_visits)
            .add_systems(Update, push_history_via_host_emit);
    }
}

#[derive(Component, Clone, Copy, Debug)]
struct Sent(i64);

#[derive(Component, Clone, Debug)]
struct PageMetadata {
    url: Url,
    title: String,
    favicon_url: Option<String>,
}

fn push_history_via_host_emit(
    mut commands: Commands,
    browsers: NonSend<Browsers>,
    ready: Query<Entity, (With<WebviewSource>, With<UiReady>, Without<Sent>)>,
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
            .map(|(meta, _)| meta.url.as_str().to_owned())
            .collect();
        let url = history.join(", ");
        let payload = HistoryEvent { url, history };
        let ron_body = ron::ser::to_string(&payload).unwrap_or_default();
        commands.trigger(HostEmitEvent::new(wv, HISTORY_EVENT, &ron_body));
        commands.entity(wv).insert(Sent(now_millis()));
    }
}

fn spawn_sample_history_visits(mut commands: Commands) {
    let now = now_millis();
    let samples = [
        (
            "https://example.com/",
            "Example",
            Some("https://example.com/favicon.ico"),
        ),
        ("https://bevyengine.org/", "Bevy", None),
        ("https://rust-lang.org/", "Rust", None),
    ];
    for (i, (href, title, favicon_url)) in samples.iter().enumerate() {
        commands.spawn((
            Visit,
            PageMetadata {
                url: Url::parse(href).unwrap(),
                title: (*title).to_owned(),
                favicon_url: favicon_url.map(String::from),
            },
            CreatedAt(now - (i as i64 * 60_000)),
        ));
    }
}
