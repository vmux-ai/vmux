pub fn frecency(visit_count: u32, last_visited_at: i64, now: i64) -> f32 {
    let age_hours = ((now - last_visited_at).max(0) as f32) / 3_600_000.0;
    let decay = 1.0 / (1.0 + age_hours / 24.0);
    (visit_count as f32) * decay
}

pub fn match_strength(query: &str, url: &str, title: &str) -> f32 {
    if query.is_empty() {
        return 1.0;
    }
    let q = query.to_lowercase();
    let u = url.to_lowercase();
    let t = title.to_lowercase();
    let mut score = 0.0;
    if u.starts_with(&q) {
        score += 3.0;
    }
    if t.starts_with(&q) {
        score += 2.0;
    }
    if u.contains(&q) && !u.starts_with(&q) {
        score += 1.0;
    }
    if t.contains(&q) && !t.starts_with(&q) {
        score += 1.0;
    }
    score
}

pub fn score(
    visit_count: u32,
    last_visited_at: i64,
    now: i64,
    query: &str,
    url: &str,
    title: &str,
) -> f32 {
    let m = match_strength(query, url, title);
    if m == 0.0 {
        return 0.0;
    }
    frecency(visit_count, last_visited_at, now) * m
}

#[cfg(not(target_arch = "wasm32"))]
use bevy::ecs::message::Messages;
#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use crate::event::{
    HISTORY_CHANGED_EVENT, HISTORY_QUERY_RESPONSE_EVENT, HISTORY_SUGGESTIONS_RESPONSE_EVENT,
    HistoryChangedEvent, HistoryClearAllRequest, HistoryDeleteRequest, HistoryEntry,
    HistoryOpenRequest, HistoryQueryRequest, HistoryQueryResponse, HistorySuggestionsRequest,
    HistorySuggestionsResponse,
};
#[cfg(not(target_arch = "wasm32"))]
use bevy_cef::prelude::{BinHostEmitEvent, BinReceive};
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::{CreatedAt, LastVisitedAt, PageMetadata, Url, Visit, VisitCount, VisitedUrl};

#[cfg(not(target_arch = "wasm32"))]
pub fn on_history_query_request(
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

#[cfg(not(target_arch = "wasm32"))]
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
                        favicon_url: meta.favicon_url.clone(),
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
                            favicon_url: meta.favicon_url.clone(),
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

#[cfg(not(target_arch = "wasm32"))]
pub fn on_history_delete_request(
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

#[cfg(not(target_arch = "wasm32"))]
pub fn on_history_clear_all_request(
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
#[cfg(not(target_arch = "wasm32"))]
pub struct HistoryOpenIntent {
    pub url: String,
    pub in_new_stack: bool,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_history_open_request(
    trigger: On<BinReceive<HistoryOpenRequest>>,
    mut messages: ResMut<Messages<HistoryOpenIntent>>,
) {
    let req = &trigger.event().payload;
    messages.write(HistoryOpenIntent {
        url: req.url.clone(),
        in_new_stack: req.in_new_stack,
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn broadcast_history_changed(
    changed: Query<(), (Changed<LastVisitedAt>, With<Url>)>,
    webviews: Query<(Entity, &bevy_cef::prelude::WebviewSource)>,
    browsers: NonSend<bevy_cef_core::prelude::Browsers>,
    mut commands: Commands,
) {
    if changed.iter().next().is_none() {
        return;
    }
    for (e, src) in &webviews {
        let bevy_cef::prelude::WebviewSource::Url(url) = src else {
            continue;
        };
        if !url.starts_with("vmux://history") {
            continue;
        }
        if !browsers.has_browser(e) || !browsers.host_emit_ready(&e) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            e,
            HISTORY_CHANGED_EVENT,
            &HistoryChangedEvent,
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_history_suggestions_request(
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
                    favicon_url: meta.favicon_url.clone(),
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
mod tests {
    use super::*;

    #[test]
    fn frecency_decays_with_age() {
        let now = 1_000_000_000;
        let recent = frecency(10, now - 3_600_000, now);
        let old = frecency(10, now - 100 * 3_600_000, now);
        assert!(recent > old);
    }

    #[test]
    fn match_strength_url_prefix_beats_substring() {
        let pfx = match_strength("git", "github.com", "GitHub");
        let mid = match_strength("hub", "github.com", "GitHub");
        assert!(pfx > mid);
    }

    #[test]
    fn match_strength_zero_on_miss() {
        assert_eq!(match_strength("xyz", "github.com", "GitHub"), 0.0);
    }

    #[test]
    fn match_strength_one_when_query_empty() {
        assert_eq!(match_strength("", "github.com", "GitHub"), 1.0);
    }

    #[test]
    fn higher_visit_count_ranks_higher_at_equal_match() {
        let now = 1_000_000_000;
        let a = score(20, now, now, "git", "github.com", "GitHub");
        let b = score(2, now, now, "git", "github.com", "GitHub");
        assert!(a > b);
    }
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
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CorePlugin);

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
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CorePlugin);

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
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CorePlugin);

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
