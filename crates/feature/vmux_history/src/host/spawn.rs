use bevy::prelude::*;
use bevy::prelude::{App, Commands, Entity, MessageReader, Plugin, Query, With};
use bevy_cef_core::prelude::{CefTransitionCore, WebviewCommittedNavigationEvent};
use vmux_api::VmuxRoute;
use vmux_ecs::page::PageReady;
use vmux_ecs::{
    CreatedAt, LastVisitedAt, Operation, OperationState, Order, PageMetadata, TransitionType,
    UnixMillis, Url, Visit, VisitCount, VisitedUrl,
};

pub struct HistorySpawnPlugin;

impl Plugin for HistorySpawnPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_ecs::event::RecordVisitRequest>()
            .add_systems(
                Update,
                (committed, requested, ready, record)
                    .chain()
                    .in_set(HistoryWriteSet),
            )
            .add_observer(apply);
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct HistoryWriteSet;

#[derive(Component)]
struct Recording;

fn committed(
    mut events: MessageReader<WebviewCommittedNavigationEvent>,
    orders: Query<&Order, With<Recording>>,
    mut commands: Commands,
) {
    let mut next = 0;
    for order in &orders {
        next = next.max(order.0.saturating_add(1));
    }
    for event in events.read() {
        if !event.is_main_frame || event.url.is_empty() || VmuxRoute::parse(&event.url).is_some() {
            continue;
        }
        let transition = if event.qualifiers.forward_back {
            TransitionType::BackForward
        } else if event.qualifiers.client_redirect || event.qualifiers.server_redirect {
            TransitionType::Redirect
        } else {
            match event.transition {
                CefTransitionCore::Reload => TransitionType::Reload,
                CefTransitionCore::Explicit
                | CefTransitionCore::Generated
                | CefTransitionCore::Keyword
                | CefTransitionCore::KeywordGenerated => TransitionType::Typed,
                CefTransitionCore::Link
                | CefTransitionCore::FormSubmit
                | CefTransitionCore::AutoBookmark => TransitionType::Link,
                _ => TransitionType::Other,
            }
        };
        commands.spawn((
            Operation::bundle("history.record").expect("history record kind is valid"),
            Name::new("Record history visit"),
            Order(next),
            CreatedAt(UnixMillis::now().0),
            PageMetadata {
                url: event.url.clone(),
                ..default()
            },
            transition,
            Recording,
        ));
        next = next.saturating_add(1);
    }
}

fn requested(
    mut events: MessageReader<vmux_ecs::event::RecordVisitRequest>,
    orders: Query<&Order, With<Recording>>,
    mut commands: Commands,
) {
    let mut next = 0;
    for order in &orders {
        next = next.max(order.0.saturating_add(1));
    }
    let now = UnixMillis::now().0;
    for event in events.read() {
        if event.url.is_empty() || VmuxRoute::parse(&event.url).is_some() {
            continue;
        }
        commands.spawn((
            Operation::bundle("history.record").expect("history record kind is valid"),
            Name::new("Record history visit"),
            Order(next),
            CreatedAt(now),
            PageMetadata {
                url: event.url.clone(),
                title: event.title.clone(),
                ..default()
            },
            TransitionType::Typed,
            Recording,
        ));
        next = next.saturating_add(1);
    }
}

type ReadyPages = (Added<PageReady>, Without<Url>, Without<Recording>);

fn ready(
    pages: Query<&PageMetadata, ReadyPages>,
    orders: Query<&Order, With<Recording>>,
    mut commands: Commands,
) {
    let mut next = 0;
    for order in &orders {
        next = next.max(order.0.saturating_add(1));
    }
    let now = UnixMillis::now().0;
    for page in &pages {
        let Some(route) = VmuxRoute::parse(&page.url) else {
            continue;
        };
        if matches!(route.host(), "history" | "layout" | "command-bar") {
            continue;
        }
        commands.spawn((
            Operation::bundle("history.record").expect("history record kind is valid"),
            Name::new("Record history visit"),
            Order(next),
            CreatedAt(now),
            PageMetadata {
                url: page.url.clone(),
                title: page.title.clone(),
                ..default()
            },
            TransitionType::Typed,
            Recording,
        ));
        next = next.saturating_add(1);
    }
}

#[derive(EntityEvent)]
struct RecordingRequested {
    entity: Entity,
}

fn record(
    recordings: Query<
        (Entity, &Order, &PageMetadata, &CreatedAt, &TransitionType),
        With<Recording>,
    >,
    mut commands: Commands,
) {
    let mut pending = Vec::new();
    for (entity, order, ..) in &recordings {
        pending.push((order.0, entity));
    }
    pending.sort_by_key(|(order, _)| *order);
    for (_, entity) in pending {
        commands.trigger(RecordingRequested { entity });
    }
}

