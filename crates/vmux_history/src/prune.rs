use bevy::prelude::*;
use vmux_core::{CreatedAt, LastVisitedAt, Url, Visit, VisitedUrl, now_millis};

pub const RETENTION_MS: i64 = 90 * 86_400_000;

pub fn prune_history(
    mut commands: Commands,
    visits: Query<(Entity, &CreatedAt, &VisitedUrl), With<Visit>>,
    urls: Query<(Entity, &LastVisitedAt), With<Url>>,
) {
    let cutoff = now_millis() - RETENTION_MS;

    let mut pruned_visits = Vec::<Entity>::new();
    for (e, created, _) in visits.iter() {
        if created.0 < cutoff {
            commands.entity(e).despawn();
            pruned_visits.push(e);
        }
    }

    for (url_e, last) in urls.iter() {
        if last.0 < cutoff {
            let has_remaining = visits
                .iter()
                .any(|(ve, _, visited)| visited.0 == url_e && !pruned_visits.contains(&ve));
            if !has_remaining {
                commands.entity(url_e).despawn();
            }
        }
    }
}

#[cfg(test)]
#[path = "prune.test.rs"]
mod tests;
