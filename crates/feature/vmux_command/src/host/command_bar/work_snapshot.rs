use crate::host::snapshot::{CommandBarWorkDirectory, CommandBarWorkSnapshot};
use bevy::prelude::*;
use bevy::prelude::{Added, Changed, Commands};
use std::cmp::Reverse;
use std::path::Path;
use vmux_api::command_bar::{CommandBarRecentFile, CommandBarWorkDir, SearchEngine};
use vmux_ecs::{LastActivatedAt, LastVisitedAt, PageMetadata, UnixMillis, Url, VisitCount};
use vmux_feature::manifest::FeatureManifest;

pub(super) struct WorkSnapshotPlugin;

impl bevy::app::Plugin for WorkSnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(
            Update,
            (directories, recent).in_set(crate::host::snapshot::WriteCommandBarSnapshots),
        );
    }
}

#[derive(Component, Default)]
struct WorkDirectoryCache(Vec<String>);

impl WorkDirectoryCache {
    const ENTRIES_CAP: usize = 40;

    fn read_entries(cwd: &str) -> Vec<CommandBarWorkDir> {
        let Ok(read) = std::fs::read_dir(cwd) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().is_ok_and(|file_type| file_type.is_dir());
            let path = entry.path().to_string_lossy().to_string();
            rows.push((name, is_dir, path));
        }
        rows.sort_by(|a, b| {
            let a_hidden = a.0.starts_with('.');
            let b_hidden = b.0.starts_with('.');
            b.1.cmp(&a.1)
                .then(a_hidden.cmp(&b_hidden))
                .then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });
        rows.into_iter()
            .map(|(_, is_dir, path)| CommandBarWorkDir { path, is_dir })
            .collect()
    }
}

#[derive(Component, Default)]
struct RecentFilesState {
    initialized: bool,
}

impl RecentFilesState {
    const CAP: usize = 20;

    fn score_page(
        metadata: &PageMetadata,
        visit_count: VisitCount,
        last_visited_at: LastVisitedAt,
        now: i64,
    ) -> Option<(f32, CommandBarRecentFile)> {
        let path = metadata.url.strip_prefix("file://")?;
        if Path::new(path).is_dir() {
            return None;
        }
        let age_hours = ((now - last_visited_at.0).max(0) as f32) / 3_600_000.0;
        let decay = 1.0 / (1.0 + age_hours / 24.0);
        Some((
            (visit_count.0 as f32) * decay,
            CommandBarRecentFile {
                url: metadata.url.clone(),
                title: metadata.title.clone(),
            },
        ))
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Command bar work"),
        CommandBarWorkSnapshot::default(),
        WorkDirectoryCache::default(),
        RecentFilesState::default(),
    ));
}

fn directories(
    directories: Query<(&CommandBarWorkDirectory, Option<&LastActivatedAt>)>,
    state: Single<(&mut CommandBarWorkSnapshot, &mut WorkDirectoryCache)>,
) {
    let (mut snapshot, mut cache) = state.into_inner();
    let mut collected: Vec<(String, i64)> = Vec::new();
    for (directory, activated_at) in &directories {
        if directory.0.is_empty() {
            continue;
        }
        let at = activated_at.map(|value| value.0).unwrap_or(0);
        if let Some(existing) = collected
            .iter_mut()
            .find(|(candidate, _)| candidate == &directory.0)
        {
            existing.1 = existing.1.max(at);
        } else {
            collected.push((directory.0.clone(), at));
        }
    }
    collected.sort_by_key(|(_, activated_at)| Reverse(*activated_at));
    let cwds: Vec<String> = collected.into_iter().map(|(path, _)| path).collect();
    if cache.0 == cwds {
        return;
    }
    cache.0.clone_from(&cwds);

    let mut entries = Vec::new();
    for cwd in &cwds {
        for entry in WorkDirectoryCache::read_entries(cwd) {
            if entries
                .iter()
                .any(|existing: &CommandBarWorkDir| existing.path == entry.path)
            {
                continue;
            }
            entries.push(entry);
            if entries.len() >= WorkDirectoryCache::ENTRIES_CAP {
                break;
            }
        }
        if entries.len() >= WorkDirectoryCache::ENTRIES_CAP {
            break;
        }
    }
    if entries != snapshot.work_dirs {
        snapshot.work_dirs = entries;
    }
}

type RecentPageChanges<'w, 's> = Query<'w, 's, (), Or<(Added<Url>, Changed<LastVisitedAt>)>>;

