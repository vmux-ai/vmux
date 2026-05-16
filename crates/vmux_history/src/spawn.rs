use bevy::prelude::*;
use vmux_core::{CreatedAt, LastVisitedAt, PageMetadata, Url, VisitCount, now_millis};

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
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CorePlugin);
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
