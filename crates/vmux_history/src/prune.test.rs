use super::*;
use vmux_core::{CorePlugin, PageMetadata, VisitCount};

fn app() -> App {
    let mut a = App::new();
    a.add_plugins(MinimalPlugins);
    a.add_plugins(CorePlugin);
    a.add_systems(Update, prune_history);
    a
}

#[test]
fn removes_old_visits_and_urls() {
    let mut a = app();
    let old = now_millis() - RETENTION_MS - 1000;
    let url_e = a
        .world_mut()
        .spawn((
            Url,
            PageMetadata {
                url: "old".into(),
                ..default()
            },
            VisitCount(1),
            LastVisitedAt(old),
            CreatedAt(old),
        ))
        .id();
    a.world_mut()
        .spawn((Visit, CreatedAt(old), VisitedUrl(url_e)));
    a.update();
    assert_eq!(a.world_mut().query::<&Url>().iter(a.world()).count(), 0);
    assert_eq!(a.world_mut().query::<&Visit>().iter(a.world()).count(), 0);
}

#[test]
fn keeps_recent_entries() {
    let mut a = app();
    let now = now_millis();
    let url_e = a
        .world_mut()
        .spawn((
            Url,
            PageMetadata {
                url: "recent".into(),
                ..default()
            },
            VisitCount(1),
            LastVisitedAt(now),
            CreatedAt(now),
        ))
        .id();
    a.world_mut()
        .spawn((Visit, CreatedAt(now), VisitedUrl(url_e)));
    a.update();
    assert_eq!(a.world_mut().query::<&Url>().iter(a.world()).count(), 1);
    assert_eq!(a.world_mut().query::<&Visit>().iter(a.world()).count(), 1);
}

#[test]
fn keeps_url_when_recent_visit_exists() {
    let mut a = app();
    let now = now_millis();
    let old = now - RETENTION_MS - 1000;
    let url_e = a
        .world_mut()
        .spawn((
            Url,
            PageMetadata {
                url: "u".into(),
                ..default()
            },
            VisitCount(2),
            LastVisitedAt(now),
            CreatedAt(old),
        ))
        .id();
    a.world_mut()
        .spawn((Visit, CreatedAt(now), VisitedUrl(url_e)));
    a.update();
    assert_eq!(a.world_mut().query::<&Url>().iter(a.world()).count(), 1);
}