fn recent(
    changed: RecentPageChanges,
    urls: Query<(&PageMetadata, &VisitCount, &LastVisitedAt), With<Url>>,
    manifests: Query<Ref<FeatureManifest>>,
    state: Single<(&mut CommandBarWorkSnapshot, &mut RecentFilesState)>,
) {
    let (mut snapshot, mut recent_state) = state.into_inner();
    let catalog_changed = manifests
        .iter()
        .any(|manifest| manifest.is_added() || manifest.is_changed());
    if recent_state.initialized && changed.is_empty() && !catalog_changed {
        return;
    }
    recent_state.initialized = true;
    let now = UnixMillis::now().0;
    let mut scored = Vec::new();
    for (metadata, visit_count, last_visited_at) in &urls {
        if let Some(file) =
            RecentFilesState::score_page(metadata, *visit_count, *last_visited_at, now)
        {
            scored.push(file);
        }
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut recent_files = Vec::new();
    for (_, file) in scored.into_iter().take(RecentFilesState::CAP) {
        recent_files.push(file);
    }

    let mut catalog = Vec::new();
    for manifest in &manifests {
        for engine in &manifest.search_engines {
            if catalog
                .iter()
                .any(|candidate: &SearchEngine| candidate.id == engine.id)
            {
                continue;
            }
            catalog.push(SearchEngine {
                id: engine.id.clone(),
                name: engine.name.clone(),
                hosts: engine.hosts.clone(),
                query_url: engine.query_url.clone(),
            });
        }
    }
    let mut engine_recency = Vec::new();
    for engine in catalog {
        let mut latest = i64::MIN;
        for (metadata, _, visited) in &urls {
            if engine.matches_url(&metadata.url) {
                latest = latest.max(visited.0);
            }
        }
        engine_recency.push((engine, latest));
    }
    engine_recency.sort_by_key(|(_, visited)| Reverse(*visited));
    let mut search_engines = Vec::new();
    for (engine, _) in engine_recency {
        search_engines.push(engine);
    }
    if recent_files != snapshot.recent_files {
        snapshot.recent_files = recent_files;
    }
    if search_engines != snapshot.search_engines {
        snapshot.search_engines = search_engines;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use vmux_ecs::CreatedAt;

    #[test]
    fn work_dirs_list_open_pane_dir_contents() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("a.txt"), "").unwrap();
        fs::create_dir(root.path().join("sub")).unwrap();
        let cwd = root.path().to_string_lossy().to_string();

        let mut app = App::new();
        app.add_plugins(WorkSnapshotPlugin);
        app.world_mut().spawn(CommandBarWorkDirectory(cwd));
        app.update();

        let snap = app
            .world_mut()
            .query::<&CommandBarWorkSnapshot>()
            .single(app.world())
            .unwrap()
            .clone();
        assert!(
            snap.work_dirs
                .iter()
                .any(|e| e.path.ends_with("/a.txt") && !e.is_dir),
            "lists files in the work dir"
        );
        assert!(
            snap.work_dirs
                .iter()
                .any(|e| e.path.ends_with("/sub") && e.is_dir),
            "lists subdirs in the work dir"
        );
    }

    #[test]
    fn work_dirs_include_vmux_managed_worktree() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join(".vmux/worktrees/repo/task");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("changed.rs"), "").unwrap();
        let mut app = App::new();
        app.add_plugins(WorkSnapshotPlugin);
        app.world_mut()
            .spawn(CommandBarWorkDirectory(root.to_string_lossy().into_owned()));
        app.update();
        let snap = app
            .world_mut()
            .query::<&CommandBarWorkSnapshot>()
            .single(app.world())
            .unwrap()
            .clone();
        assert!(
            snap.work_dirs
                .iter()
                .any(|entry| entry.path.ends_with("/changed.rs")),
            "includes files from vmux-managed worktrees"
        );
    }

    #[test]
    fn work_dirs_list_acp_agent_cwd_contents() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("notes.md"), "").unwrap();
        let cwd = root.path().to_string_lossy().to_string();

        let mut app = App::new();
        app.add_plugins(WorkSnapshotPlugin);
        app.world_mut().spawn(CommandBarWorkDirectory(cwd));
        app.update();

        let snap = app
            .world_mut()
            .query::<&CommandBarWorkSnapshot>()
            .single(app.world())
            .unwrap()
            .clone();
        assert!(
            snap.work_dirs
                .iter()
                .any(|e| e.path.ends_with("/notes.md") && !e.is_dir),
            "lists files in the ACP agent's cwd"
        );
    }

    #[test]
    fn replaced_work_snapshot_entity_rebuilds_directories_and_recent_files() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("fresh.rs"), "").unwrap();
        let mut app = App::new();
        app.add_plugins(WorkSnapshotPlugin);
        app.world_mut().spawn(CommandBarWorkDirectory(
            root.path().to_string_lossy().into_owned(),
        ));
        app.world_mut().spawn((
            Url,
            PageMetadata {
                url: "file:///work/main.rs".into(),
                title: "main.rs".into(),
                ..default()
            },
            VisitCount(1),
            LastVisitedAt(1000),
            CreatedAt(0),
        ));
        app.update();

        let old = app
            .world_mut()
            .query_filtered::<Entity, With<CommandBarWorkSnapshot>>()
            .single(app.world())
            .unwrap();
        app.world_mut().despawn(old);
        app.world_mut().spawn((
            Name::new("Command bar work"),
            CommandBarWorkSnapshot::default(),
            WorkDirectoryCache::default(),
            RecentFilesState::default(),
        ));
        app.update();

        let snap = app
            .world_mut()
            .query::<&CommandBarWorkSnapshot>()
            .single(app.world())
            .unwrap()
            .clone();
        assert!(
            snap.work_dirs
                .iter()
                .any(|entry| entry.path.ends_with("/fresh.rs"))
        );
        assert_eq!(snap.recent_files.len(), 1);
        assert_eq!(snap.recent_files[0].title, "main.rs");
    }

    #[test]
    fn recent_files_only_file_urls_ranked() {
        let mut app = App::new();
        app.add_plugins(WorkSnapshotPlugin);
        app.world_mut().spawn((
            Url,
            PageMetadata {
                url: "https://example.com".into(),
                ..default()
            },
            VisitCount(9),
            LastVisitedAt(1000),
            CreatedAt(0),
        ));
        app.world_mut().spawn((
            Url,
            PageMetadata {
                url: "file:///work/main.rs".into(),
                title: "main.rs".into(),
                ..default()
            },
            VisitCount(1),
            LastVisitedAt(1000),
            CreatedAt(0),
        ));
        app.update();
        let snap = app
            .world_mut()
            .query::<&CommandBarWorkSnapshot>()
            .single(app.world())
            .unwrap()
            .clone();
        assert_eq!(snap.recent_files.len(), 1);
        assert_eq!(snap.recent_files[0].title, "main.rs");
    }

    #[test]
    fn search_engines_are_ordered_by_most_recent_visit() {
        let mut app = App::new();
        app.add_plugins(WorkSnapshotPlugin);
        app.world_mut().spawn(FeatureManifest::parse(
            r#"(
                search_engines: [
                    (id: "google", name: "Google", hosts: ["google.com"], query_url: "https://www.google.com/search?q={query}"),
                    (id: "bing", name: "Bing", hosts: ["bing.com"], query_url: "https://www.bing.com/search?q={query}"),
                    (id: "duckduckgo", name: "DuckDuckGo", hosts: ["duckduckgo.com"], query_url: "https://duckduckgo.com/?q={query}"),
                    (id: "brave", name: "Brave Search", hosts: ["search.brave.com"], query_url: "https://search.brave.com/search?q={query}"),
                    (id: "kagi", name: "Kagi", hosts: ["kagi.com"], query_url: "https://kagi.com/search?q={query}"),
                ],
            )"#,
        ));
        for (url, visited) in [
            ("https://www.google.com/search?q=old", 1000),
            ("https://kagi.com/search?q=new", 3000),
            ("https://search.brave.com/search?q=middle", 2000),
        ] {
            app.world_mut().spawn((
                Url,
                PageMetadata {
                    url: url.into(),
                    ..default()
                },
                VisitCount(1),
                LastVisitedAt(visited),
                CreatedAt(0),
            ));
        }
        app.update();

        let snapshot = app
            .world_mut()
            .query::<&CommandBarWorkSnapshot>()
            .single(app.world())
            .unwrap()
            .clone();
        let engines = &snapshot.search_engines;
        assert_eq!(engines.len(), 5);
        assert_eq!(
            engines[..3]
                .iter()
                .map(|engine| engine.id.as_str())
                .collect::<Vec<_>>(),
            ["kagi", "brave", "google"]
        );
    }
}