fn apply(
    request: On<RecordingRequested>,
    mut recordings: Query<
        (
            &PageMetadata,
            &CreatedAt,
            &TransitionType,
            &mut OperationState,
        ),
        With<Recording>,
    >,
    mut urls: Query<(Entity, &PageMetadata, &mut VisitCount, &mut LastVisitedAt), With<Url>>,
    mut commands: Commands,
) {
    let operation = request.entity;
    let Ok((metadata, created, transition, mut state)) = recordings.get_mut(operation) else {
        commands.entity(operation).try_despawn();
        return;
    };
    if !matches!(*state, OperationState::Pending | OperationState::Running) {
        commands.entity(operation).despawn();
        return;
    }
    *state = OperationState::Running;
    let mut existing = None;
    for (entity, current, ..) in &urls {
        if current.url == metadata.url {
            existing = Some(entity);
            break;
        }
    }
    let url = match existing {
        Some(entity) => {
            let (_, _, mut count, mut last) = urls.get_mut(entity).unwrap();
            count.0 = count.0.saturating_add(1);
            last.0 = created.0;
            entity
        }
        None => commands
            .spawn((
                Url,
                moonshine_save::prelude::Save,
                metadata.clone(),
                VisitCount(1),
                LastVisitedAt(created.0),
                *created,
            ))
            .id(),
    };
    if *transition != TransitionType::BackForward {
        commands.spawn((
            Visit,
            moonshine_save::prelude::Save,
            *created,
            VisitedUrl(url),
            *transition,
        ));
    }
    commands.entity(operation).despawn();
}

