use crate::host::snapshot::{CommandBarWorkDirectory, CommandBarWorkSnapshot};
use bevy::prelude::*;
use vmux_api::command_bar::{CommandBarWorkDir, SearchEngine};
use vmux_ecs::manifest::FeatureManifest;
use vmux_ecs::{LastActivatedAt, LastVisitedAt, PageMetadata, Url, VisitCount};

use super::work_snapshot_driver::{RecentFile, WorkDirectories};

const WORK_DIR_ENTRIES_CAP: usize = 40;
const RECENT_FILES_CAP: usize = 20;
type RecentPageChange = Or<(Added<Url>, Changed<LastVisitedAt>)>;

pub(super) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(
            Update,
            (directories, recent).in_set(crate::host::snapshot::WriteCommandBarSnapshots),
        );
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Command bar work"),
        CommandBarWorkSnapshot::default(),
    ));
}

fn directories(
    directories: Query<(&CommandBarWorkDirectory, Option<&LastActivatedAt>)>,
    mut last_cwds: Local<Vec<String>>,
    mut state: Single<&mut CommandBarWorkSnapshot>,
) {
    let mut current = WorkDirectories::default();
    for (directory, activated_at) in &directories {
        current.add(
            &directory.0,
            activated_at.map(|activated_at| activated_at.0).unwrap_or(0),
        );
    }
    let cwds = current.paths();
    if *last_cwds == cwds {
        return;
    }
    *last_cwds = cwds.clone();

    let mut entries = Vec::new();
    for cwd in &cwds {
        for entry in CommandBarWorkDirectory(cwd.clone()).entries() {
            if entries
                .iter()
                .any(|existing: &CommandBarWorkDir| existing.path == entry.path)
            {
                continue;
            }
            entries.push(entry);
            if entries.len() >= WORK_DIR_ENTRIES_CAP {
                break;
            }
        }
        if entries.len() >= WORK_DIR_ENTRIES_CAP {
            break;
        }
    }
    if entries != state.work_dirs {
        state.work_dirs = entries;
    }
}

fn recent(
    changed: Query<(), RecentPageChange>,
    urls: Query<(&PageMetadata, &VisitCount, &LastVisitedAt), With<Url>>,
    manifests: Query<Ref<FeatureManifest>>,
    mut initialized: Local<bool>,
    mut state: Single<&mut CommandBarWorkSnapshot>,
) {
    let catalog_changed = manifests
        .iter()
        .any(|manifest| manifest.is_added() || manifest.is_changed());
    if *initialized && changed.is_empty() && !catalog_changed {
        return;
    }
    *initialized = true;
    let now = vmux_ecs::UnixMillis::now().0;
    let mut scored = Vec::new();
    for (metadata, visit_count, last_visited_at) in &urls {
        if let Some(file) = RecentFile::from_page(metadata, *visit_count, *last_visited_at, now) {
            scored.push(file);
        }
    }
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut recent_files = Vec::new();
    for file in scored.into_iter().take(RECENT_FILES_CAP) {
        recent_files.push(file.value);
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
    engine_recency.sort_by_key(|(_, visited)| std::cmp::Reverse(*visited));
    let mut search_engines = Vec::new();
    for (engine, _) in engine_recency {
        search_engines.push(engine);
    }
    if recent_files != state.recent_files {
        state.recent_files = recent_files;
    }
    if search_engines != state.search_engines {
        state.search_engines = search_engines;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use vmux_ecs::CreatedAt;

    use super::*;

    struct WorkSnapshot;

    impl WorkSnapshot {
        fn read(app: &mut App) -> CommandBarWorkSnapshot {
            let world = app.world_mut();
            let mut query = world.query::<&CommandBarWorkSnapshot>();
            query.single(world).unwrap().clone()
        }

        fn search_manifest() -> FeatureManifest {
            FeatureManifest::parse(
                r#"(
                    search_engines: [
                        (id: "google", name: "Google", hosts: ["google.com"], query_url: "https://www.google.com/search?q={query}"),
                        (id: "bing", name: "Bing", hosts: ["bing.com"], query_url: "https://www.bing.com/search?q={query}"),
                        (id: "duckduckgo", name: "DuckDuckGo", hosts: ["duckduckgo.com"], query_url: "https://duckduckgo.com/?q={query}"),
                        (id: "brave", name: "Brave Search", hosts: ["search.brave.com"], query_url: "https://search.brave.com/search?q={query}"),
                        (id: "kagi", name: "Kagi", hosts: ["kagi.com"], query_url: "https://kagi.com/search?q={query}"),
                    ],
                )"#,
            )
        }
    }

    #[test]
    fn work_dirs_list_open_pane_dir_contents() {
        let root = std::env::temp_dir().join(format!("vmux-work-contents-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        let cwd = root.to_string_lossy().to_string();

        let mut app = App::new();
        app.add_plugins(Plugin);
        app.world_mut().spawn(CommandBarWorkDirectory(cwd));
        app.update();

        let snap = WorkSnapshot::read(&mut app);
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
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn work_dirs_include_vmux_managed_worktree() {
        let base = std::env::temp_dir().join(format!("vmux-worktree-{}", std::process::id()));
        let root = base.join(".vmux/worktrees/repo/task");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("changed.rs"), "").unwrap();
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.world_mut()
            .spawn(CommandBarWorkDirectory(root.to_string_lossy().into_owned()));
        app.update();
        let snap = WorkSnapshot::read(&mut app);
        assert!(
            snap.work_dirs
                .iter()
                .any(|entry| entry.path.ends_with("/changed.rs")),
            "includes files from vmux-managed worktrees"
        );
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn work_dirs_list_acp_agent_cwd_contents() {
        let root = std::env::temp_dir().join(format!("vmux-acp-work-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("notes.md"), "").unwrap();
        let cwd = root.to_string_lossy().to_string();

        let mut app = App::new();
        app.add_plugins(Plugin);
        app.world_mut().spawn(CommandBarWorkDirectory(cwd.clone()));
        app.update();

        let snap = WorkSnapshot::read(&mut app);
        assert!(
            snap.work_dirs
                .iter()
                .any(|e| e.path.ends_with("/notes.md") && !e.is_dir),
            "lists files in the ACP agent's cwd"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn recent_files_only_file_urls_ranked() {
        let mut app = App::new();
        app.add_plugins(Plugin);
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
        let snap = WorkSnapshot::read(&mut app);
        assert_eq!(snap.recent_files.len(), 1);
        assert_eq!(snap.recent_files[0].title, "main.rs");
    }

    #[test]
    fn search_engines_are_ordered_by_most_recent_visit() {
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.world_mut().spawn(WorkSnapshot::search_manifest());
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

        let snapshot = WorkSnapshot::read(&mut app);
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
