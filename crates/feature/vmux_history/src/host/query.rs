use bevy::prelude::*;

use crate::event::{
    HistoryClearAllRequest, HistoryClearConfirmRequest, HistoryDeleteRequest, HistoryEntry,
    HistoryLoadMoreRequest, HistoryOpenRequest, HistoryQueryRequest, HistorySuggestionsRequest,
    HistorySuggestionsResponse,
};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use bevy_ecs::query::Changed as ComponentChanged;
use bevy_ecs::system::{Commands as WorldCommands, SystemParam};
use vmux_ecs::{CreatedAt, LastVisitedAt, PageMetadata, Url, Visit, VisitCount, VisitedUrl};

use super::state::HistoryPageState;

pub struct HistoryQueryPlugin;

impl Plugin for HistoryQueryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiEventPlugin::<(
                HistoryQueryRequest,
                HistoryLoadMoreRequest,
                HistoryDeleteRequest,
                HistoryClearAllRequest,
                HistoryClearConfirmRequest,
                HistoryOpenRequest,
            )>::default(),
            UiEventPlugin::<(HistorySuggestionsRequest,)>::default(),
        ))
        .add_message::<HistoryOpenIntent>()
        .add_observer(request)
        .add_observer(load_more)
        .add_observer(delete_request)
        .add_observer(clear_confirm)
        .add_observer(clear_all_request)
        .add_observer(open_request)
        .add_observer(suggestions_request)
        .add_systems(
            Update,
            broadcast_changed.after(super::spawn::HistoryWriteSet),
        )
        .add_systems(PostUpdate, publish_pages);
    }
}

fn request(trigger: On<UiInput<HistoryQueryRequest>>, mut pages: Query<&mut HistoryPageState>) {
    let Ok(mut state) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    state.search(&trigger.event().payload.query);
}

fn load_more(
    trigger: On<UiInput<HistoryLoadMoreRequest>>,
    mut pages: Query<&mut HistoryPageState>,
) {
    let Ok(mut state) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    state.load_more();
}

fn clear_confirm(
    trigger: On<UiInput<HistoryClearConfirmRequest>>,
    mut pages: Query<&mut HistoryPageState>,
) {
    let Ok(mut state) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    state.set_clear_confirm_open(trigger.event().payload.open);
}

#[derive(SystemParam)]
struct HistoryQuery<'w, 's> {
    urls: Query<
        'w,
        's,
        (
            Entity,
            &'static PageMetadata,
            &'static VisitCount,
            &'static LastVisitedAt,
        ),
        With<Url>,
    >,
    visits: Query<'w, 's, (Entity, &'static CreatedAt, &'static VisitedUrl), With<Visit>>,
}

impl HistoryQuery<'_, '_> {
    fn state(&self, query: Option<&str>, limit: usize, now: i64) -> crate::state::HistoryUiState {
        let mut ordered = Vec::new();
        if let Some(query) = query {
            let mut scored = Vec::new();
            for (entity, ..) in &self.urls {
                let score = self.score(entity, query, now);
                if score > 0.0 {
                    scored.push((score, entity));
                }
            }
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            for (_, entity) in scored {
                let Ok((_, _, _, last)) = self.urls.get(entity) else {
                    continue;
                };
                ordered.push((entity, last.0));
            }
        } else {
            for (_, created, visited) in &self.visits {
                if self.urls.contains(visited.0) {
                    ordered.push((visited.0, created.0));
                }
            }
            ordered.sort_by_key(|(_, created)| std::cmp::Reverse(*created));
        }
        let has_more = ordered.len() > limit;
        ordered.truncate(limit);
        let mut entries = Vec::with_capacity(ordered.len());
        for (entity, created) in ordered {
            let Ok((_, metadata, count, last)) = self.urls.get(entity) else {
                continue;
            };
            entries.push(HistoryEntry {
                url_entity_bits: entity.to_bits(),
                url: metadata.url.clone(),
                title: metadata.title.clone(),
                favicon_url: metadata.icon.favicon_url().to_string(),
                visit_created_at: created,
                visit_count: count.0,
                last_visited_at: last.0,
            });
        }
        crate::state::HistoryUiState {
            entries,
            has_more,
            query: query.unwrap_or_default().to_string(),
            clear_confirm_open: false,
        }
    }

    fn score(&self, entity: Entity, query: &str, now: i64) -> f32 {
        let Ok((_, metadata, count, last)) = self.urls.get(entity) else {
            return 0.0;
        };
        let strength = Self::match_strength(query, &metadata.url, &metadata.title);
        if strength == 0.0 {
            return 0.0;
        }
        let age_hours = ((now - last.0).max(0) as f32) / 3_600_000.0;
        let decay = 1.0 / (1.0 + age_hours / 24.0);
        count.0 as f32 * decay * strength
    }

