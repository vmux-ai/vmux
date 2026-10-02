use bevy::prelude::*;
use vmux_api::VmuxRoute;
use vmux_ecs::{
    CreatedAt, LastVisitedAt, PageMetadata, TransitionType, UnixMillis, Url, Visit, VisitCount,
    VisitedUrl, page::PageReady,
};

pub struct HistorySpawnPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct HistoryWriteSet;

#[derive(bevy::ecs::system::SystemParam)]
struct VisitWriter<'w, 's> {
    urls: Query<
        'w,
        's,
        (
            Entity,
            &'static PageMetadata,
            &'static mut VisitCount,
            &'static mut LastVisitedAt,
        ),
        With<Url>,
    >,
    commands: Commands<'w, 's>,
}

impl VisitWriter<'_, '_> {
    fn record(&mut self, url: &str, title: &str, transition: TransitionType, now: i64) {
        let mut url_entity = None;
        for (entity, metadata, mut count, mut last) in &mut self.urls {
            if metadata.url == url {
                count.0 = count.0.saturating_add(1);
                last.0 = now;
                url_entity = Some(entity);
                break;
            }
        }

        let url_entity = match url_entity {
            Some(entity) => entity,
            None => self
                .commands
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
            self.commands
                .spawn((Visit, CreatedAt(now), VisitedUrl(url_entity), transition));
        }
    }
}

impl Plugin for HistorySpawnPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<vmux_ecs::event::RecordVisitRequest>()
            .add_systems(
                Update,
                (spawn, record_requested_visits, record_vmux_pages)
                    .chain()
                    .in_set(HistoryWriteSet),
            );
    }
}

fn spawn(
    mut events: bevy::ecs::message::MessageReader<
        bevy_cef_core::prelude::WebviewCommittedNavigationEvent,
    >,
    mut visits: VisitWriter,
) {
    for ev in events.read() {
        if !ev.is_main_frame {
            continue;
        }
        if VmuxRoute::parse(&ev.url).is_some() || ev.url.is_empty() {
            continue;
        }
        let now = UnixMillis::now().0;
        let transition = super::transition::map(ev.transition, ev.qualifiers);
        visits.record(&ev.url, "", transition, now);
    }
}

fn record_requested_visits(
    mut reader: bevy::ecs::message::MessageReader<vmux_ecs::event::RecordVisitRequest>,
    mut visits: VisitWriter,
) {
    let now = UnixMillis::now().0;
    for req in reader.read() {
        if req.url.is_empty() || VmuxRoute::parse(&req.url).is_some() {
            continue;
        }
        visits.record(&req.url, &req.title, TransitionType::Typed, now);
    }
}

fn record_vmux_pages(
    pages: Query<&PageMetadata, (Added<PageReady>, Without<Url>)>,
    mut visits: VisitWriter,
) {
    let now = UnixMillis::now().0;
    for page in &pages {
        if !recordable_vmux_url(&page.url) {
            continue;
        }
        visits.record(&page.url, &page.title, TransitionType::Typed, now);
    }
}

fn recordable_vmux_url(url: &str) -> bool {
    let Some(route) = VmuxRoute::parse(url) else {
        return false;
    };
    !matches!(route.host(), "history" | "layout" | "command-bar")
}

#[cfg(test)]
mod system_tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy_cef_core::prelude::{
        CefTransitionCore, CefTransitionQualifiers, WebviewCommittedNavigationEvent,
    };
    use vmux_ecs::EcsPlugin;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(EcsPlugin)
            .add_message::<WebviewCommittedNavigationEvent>()
            .add_systems(Update, spawn);
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
    fn committed_vmux_navigation_is_skipped_for_page_ready_recording() {
        let mut app = app();
        send(&mut app, "vmux://history", CefTransitionCore::Link, false);
        app.update();
        assert_eq!(app.world_mut().query::<&Url>().iter(app.world()).count(), 0);
    }

    #[test]
    fn ready_vmux_page_is_recorded_but_history_shell_is_not() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(EcsPlugin)
            .add_systems(Update, record_vmux_pages);
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
            .add_plugins(EcsPlugin)
            .add_message::<vmux_ecs::event::RecordVisitRequest>()
            .add_systems(Update, record_requested_visits);
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
