use std::path::PathBuf;

use bevy_cef::prelude::*;
use chrono::Duration;
use url::Url;
use vmux_webview_app::{UiReady, WebviewAppConfig, WebviewAppPlugin};

use crate::event::{HISTORY_EVENT, HistoryEvent};

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WebviewAppPlugin::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            WebviewAppConfig::with_custom_host("history"),
        ))
        .add_systems(Startup, spawn_sample_history_visits)
        .add_systems(Update, push_history_via_host_emit);
    }
}

#[allow(dead_code)]
#[derive(Component, Clone, Copy, Debug)]
struct Sent(DateTime<Utc>);

#[derive(Bundle)]
struct VisitBundle {
    visit: Visit,
    metadata: PageMetadata,
    created_at: CreatedAt,
}

#[derive(Component, Clone, Copy)]
struct Visit;

#[allow(dead_code)]
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
        commands.entity(wv).insert(Sent(Utc::now()));
    }
}

fn spawn_sample_history_visits(mut commands: Commands) {
    let now = Utc::now();
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
        commands.spawn(VisitBundle {
            visit: Visit,
            metadata: PageMetadata {
                url: Url::parse(href).unwrap(),
                title: (*title).to_owned(),
                favicon_url: favicon_url.map(String::from),
            },
            created_at: CreatedAt(now - Duration::minutes(i as i64)),
        });
    }
}