    fn match_strength(query: &str, url: &str, title: &str) -> f32 {
        if query.is_empty() {
            return 1.0;
        }
        let query = query.to_lowercase();
        let url = url.to_lowercase();
        let title = title.to_lowercase();
        let mut score = 0.0;
        if url.starts_with(&query) {
            score += 3.0;
        } else if url.contains(&query) {
            score += 1.0;
        }
        if title.starts_with(&query) {
            score += 2.0;
        } else if title.contains(&query) {
            score += 1.0;
        }
        score
    }
}

fn delete_request(
    trigger: On<UiInput<HistoryDeleteRequest>>,
    history: HistoryQuery,
    mut commands: WorldCommands,
    mut pages: Query<&mut HistoryPageState>,
) {
    if pages.get(trigger.event().webview).is_err() {
        return;
    }
    let mut target = None;
    for (entity, ..) in &history.urls {
        if entity.to_bits() == trigger.event().payload.url_entity_bits {
            target = Some(entity);
            break;
        }
    }
    let Some(target) = target else {
        return;
    };
    for (visit, _, url) in &history.visits {
        if url.0 == target {
            commands.entity(visit).despawn();
        }
    }
    commands.entity(target).despawn();
    for mut page in &mut pages {
        page.set_changed();
    }
}

fn clear_all_request(
    trigger: On<UiInput<HistoryClearAllRequest>>,
    mut commands: WorldCommands,
    urls: Query<Entity, With<Url>>,
    visits: Query<Entity, With<Visit>>,
    mut pages: Query<(Entity, &mut HistoryPageState)>,
) {
    for e in urls.iter() {
        commands.entity(e).despawn();
    }
    for e in visits.iter() {
        commands.entity(e).despawn();
    }
    for (entity, mut page) in &mut pages {
        if entity == trigger.event().webview {
            page.clear_confirm_open = false;
        }
        page.set_changed();
    }
}

#[derive(Clone, Debug, Message)]
pub struct HistoryOpenIntent {
    pub url: String,
    pub in_new_stack: bool,
}

fn open_request(
    trigger: On<UiInput<HistoryOpenRequest>>,
    mut messages: MessageWriter<HistoryOpenIntent>,
) {
    let req = &trigger.event().payload;
    messages.write(HistoryOpenIntent {
        url: req.url.clone(),
        in_new_stack: req.in_new_stack,
    });
}

fn broadcast_changed(
    changed: Query<(), (Changed<LastVisitedAt>, With<Url>)>,
    mut pages: Query<&mut HistoryPageState>,
) {
    if changed.iter().next().is_none() {
        return;
    }
    for mut page in &mut pages {
        page.set_changed();
    }
}

fn publish_pages(
    mut pages: Query<(Entity, &mut HistoryPageState), ComponentChanged<HistoryPageState>>,
    history: HistoryQuery,
    mut commands: WorldCommands,
) {
    let now = vmux_ecs::UnixMillis::now().0;
    for (entity, mut state) in &mut pages {
        let mut snapshot = history.state(state.query.as_deref(), state.limit as usize, now);
        snapshot.clear_confirm_open = state.clear_confirm_open;
        state.bypass_change_detection().has_more = snapshot.has_more;
        commands.trigger(
            vmux_ecs::UiStateWrite::<crate::state::HistoryUiState>::from_event(entity, &snapshot),
        );
    }
}

fn suggestions_request(
    trigger: On<UiInput<HistorySuggestionsRequest>>,
    history: HistoryQuery,
    mut commands: WorldCommands,
) {
    let req = &trigger.event().payload;
    let entries = history
        .state(
            Some(&req.query),
            req.limit as usize,
            vmux_ecs::UnixMillis::now().0,
        )
        .entries;
    commands.trigger(vmux_ecs::UiStateWrite::<
        vmux_api::command_bar::CommandBarUiState,
    >::from_event(
        trigger.event().webview,
        &HistorySuggestionsResponse {
            request_id: req.request_id,
            entries,
        },
    ));
}

