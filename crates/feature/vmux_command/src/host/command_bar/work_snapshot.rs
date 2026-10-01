use crate::snapshot::{CommandBarProjection, CommandBarWorkDirectory};
use bevy::prelude::*;
use vmux_api::command_bar::{CommandBarRecentFile, CommandBarWorkDir, SearchEngine};
use vmux_ecs::{LastVisitedAt, PageMetadata, Url, VisitCount};
use vmux_history::LastActivatedAt;

const WORK_DIR_ENTRIES_CAP: usize = 40;
const RECENT_FILES_CAP: usize = 20;

pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (directories, recent).in_set(crate::snapshot::WriteCommandBarSnapshots),
        );
    }
}

struct WorkDirectories(Vec<(String, i64)>);

impl WorkDirectories {
    fn add(&mut self, path: &str, activated_at: i64) {
        if path.is_empty() {
            return;
        }
        if let Some(existing) = self.0.iter_mut().find(|(candidate, _)| candidate == path) {
            existing.1 = existing.1.max(activated_at);
            return;
        }
        self.0.push((path.to_string(), activated_at));
    }

    fn paths(mut self) -> Vec<String> {
        self.0
            .sort_by_key(|(_, activated_at)| std::cmp::Reverse(*activated_at));
        self.0.into_iter().map(|(path, _)| path).collect()
    }
}

impl CommandBarWorkDirectory {
    fn entries(&self) -> Vec<CommandBarWorkDir> {
        let Ok(read) = std::fs::read_dir(&self.0) else {
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

struct RecentFile {
    score: f32,
    value: CommandBarRecentFile,
}

impl RecentFile {
    fn from_page(
        metadata: &PageMetadata,
        visit_count: VisitCount,
        last_visited_at: LastVisitedAt,
        now: i64,
    ) -> Option<Self> {
        let path = metadata.url.strip_prefix("file://")?;
        if std::path::Path::new(path).is_dir() {
            return None;
        }
        let age_hours = ((now - last_visited_at.0).max(0) as f32) / 3_600_000.0;
        let decay = 1.0 / (1.0 + age_hours / 24.0);
        Some(Self {
            score: (visit_count.0 as f32) * decay,
            value: CommandBarRecentFile {
                url: metadata.url.clone(),
                title: metadata.title.clone(),
            },
        })
    }
}

fn directories(
    directories: Query<(&CommandBarWorkDirectory, Option<&LastActivatedAt>)>,
    mut last_cwds: Local<Vec<String>>,
    mut state: Single<&mut CommandBarProjection>,
) {
    let mut current = WorkDirectories(Vec::new());
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
    if entries != state.work.work_dirs {
        state.work.work_dirs = entries;
    }
}

fn recent(
    changed: Query<(), Or<(Added<Url>, Changed<LastVisitedAt>)>>,
    urls: Query<(&PageMetadata, &VisitCount, &LastVisitedAt), With<Url>>,
    mut initialized: Local<bool>,
    mut state: Single<&mut CommandBarProjection>,
) {
    if *initialized && changed.is_empty() {
        return;
    }
    *initialized = true;
    let now = vmux_ecs::now_millis();
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

    let mut engine_recency = Vec::new();
    for engine in SearchEngine::ALL {
        let mut latest = i64::MIN;
        for (metadata, _, visited) in &urls {
            if SearchEngine::from_url(&metadata.url) == Some(engine) {
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
    if recent_files != state.work.recent_files {
        state.work.recent_files = recent_files;
    }
    if search_engines != state.work.search_engines {
        state.work.search_engines = search_engines;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl CommandBarProjection {
        fn read(app: &mut App) -> Self {
            let world = app.world_mut();
            let mut query = world.query::<&Self>();
            query.single(world).unwrap().clone()
        }
    }

    #[test]
    fn work_dirs_list_open_pane_dir_contents() {
        use std::fs;
        let root = std::env::temp_dir().join(format!("vmux-work-contents-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        let cwd = root.to_string_lossy().to_string();

        let mut app = App::new();
        app.add_plugins(Plugin);
        app.world_mut().spawn(CommandBarProjection::default());
        app.world_mut().spawn(CommandBarWorkDirectory(cwd));
        app.update();

        let snap = CommandBarProjection::read(&mut app).work;
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
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn work_dirs_include_vmux_managed_worktree() {
        let base = std::env::temp_dir().join(format!("vmux-worktree-{}", std::process::id()));
        let root = base.join(".vmux/worktrees/repo/task");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("changed.rs"), "").unwrap();
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.world_mut().spawn(CommandBarProjection::default());
        app.world_mut()
            .spawn(CommandBarWorkDirectory(root.to_string_lossy().into_owned()));
        app.update();
        let snap = CommandBarProjection::read(&mut app).work;
        assert!(
            snap.work_dirs
                .iter()
                .any(|entry| entry.path.ends_with("/changed.rs")),
            "includes files from vmux-managed worktrees"
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn work_dirs_list_acp_agent_cwd_contents() {
        use std::fs;
        let root = std::env::temp_dir().join(format!("vmux-acp-work-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("notes.md"), "").unwrap();
        let cwd = root.to_string_lossy().to_string();

        let mut app = App::new();
        app.add_plugins(Plugin);
        app.world_mut().spawn(CommandBarProjection::default());
        app.world_mut().spawn(CommandBarWorkDirectory(cwd.clone()));
        app.update();

        let snap = CommandBarProjection::read(&mut app).work;
        assert!(
            snap.work_dirs
                .iter()
                .any(|e| e.path.ends_with("/notes.md") && !e.is_dir),
            "lists files in the ACP agent's cwd"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn recent_files_only_file_urls_ranked() {
        use vmux_ecs::CreatedAt;
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.world_mut().spawn(CommandBarProjection::default());
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
        let snap = CommandBarProjection::read(&mut app).work;
        assert_eq!(snap.recent_files.len(), 1);
        assert_eq!(snap.recent_files[0].title, "main.rs");
    }

    #[test]
    fn search_engines_are_ordered_by_most_recent_visit() {
        use vmux_ecs::CreatedAt;
        let mut app = App::new();
        app.add_plugins(Plugin);
        app.world_mut().spawn(CommandBarProjection::default());
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

        let snapshot = CommandBarProjection::read(&mut app);
        let engines = &snapshot.work.search_engines;
        assert_eq!(engines.len(), SearchEngine::ALL.len());
        assert_eq!(
            &engines[..3],
            &[
                SearchEngine::Kagi,
                SearchEngine::Brave,
                SearchEngine::Google
            ]
        );
    }
}
