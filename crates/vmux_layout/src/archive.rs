use bevy::prelude::*;
use vmux_core::{ArchivedPage, PageArchiveRequest, now_millis};

const MAX_ARCHIVE_ENTRIES: usize = 25;
const ARCHIVE_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

pub struct ArchivePlugin;

impl Plugin for ArchivePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, (capture_archived_pages, maintain_archive));
    }
}

fn capture_archived_pages(mut reader: MessageReader<PageArchiveRequest>, mut commands: Commands) {
    for req in reader.read() {
        if req.url.is_empty() {
            continue;
        }
        commands.spawn(ArchivedPage {
            url: req.url.clone(),
            title: req.title.clone(),
            space_id: req.space_id.clone(),
            closed_at: now_millis(),
            launch: req.launch.clone(),
        });
    }
}

fn maintain_archive(archived: Query<(Entity, &ArchivedPage)>, mut commands: Commands) {
    let now = now_millis();
    let mut live: Vec<(Entity, i64)> = Vec::new();
    for (entity, page) in &archived {
        if now - page.closed_at > ARCHIVE_TTL_MS {
            commands.entity(entity).despawn();
        } else {
            live.push((entity, page.closed_at));
        }
    }
    if live.len() > MAX_ARCHIVE_ENTRIES {
        live.sort_by_key(|(_, closed_at)| *closed_at);
        let overflow = live.len() - MAX_ARCHIVE_ENTRIES;
        for (entity, _) in live.into_iter().take(overflow) {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(url: &str, closed_at: i64) -> ArchivedPage {
        ArchivedPage {
            url: url.to_string(),
            title: String::new(),
            space_id: "s".to_string(),
            closed_at,
            launch: None,
        }
    }

    #[test]
    fn capture_spawns_archived_page() {
        let mut app = App::new();
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, capture_archived_pages);
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .write(PageArchiveRequest {
                url: "https://a.example".to_string(),
                title: "A".to_string(),
                space_id: "s".to_string(),
                launch: None,
            });
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let all: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].url, "https://a.example");
    }

    #[test]
    fn capture_skips_empty_url() {
        let mut app = App::new();
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, capture_archived_pages);
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .write(PageArchiveRequest {
                url: String::new(),
                title: String::new(),
                space_id: "s".to_string(),
                launch: None,
            });
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        assert_eq!(q.iter(app.world()).count(), 0);
    }

    #[test]
    fn maintain_enforces_cap_dropping_oldest() {
        let mut app = App::new();
        app.add_systems(Update, maintain_archive);
        let now = now_millis();
        for i in 0..(MAX_ARCHIVE_ENTRIES as i64 + 1) {
            app.world_mut().spawn(page(&format!("u{i}"), now - i));
        }
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
        assert_eq!(urls.len(), MAX_ARCHIVE_ENTRIES);
        let oldest = format!("u{}", MAX_ARCHIVE_ENTRIES);
        assert!(!urls.contains(&oldest));
    }

    #[test]
    fn maintain_purges_expired() {
        let mut app = App::new();
        app.add_systems(Update, maintain_archive);
        let now = now_millis();
        app.world_mut().spawn(page("fresh", now));
        app.world_mut()
            .spawn(page("stale", now - ARCHIVE_TTL_MS - 1));
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
        assert_eq!(urls, vec!["fresh".to_string()]);
    }
}
