use bevy::prelude::*;
use vmux_core::{
    CreatedAt, LastVisitedAt, PageMetadata, TransitionType, Url, Visit, VisitCount, VisitedUrl,
    now_millis,
};

pub fn find_or_create_url(world: &mut World, url: &str) -> Entity {
    let mut existing = None;
    let mut query = world.query::<(Entity, &PageMetadata)>();
    for (e, meta) in query.iter(world) {
        if world.get::<Url>(e).is_some() && meta.url == url {
            existing = Some(e);
            break;
        }
    }
    if let Some(e) = existing {
        return e;
    }
    let now = now_millis();
    world
        .spawn((
            Url,
            PageMetadata {
                url: url.to_string(),
                ..default()
            },
            VisitCount(0),
            LastVisitedAt(0),
            CreatedAt(now),
        ))
        .id()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::CorePlugin;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(CorePlugin);
        app
    }

    #[test]
    fn creates_when_missing() {
        let mut app = app();
        let e = find_or_create_url(app.world_mut(), "https://example.com");
        assert!(app.world().get::<Url>(e).is_some());
        let meta = app.world().get::<PageMetadata>(e).unwrap();
        assert_eq!(meta.url, "https://example.com");
        assert_eq!(app.world().get::<VisitCount>(e).unwrap().0, 0);
    }

    #[test]
    fn returns_existing_match() {
        let mut app = app();
        let e1 = find_or_create_url(app.world_mut(), "https://example.com");
        let e2 = find_or_create_url(app.world_mut(), "https://example.com");
        assert_eq!(e1, e2);
    }

    #[test]
    fn distinct_urls_get_distinct_entities() {
        let mut app = app();
        let a = find_or_create_url(app.world_mut(), "https://a.com");
        let b = find_or_create_url(app.world_mut(), "https://b.com");
        assert_ne!(a, b);
    }

    #[test]
    fn ignores_entities_without_url_marker() {
        let mut app = app();
        app.world_mut().spawn(PageMetadata {
            url: "https://example.com".into(),
            ..default()
        });
        let e = find_or_create_url(app.world_mut(), "https://example.com");
        assert!(app.world().get::<Url>(e).is_some());
    }
}

pub fn spawn_visits(
    mut events: bevy::ecs::message::MessageReader<
        bevy_cef_core::prelude::WebviewCommittedNavigationEvent,
    >,
    mut commands: Commands,
    mut urls: Query<(Entity, &PageMetadata, &mut VisitCount, &mut LastVisitedAt), With<Url>>,
) {
    for ev in events.read() {
        if !ev.is_main_frame {
            continue;
        }
        if ev.url.starts_with("vmux://") || ev.url.is_empty() {
            continue;
        }
        let now = now_millis();
        let transition = crate::transition::map(ev.transition, ev.qualifiers);
        record_visit(&mut commands, &mut urls, &ev.url, "", transition, now);
    }
}

/// Find-or-create the `Url` entity for `url` (bumping `VisitCount`/`LastVisitedAt`),
/// then spawn a `Visit` unless this was a back/forward navigation. Sets the title on
/// newly-created urls (browser visits pass ""); existing urls keep their title.
pub(crate) fn record_visit(
    commands: &mut Commands,
    urls: &mut Query<(Entity, &PageMetadata, &mut VisitCount, &mut LastVisitedAt), With<Url>>,
    url: &str,
    title: &str,
    transition: TransitionType,
    now: i64,
) {
    let mut url_entity = None;
    for (e, meta, mut count, mut last) in urls.iter_mut() {
        if meta.url == url {
            count.0 = count.0.saturating_add(1);
            last.0 = now;
            url_entity = Some(e);
            break;
        }
    }

    let url_e = match url_entity {
        Some(e) => e,
        None => commands
            .spawn((
                Url,
                PageMetadata {
                    url: url.to_string(),
                    title: title.to_string(),
                    ..default()
                },
                VisitCount(1),
                LastVisitedAt(now),
                CreatedAt(now),
            ))
            .id(),
    };

    if transition != TransitionType::BackForward {
        commands.spawn((Visit, CreatedAt(now), VisitedUrl(url_e), transition));
    }
}