#[cfg(test)]
mod handler_tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Published(Vec<crate::state::HistoryUiState>);

    struct Fixture {
        app: App,
        page: Entity,
    }

    impl Fixture {
        fn new(limit: u32) -> Self {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_ecs::PrimitivesPlugin,
                super::super::state::StatePlugin,
                HistoryQueryPlugin,
            ));
            app.init_resource::<Published>().add_observer(
                |write: On<vmux_ecs::UiStateWrite<crate::state::HistoryUiState>>,
                 mut output: ResMut<Published>| {
                    output.0.push(
                        <crate::state::HistoryUiState as vmux_api::UiState>::from_updates(
                            None,
                            vec![write.event().update().clone()],
                        ),
                    );
                },
            );
            let page = app
                .world_mut()
                .spawn((
                    HistoryPageState { limit, ..default() },
                    PageMetadata {
                        url: "vmux://history/".into(),
                        ..default()
                    },
                ))
                .id();
            app.world_mut().entity_mut(page).insert((
                bevy_cef::prelude::BinIpcSurfaceGeneration(1),
                bevy_cef::prelude::BinIpcNavigationRevision(1),
                bevy_cef::prelude::BinIpcEpoch(1),
            ));
            app.world_mut().spawn(vmux_api::UiEventPermissions {
                url: "vmux://history/",
                owns_subtree: false,
                permissions: &[<crate::state::HistoryUiState as vmux_api::BinEvent>::PERMISSION],
            });
            Self { app, page }
        }

        fn url(&mut self, url: &str, title: &str, count: u32, last: i64) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    Url,
                    PageMetadata {
                        url: url.into(),
                        title: title.into(),
                        ..default()
                    },
                    VisitCount(count),
                    LastVisitedAt(last),
                ))
                .id()
        }

        fn visit(&mut self, url: Entity, created: i64) {
            self.app
                .world_mut()
                .spawn((Visit, CreatedAt(created), VisitedUrl(url)));
        }

        fn query(&mut self, query: &str) {
            self.app.world_mut().trigger(UiInput {
                webview: self.page,
                payload: HistoryQueryRequest {
                    query: query.into(),
                },
            });
        }

        fn state(&mut self) -> crate::state::HistoryUiState {
            self.app.update();
            self.app
                .world()
                .resource::<Published>()
                .0
                .last()
                .unwrap()
                .clone()
        }
    }

    #[test]
    fn canonical_visits_order_paginate_and_ignore_removed_urls() {
        let mut fixture = Fixture::new(2);
        let url = fixture.url("https://example.com", "Example", 3, 300);
        for time in [100, 300, 200] {
            fixture.visit(url, time);
        }
        let removed = fixture.url("https://removed.example", "Removed", 1, 400);
        fixture.visit(removed, 400);
        fixture.app.world_mut().despawn(removed);
        let state = fixture.state();
        assert_eq!(
            state
                .entries
                .iter()
                .map(|entry| entry.visit_created_at)
                .collect::<Vec<_>>(),
            [300, 200]
        );
        assert!(state.has_more);
        fixture
            .app
            .world_mut()
            .get_mut::<HistoryPageState>(fixture.page)
            .unwrap()
            .limit = 4;
        let state = fixture.state();
        assert_eq!(
            state
                .entries
                .iter()
                .map(|entry| entry.visit_created_at)
                .collect::<Vec<_>>(),
            [300, 200, 100]
        );
        assert!(!state.has_more);
    }

    #[test]
    fn current_canonical_count_and_age_determine_query_ranking() {
        let mut fixture = Fixture::new(50);
        let now = vmux_ecs::UnixMillis::now().0;
        let older = fixture.url("git.old", "Old", 20, now - 100 * 3_600_000);
        let recent = fixture.url("git.recent", "Recent", 10, now);
        fixture.url("unrelated", "Other", 1000, now);
        fixture.query("git");
        let state = fixture.state();
        assert_eq!(
            state
                .entries
                .iter()
                .map(|entry| entry.url_entity_bits)
                .collect::<Vec<_>>(),
            [recent.to_bits(), older.to_bits()]
        );
        fixture
            .app
            .world_mut()
            .get_mut::<VisitCount>(older)
            .unwrap()
            .0 = 100;
        fixture.query("git");
        let state = fixture.state();
        assert_eq!(state.entries[0].url_entity_bits, older.to_bits());
        assert_eq!(state.query, "git");
    }

    #[test]
    fn prefix_match_outranks_substring_at_equal_frecency() {
        let mut fixture = Fixture::new(50);
        let now = vmux_ecs::UnixMillis::now().0;
        let substring = fixture.url("github.com", "GitHub", 10, now);
        let prefix = fixture.url("hub.example", "GitHub", 10, now);
        fixture.query("HUB");
        let state = fixture.state();
        assert_eq!(
            state
                .entries
                .iter()
                .map(|entry| entry.url_entity_bits)
                .collect::<Vec<_>>(),
            [prefix.to_bits(), substring.to_bits()]
        );
    }
    #[test]
    fn deletion_refuses_non_history_entities_and_non_history_sources() {
        let mut fixture = Fixture::new(50);
        let protected = fixture
            .app
            .world_mut()
            .spawn(Name::new("unrelated state"))
            .id();
        let url = fixture.url("https://example.com", "Example", 1, 100);
        fixture.visit(url, 100);
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.page,
            payload: HistoryDeleteRequest {
                url_entity_bits: protected.to_bits(),
            },
        });
        fixture.app.update();
        assert!(fixture.app.world().get_entity(protected).is_ok());
        let other = fixture
            .app
            .world_mut()
            .spawn(PageMetadata {
                url: "vmux://other/".into(),
                ..default()
            })
            .id();
        fixture.app.world_mut().trigger(UiInput {
            webview: other,
            payload: HistoryDeleteRequest {
                url_entity_bits: url.to_bits(),
            },
        });
        fixture.app.update();
        assert!(fixture.app.world().get_entity(url).is_ok());
        fixture.app.world_mut().trigger(UiInput {
            webview: fixture.page,
            payload: HistoryDeleteRequest {
                url_entity_bits: url.to_bits(),
            },
        });
        let state = fixture.state();
        assert!(fixture.app.world().get_entity(url).is_err());
        assert!(state.entries.is_empty());
    }
}
