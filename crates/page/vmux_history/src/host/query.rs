//! Serving history queries to the webview.
//!
//! Gated as a whole rather than item by item. The ranking above it is pure arithmetic and
//! compiles everywhere.

use crate::ranking::score;
use bevy::ecs::message::Messages;
use bevy::prelude::*;

use crate::event::{
    HISTORY_CHANGED_EVENT, HISTORY_QUERY_RESPONSE_EVENT, HISTORY_SUGGESTIONS_RESPONSE_EVENT,
    HistoryChangedEvent, HistoryClearAllRequest, HistoryDeleteRequest, HistoryEntry,
    HistoryOpenRequest, HistoryQueryRequest, HistoryQueryResponse, HistorySuggestionsRequest,
    HistorySuggestionsResponse,
};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};
use vmux_core::{CreatedAt, LastVisitedAt, PageMetadata, Url, Visit, VisitCount, VisitedUrl};

/// Answers the history page's queries, deletions and opens, and pushes the command bar its
/// suggestions.
pub struct HistoryQueryPlugin;

impl Plugin for HistoryQueryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            BinEventEmitterPlugin::<(
                HistoryQueryRequest,
                HistoryDeleteRequest,
                HistoryClearAllRequest,
                HistoryOpenRequest,
                HistoryChangedEvent,
            )>::for_hosts(&["history"]),
            BinEventEmitterPlugin::<(HistorySuggestionsRequest,)>::for_hosts(&[
                "command-bar",
                "start",
                "layout",
            ]),
        ))
        .add_message::<HistoryOpenIntent>()
        .add_observer(on_history_query_request)
        .add_observer(on_history_delete_request)
        .add_observer(on_history_clear_all_request)
        .add_observer(on_history_open_request)
        .add_observer(on_history_suggestions_request)
        .add_systems(
            Update,
            broadcast_history_changed.after(crate::spawn::record_requested_visits),
        );
    }
}

fn on_history_query_request(
    trigger: On<BinReceive<HistoryQueryRequest>>,
    urls: Query<(Entity, &PageMetadata, &VisitCount, &LastVisitedAt), With<Url>>,
    visits: Query<(&CreatedAt, &VisitedUrl), With<Visit>>,
    mut commands: Commands,
) {
    let req = &trigger.event().payload;
    let now = vmux_core::now_millis();

    let url_rows: Vec<_> = urls
        .iter()
        .map(|(e, m, c, l)| (e, m.clone(), *c, *l))
        .collect();
    let visit_rows: Vec<_> = visits.iter().map(|(c, vu)| (*c, *vu)).collect();

    let entries = build_entries(&req.query, &url_rows, &visit_rows, now);
    let total = entries.len();
    let offset = req.offset as usize;
    let limit = req.limit as usize;
    let page: Vec<_> = entries.into_iter().skip(offset).take(limit).collect();
    let returned = page.len();
    let has_more = offset + returned < total;

    let payload = HistoryQueryResponse {
        request_id: req.request_id,
        entries: page,
        has_more,
    };
    commands.trigger(BinHostEmitEvent::from_rkyv(
        trigger.event().webview,
        HISTORY_QUERY_RESPONSE_EVENT,
        &payload,
    ));
}