/// Record visits requested by other domains (the editor's `file://` opens) into the
/// same history store, so file opens persist and rank like browser navigations.
pub fn record_requested_visits(
    mut reader: bevy::ecs::message::MessageReader<vmux_core::event::RecordVisitRequest>,
    mut commands: Commands,
    mut urls: Query<(Entity, &PageMetadata, &mut VisitCount, &mut LastVisitedAt), With<Url>>,
) {
    let now = now_millis();
    for req in reader.read() {
        if req.url.is_empty() || req.url.starts_with("vmux://") {
            continue;
        }
        record_visit(
            &mut commands,
            &mut urls,
            &req.url,
            &req.title,
            TransitionType::Typed,
            now,
        );
    }
}

#[cfg(test)]
mod system_tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy_cef_core::prelude::{
        CefTransitionCore, CefTransitionQualifiers, WebviewCommittedNavigationEvent,
    };
    use vmux_core::CorePlugin;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CorePlugin)
            .add_message::<WebviewCommittedNavigationEvent>()
            .add_systems(Update, spawn_visits);
        app
    }

    fn send(app: &mut App, url: &str, transition: CefTransitionCore, forward_back: bool) {
        let mut writer = app
            .world_mut()
            .resource_mut::<Messages<WebviewCommittedNavigationEvent>>();
        writer.write(WebviewCommittedNavigationEvent {
            webview: Entity::PLACEHOLDER,
            url: url.into(),
            is_main_frame: true,
            transition,
            qualifiers: CefTransitionQualifiers {
                forward_back,
                ..Default::default()
            },
        });
    }

    #[test]
    fn first_visit_spawns_url_and_visit() {
        let mut app = app();
        send(
            &mut app,
            "https://example.com",
            CefTransitionCore::Link,
            false,
        );
        app.update();
        let urls = app.world_mut().query::<&Url>().iter(app.world()).count();
        let visits = app.world_mut().query::<&Visit>().iter(app.world()).count();
        assert_eq!(urls, 1);
        assert_eq!(visits, 1);
    }

    #[test]
    fn second_visit_same_url_increments_count() {
        let mut app = app();
        send(
            &mut app,
            "https://example.com",
            CefTransitionCore::Link,
            false,
        );
        app.update();
        send(
            &mut app,
            "https://example.com",
            CefTransitionCore::Link,
            false,
        );
        app.update();
        let urls = app.world_mut().query::<&Url>().iter(app.world()).count();
        let visits = app.world_mut().query::<&Visit>().iter(app.world()).count();
        assert_eq!(urls, 1);
        assert_eq!(visits, 2);
        let count = app
            .world_mut()
            .query::<&VisitCount>()
            .iter(app.world())
            .next()
            .unwrap()
            .0;
        assert_eq!(count, 2);
    }

    #[test]
    fn back_forward_bumps_count_but_no_visit() {
        let mut app = app();
        send(
            &mut app,
            "https://example.com",
            CefTransitionCore::Link,
            false,
        );
        app.update();
        send(
            &mut app,
            "https://example.com",
            CefTransitionCore::Link,
            true,
        );
        app.update();
        let visits = app.world_mut().query::<&Visit>().iter(app.world()).count();
        let count = app
            .world_mut()
            .query::<&VisitCount>()
            .iter(app.world())
            .next()
            .unwrap()
            .0;
        assert_eq!(visits, 1);
        assert_eq!(count, 2);
    }

    #[test]
    fn subframe_skipped() {
        let mut app = app();
        let mut writer = app
            .world_mut()
            .resource_mut::<Messages<WebviewCommittedNavigationEvent>>();
        writer.write(WebviewCommittedNavigationEvent {
            webview: Entity::PLACEHOLDER,
            url: "https://example.com".into(),
            is_main_frame: false,
            transition: CefTransitionCore::Link,
            qualifiers: CefTransitionQualifiers::default(),
        });
        app.update();
        assert_eq!(
            app.world_mut().query::<&Visit>().iter(app.world()).count(),
            0
        );
    }

    #[test]
    fn vmux_scheme_skipped() {
        let mut app = app();
        send(&mut app, "vmux://history", CefTransitionCore::Link, false);
        app.update();
        assert_eq!(app.world_mut().query::<&Url>().iter(app.world()).count(), 0);
    }

    #[test]
    fn record_request_spawns_url_with_title() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CorePlugin)
            .add_message::<vmux_core::event::RecordVisitRequest>()
            .add_systems(Update, record_requested_visits);
        app.world_mut()
            .resource_mut::<Messages<vmux_core::event::RecordVisitRequest>>()
            .write(vmux_core::event::RecordVisitRequest {
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
