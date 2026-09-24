use crate::ranking::score;
use bevy::ecs::message::Messages;
use bevy::prelude::*;

use crate::event::{
    HistoryClearAllRequest, HistoryDeleteRequest, HistoryEntry, HistoryOpenRequest,
    HistoryQueryRequest, HistoryQueryResponse, HistorySuggestionsRequest,
    HistorySuggestionsResponse,
};
use bevy_cef::prelude::{BinReceive, UiEventPlugin};
use vmux_core::{CreatedAt, LastVisitedAt, PageMetadata, Url, Visit, VisitCount, VisitedUrl};

use super::state::{HistoryQueryState, HistoryUiStateUpdates};

pub struct HistoryQueryPlugin;

impl Plugin for HistoryQueryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiEventPlugin::<(
                HistoryQueryRequest,
                HistoryDeleteRequest,
                HistoryClearAllRequest,
                HistoryOpenRequest,
            )>::default(),
            UiEventPlugin::<(HistorySuggestionsRequest,)>::default(),
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
    mut pages: Query<&mut HistoryQueryState>,
    mut commands: Commands,
) {
    let req = &trigger.event().payload;
    let Ok(mut state) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    *state = HistoryQueryState::from_request(req);
    let response = history_query_response(req, &urls, &visits);
    HistoryUiStateUpdates::write(&mut commands, trigger.event().webview, &response);
}

fn history_query_response(
    request: &HistoryQueryRequest,
    urls: &Query<(Entity, &PageMetadata, &VisitCount, &LastVisitedAt), With<Url>>,
    visits: &Query<(&CreatedAt, &VisitedUrl), With<Visit>>,
) -> HistoryQueryResponse {
    let url_rows: Vec<_> = urls
        .iter()
        .map(|(entity, metadata, count, last)| (entity, metadata.clone(), *count, *last))
        .collect();
    let visit_rows: Vec<_> = visits
        .iter()
        .map(|(created, visited)| (*created, *visited))
        .collect();
    let entries = build_entries(
        &request.query,
        &url_rows,
        &visit_rows,
        vmux_core::now_millis(),
    );
    let offset = request.offset as usize;
    let limit = request.limit as usize;
    let total = entries.len();
    let entries: Vec<_> = entries.into_iter().skip(offset).take(limit).collect();
    let has_more = offset + entries.len() < total;
    HistoryQueryResponse {
        request_id: request.request_id,
        offset: request.offset,
        entries,
        has_more,
    }
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

fn broadcast_history_changed(
    changed: Query<(), (Changed<LastVisitedAt>, With<Url>)>,
    pages: Query<(Entity, &HistoryQueryState)>,
    urls: Query<(Entity, &PageMetadata, &VisitCount, &LastVisitedAt), With<Url>>,
    visits: Query<(&CreatedAt, &VisitedUrl), With<Visit>>,
    mut commands: Commands,
) {
    if changed.iter().next().is_none() {
        return;
    }
    for (entity, state) in &pages {
        if state.request_id == 0 {
            continue;
        }
        let request = HistoryQueryRequest {
            query: state.query.clone(),
            offset: 0,
            limit: state.limit,
            request_id: state.request_id,
        };
        let response = history_query_response(&request, &urls, &visits);
        HistoryUiStateUpdates::write(&mut commands, entity, &response);
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

    vmux_core::host::UiState::<vmux_api::command_bar::CommandBarUiState>::write(
        &mut commands,
        trigger.event().webview,
        &HistorySuggestionsResponse {
            request_id: req.request_id,
            entries,
        },
    );
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