pub fn build_entries(
    query: &Option<String>,
    urls: &[(Entity, PageMetadata, VisitCount, LastVisitedAt)],
    visits: &[(CreatedAt, VisitedUrl)],
    now: i64,
) -> Vec<HistoryEntry> {
    match query {
        None => {
            let mut entries: Vec<HistoryEntry> = visits
                .iter()
                .filter_map(|(created, visited_url)| {
                    let (e, meta, count, last) =
                        urls.iter().find(|(e, _, _, _)| *e == visited_url.0)?;
                    Some(HistoryEntry {
                        url_entity_bits: e.to_bits(),
                        url: meta.url.clone(),
                        title: meta.title.clone(),
                        favicon_url: meta.icon.favicon_url().to_string(),
                        visit_created_at: created.0,
                        visit_count: count.0,
                        last_visited_at: last.0,
                    })
                })
                .collect();
            entries.sort_by_key(|e| std::cmp::Reverse(e.visit_created_at));
            entries
        }
        Some(q) => {
            let mut scored: Vec<(f32, HistoryEntry)> = urls
                .iter()
                .filter_map(|(e, meta, count, last)| {
                    let s = score(count.0, last.0, now, q, &meta.url, &meta.title);
                    if s <= 0.0 {
                        return None;
                    }
                    Some((
                        s,
                        HistoryEntry {
                            url_entity_bits: e.to_bits(),
                            url: meta.url.clone(),
                            title: meta.title.clone(),
                            favicon_url: meta.icon.favicon_url().to_string(),
                            visit_created_at: last.0,
                            visit_count: count.0,
                            last_visited_at: last.0,
                        },
                    ))
                })
                .collect();
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            scored.into_iter().map(|(_, e)| e).collect()
        }
    }
}

fn on_history_delete_request(
    trigger: On<BinReceive<HistoryDeleteRequest>>,
    mut commands: Commands,
    visits: Query<(Entity, &VisitedUrl), With<Visit>>,
) {
    let target = Entity::from_bits(trigger.event().payload.url_entity_bits);
    for (visit_e, visited_url) in visits.iter() {
        if visited_url.0 == target {
            commands.entity(visit_e).despawn();
        }
    }
    if commands.get_entity(target).is_ok() {
        commands.entity(target).despawn();
    }
}

fn on_history_clear_all_request(
    _trigger: On<BinReceive<HistoryClearAllRequest>>,
    mut commands: Commands,
    urls: Query<Entity, With<Url>>,
    visits: Query<Entity, With<Visit>>,
) {
    for e in urls.iter() {
        commands.entity(e).despawn();
    }
    for e in visits.iter() {
        commands.entity(e).despawn();
    }
}

#[derive(Clone, Debug, Message)]
pub struct HistoryOpenIntent {
    pub url: String,
    pub in_new_stack: bool,
}

fn on_history_open_request(
    trigger: On<BinReceive<HistoryOpenRequest>>,
    mut messages: ResMut<Messages<HistoryOpenIntent>>,
) {
    let req = &trigger.event().payload;
    messages.write(HistoryOpenIntent {
        url: req.url.clone(),
        in_new_stack: req.in_new_stack,
    });
}

/// Tell every open history page that what it is showing has changed.
///
/// Found by `PageMetadata` rather than `WebviewSource`: the page runs in this process now and has
/// no source, so the old query matched nothing and the page rendered once and never again.
/// `can_emit_to` for the same reason — a page with no CEF browser can still be emitted to.
fn broadcast_history_changed(
    changed: Query<(), (Changed<LastVisitedAt>, With<Url>)>,
    pages: Query<(Entity, &vmux_core::PageMetadata)>,
    browsers: NonSend<bevy_cef_core::prelude::Browsers>,
    mut commands: Commands,
) {
    if changed.iter().next().is_none() {
        return;
    }
    for (e, page) in &pages {
        if !page.url.starts_with("vmux://history") {
            continue;
        }
        if !browsers.can_emit_to(&e) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            e,
            HISTORY_CHANGED_EVENT,
            &HistoryChangedEvent,
        ));
    }
}

