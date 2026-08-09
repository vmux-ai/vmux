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