#[cfg(test)]
mod system_tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy_cef_core::prelude::{
        CefTransitionCore, CefTransitionQualifiers, WebviewCommittedNavigationEvent,
    };
    use vmux_ecs::PrimitivesPlugin;

    #[test]
    fn cancellation_after_an_earlier_visit_prevents_later_pending_writes() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PrimitivesPlugin, HistorySpawnPlugin))
            .add_message::<WebviewCommittedNavigationEvent>()
            .add_observer(
                |_: On<Add, Visit>, mut operations: Query<&mut OperationState, With<Recording>>| {
                    for mut state in &mut operations {
                        if *state == OperationState::Pending {
                            *state = OperationState::Canceled;
                        }
                    }
                },
            );
        for _ in 0..2 {
            app.world_mut()
                .write_message(vmux_ecs::event::RecordVisitRequest {
                    url: "https://example.com/canceled".into(),
                    title: "Canceled".into(),
                });
        }
        app.update();
        let counts: Vec<_> = app
            .world_mut()
            .query_filtered::<&VisitCount, With<Url>>()
            .iter(app.world())
            .map(|count| count.0)
            .collect();
        assert_eq!(counts, [1]);
        assert_eq!(
            app.world_mut().query::<&Visit>().iter(app.world()).count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query::<&Recording>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn same_update_recording_requests_share_one_url_and_keep_each_visit() {
        for source in 0..3 {
            let mut fixture = Fixture::new();
            for event in 0..2 {
                if source == 1 || source == 2 && event == 0 {
                    fixture.send(
                        "https://example.com/repeated",
                        CefTransitionCore::Link,
                        false,
                    );
                } else {
                    fixture
                        .app
                        .world_mut()
                        .write_message(vmux_ecs::event::RecordVisitRequest {
                            url: "https://example.com/repeated".into(),
                            title: "Repeated".into(),
                        });
                }
            }
            let app = &mut fixture.app;
            app.update();
            let urls: Vec<_> = app
                .world_mut()
                .query_filtered::<(Entity, &VisitCount), With<Url>>()
                .iter(app.world())
                .map(|(entity, count)| (entity, count.0))
                .collect();
            assert_eq!(urls.len(), 1);
            assert_eq!(urls[0].1, 2);
            let visits: Vec<_> = app
                .world_mut()
                .query_filtered::<&VisitedUrl, With<Visit>>()
                .iter(app.world())
                .map(|visited| visited.0)
                .collect();
            assert_eq!(visits, [urls[0].0, urls[0].0]);
        }
    }

    struct Fixture {
        app: App,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, PrimitivesPlugin, HistorySpawnPlugin))
                .add_message::<WebviewCommittedNavigationEvent>();
            Self { app }
        }

        fn send(&mut self, url: &str, transition: CefTransitionCore, forward_back: bool) {
            self.app
                .world_mut()
                .write_message(WebviewCommittedNavigationEvent {
                    at: std::time::Instant::now(),
                    webview: Entity::PLACEHOLDER,
                    url: url.into(),
                    is_main_frame: true,
                    navigation_epoch: None,
                    transition,
                    qualifiers: CefTransitionQualifiers {
                        forward_back,
                        ..Default::default()
                    },
                });
        }
    }

    #[test]
    fn first_visit_spawns_url_and_visit() {
        let mut fixture = Fixture::new();
        fixture.send("https://example.com", CefTransitionCore::Link, false);
        fixture.app.update();
        let urls = fixture
            .app
            .world_mut()
            .query::<&Url>()
            .iter(fixture.app.world())
            .count();
        let visits = fixture
            .app
            .world_mut()
            .query::<&Visit>()
            .iter(fixture.app.world())
            .count();
        assert_eq!(urls, 1);
        assert_eq!(visits, 1);
    }

    #[test]
    fn second_visit_same_url_increments_count() {
        let mut fixture = Fixture::new();
        fixture.send("https://example.com", CefTransitionCore::Link, false);
        fixture.app.update();
        fixture.send("https://example.com", CefTransitionCore::Link, false);
        fixture.app.update();
        let urls = fixture
            .app
            .world_mut()
            .query::<&Url>()
            .iter(fixture.app.world())
            .count();
        let visits = fixture
            .app
            .world_mut()
            .query::<&Visit>()
            .iter(fixture.app.world())
            .count();
        assert_eq!(urls, 1);
        assert_eq!(visits, 2);
        let count = fixture
            .app
            .world_mut()
            .query::<&VisitCount>()
            .iter(fixture.app.world())
            .next()
            .unwrap()
            .0;
        assert_eq!(count, 2);
    }

    #[test]
    fn back_forward_bumps_count_but_no_visit() {
        let mut fixture = Fixture::new();
        fixture.send("https://example.com", CefTransitionCore::Link, false);
        fixture.app.update();
        fixture.send("https://example.com", CefTransitionCore::Link, true);
        fixture.app.update();
        let visits = fixture
            .app
            .world_mut()
            .query::<&Visit>()
            .iter(fixture.app.world())
            .count();
        let count = fixture
            .app
            .world_mut()
            .query::<&VisitCount>()
            .iter(fixture.app.world())
            .next()
            .unwrap()
            .0;
        assert_eq!(visits, 1);
        assert_eq!(count, 2);
    }

    #[test]
    fn subframe_skipped() {
        let mut fixture = Fixture::new();
        let mut writer = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<WebviewCommittedNavigationEvent>>();
        writer.write(WebviewCommittedNavigationEvent {
            at: std::time::Instant::now(),
            webview: Entity::PLACEHOLDER,
            url: "https://example.com".into(),
            is_main_frame: false,
            navigation_epoch: None,
            transition: CefTransitionCore::Link,
            qualifiers: CefTransitionQualifiers::default(),
        });
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&Visit>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn committed_vmux_navigation_is_skipped_for_page_ready_recording() {
        let mut fixture = Fixture::new();
        fixture.send("vmux://history", CefTransitionCore::Link, false);
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query::<&Url>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn ready_vmux_page_is_recorded_but_history_shell_is_not() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PrimitivesPlugin)
            .add_message::<WebviewCommittedNavigationEvent>()
            .add_plugins(HistorySpawnPlugin);
        app.world_mut().spawn((
            PageMetadata {
                url: "vmux://team/".into(),
                title: "Team".into(),
                ..default()
            },
            PageReady {},
        ));
        app.world_mut().spawn((
            PageMetadata {
                url: "vmux://history/".into(),
                title: "History".into(),
                ..default()
            },
            PageReady {},
        ));

        app.update();

        let urls: Vec<_> = app
            .world_mut()
            .query::<(&Url, &PageMetadata)>()
            .iter(app.world())
            .map(|(_, metadata)| metadata.url.clone())
            .collect();
        assert_eq!(urls, ["vmux://team/"]);
        assert_eq!(
            app.world_mut().query::<&Visit>().iter(app.world()).count(),
            1
        );
    }

    #[test]
    fn record_request_spawns_url_with_title() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PrimitivesPlugin)
            .add_message::<vmux_ecs::event::RecordVisitRequest>()
            .add_message::<WebviewCommittedNavigationEvent>()
            .add_plugins(HistorySpawnPlugin);
        app.world_mut()
            .resource_mut::<Messages<vmux_ecs::event::RecordVisitRequest>>()
            .write(vmux_ecs::event::RecordVisitRequest {
                url: "file:///Users/me/main.rs".into(),
                title: "main.rs".into(),
            });
        app.update();
        let mut q = app.world_mut().query::<(&PageMetadata, &VisitCount)>();
        let (meta, count) = q.iter(app.world()).next().expect("url recorded");
        assert_eq!(meta.url, "file:///Users/me/main.rs");
        assert_eq!(meta.title, "main.rs");
        assert_eq!(count.0, 1);
    }
}