fn on_history_suggestions_request(
    trigger: On<BinReceive<HistorySuggestionsRequest>>,
    urls: Query<(Entity, &PageMetadata, &VisitCount, &LastVisitedAt), With<Url>>,
    mut commands: Commands,
) {
    let req = &trigger.event().payload;
    let now = vmux_core::now_millis();

    let mut scored: Vec<(f32, HistoryEntry)> = urls
        .iter()
        .filter_map(|(e, meta, count, last)| {
            let s = score(count.0, last.0, now, &req.query, &meta.url, &meta.title);
            if s <= 0.0 {
                return None;
            }
            Some((
                s,
                HistoryEntry {
                    url_entity_bits: e.to_bits(),
                    url: meta.url.clone(),
                    title: meta.title.clone(),
                    favicon_url: meta.icon.favicon_url().to_string(),
                    visit_created_at: last.0,
                    visit_count: count.0,
                    last_visited_at: last.0,
                },
            ))
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let entries: Vec<HistoryEntry> = scored
        .into_iter()
        .take(req.limit as usize)
        .map(|(_, e)| e)
        .collect();

    commands.trigger(BinHostEmitEvent::from_rkyv(
        trigger.event().webview,
        HISTORY_SUGGESTIONS_RESPONSE_EVENT,
        &HistorySuggestionsResponse {
            request_id: req.request_id,
            entries,
        },
    ));
}

#[cfg(test)]
mod handler_tests {
    use super::*;
    use vmux_core::{
        CorePlugin, CreatedAt, LastVisitedAt, PageMetadata, Url, VisitCount, VisitedUrl,
    };

    #[test]
    fn build_entries_no_query_orders_by_visit_created_at_desc() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(CorePlugin);

        let url_e = app
            .world_mut()
            .spawn((
                Url,
                PageMetadata {
                    url: "https://example.com".into(),
                    ..default()
                },
                VisitCount(2),
                LastVisitedAt(200),
                CreatedAt(0),
            ))
            .id();

        let url_rows = vec![(
            url_e,
            PageMetadata {
                url: "https://example.com".into(),
                ..default()
            },
            VisitCount(2),
            LastVisitedAt(200),
        )];
        let visit_rows = vec![
            (CreatedAt(100), VisitedUrl(url_e)),
            (CreatedAt(200), VisitedUrl(url_e)),
        ];

        let entries = build_entries(&None, &url_rows, &visit_rows, 1000);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].visit_created_at, 200);
        assert_eq!(entries[1].visit_created_at, 100);
    }

    #[test]
    fn build_entries_with_query_filters_and_ranks() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(CorePlugin);

        let e1 = app
            .world_mut()
            .spawn((
                Url,
                PageMetadata {
                    url: "https://github.com".into(),
                    title: "GitHub".into(),
                    ..default()
                },
                VisitCount(10),
                LastVisitedAt(1000),
                CreatedAt(0),
            ))
            .id();

        let e2 = app
            .world_mut()
            .spawn((
                Url,
                PageMetadata {
                    url: "https://example.com".into(),
                    title: "Example".into(),
                    ..default()
                },
                VisitCount(10),
                LastVisitedAt(1000),
                CreatedAt(0),
            ))
            .id();

        let url_rows = vec![
            (
                e1,
                PageMetadata {
                    url: "https://github.com".into(),
                    title: "GitHub".into(),
                    ..default()
                },
                VisitCount(10),
                LastVisitedAt(1000),
            ),
            (
                e2,
                PageMetadata {
                    url: "https://example.com".into(),
                    title: "Example".into(),
                    ..default()
                },
                VisitCount(10),
                LastVisitedAt(1000),
            ),
        ];
        let visit_rows = vec![];

        let entries = build_entries(&Some("git".into()), &url_rows, &visit_rows, 1000);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].url, "https://github.com");
    }

    #[test]
    fn build_entries_pagination() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(CorePlugin);

        let url_e = app
            .world_mut()
            .spawn((
                Url,
                PageMetadata {
                    url: "u".into(),
                    ..default()
                },
                VisitCount(1),
                LastVisitedAt(0),
                CreatedAt(0),
            ))
            .id();

        let url_rows = vec![(
            url_e,
            PageMetadata {
                url: "u".into(),
                ..default()
            },
            VisitCount(1),
            LastVisitedAt(0),
        )];
        let visit_rows: Vec<_> = (0..5)
            .map(|i| (CreatedAt(i * 100), VisitedUrl(url_e)))
            .collect();

        let all = build_entries(&None, &url_rows, &visit_rows, 1000);
        assert_eq!(all.len(), 5);

        let page: Vec<_> = all.into_iter().skip(2).take(2).collect();
        assert_eq!(page.len(), 2);
    }
}
