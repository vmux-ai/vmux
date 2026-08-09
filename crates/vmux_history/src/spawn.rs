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
#[path = "spawn.test.rs"]
mod tests;
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
#[path = "spawn.system.test.rs"]
mod system_tests;
