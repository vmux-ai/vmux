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
use bevy_cef::prelude::{BinHostEmitEvent, BinReceive};
use vmux_core::{CreatedAt, LastVisitedAt, PageMetadata, Url, Visit, VisitCount, VisitedUrl};

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
pub struct HistoryOpenIntent {
    pub url: String,
    pub in_new_stack: bool,
}

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
#[path = "query.test.rs"]
mod handler_tests;
