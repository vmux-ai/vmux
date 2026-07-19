use bevy::prelude::*;
use vmux_command::event::{CommandBarRecentFile, CommandBarWorkDir};
use vmux_command::snapshot::CommandBarWorkSnapshot;
use vmux_core::terminal::{Terminal, TerminalLaunch};
use vmux_core::{LastVisitedAt, PageMetadata, Url, VisitCount};
use vmux_history::LastActivatedAt;

/// How many files/dirs (across all current work dirs) the work group lists.
const WORK_DIR_ENTRIES_CAP: usize = 40;
/// How many recent files the recent-files group lists.
const RECENT_FILES_CAP: usize = 20;

/// Frecency: visit count decayed by recency (mirrors `vmux_history`'s ranking;
/// inlined to avoid depending on that crate's module visibility).
fn frecency(visit_count: u32, last_visited_at: i64, now: i64) -> f32 {
    let age_hours = ((now - last_visited_at).max(0) as f32) / 3_600_000.0;
    let decay = 1.0 / (1.0 + age_hours / 24.0);
    (visit_count as f32) * decay
}

/// True when a `file://` url points at a directory on disk (browsed via the dir
/// view). The recent-*files* group excludes directories.
fn url_is_directory(url: &str) -> bool {
    url.strip_prefix("file://")
        .map(|p| std::path::Path::new(p).is_dir())
        .unwrap_or(false)
}

/// List the immediate children of `dir` as work entries (dirs first, then files;
/// hidden entries last; alphabetical within each group) for fast `file://` open.
fn list_dir_entries(dir: &str) -> Vec<CommandBarWorkDir> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut rows: Vec<(String, bool, String)> = read
        .flatten()
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let path = e.path().to_string_lossy().to_string();
            (name, is_dir, path)
        })
        .collect();
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

/// List the contents (files + dirs) of every current work dir — the cwd of an open
/// terminal/agent pane — so files can be opened via `file://` fast. Only re-reads the
/// filesystem when the set of work dirs changes (contents refresh on pane open/close
/// or restart), not every frame.
pub fn update_work_dirs_snapshot(
    terminals: Query<(&TerminalLaunch, Option<&LastActivatedAt>), With<Terminal>>,
    agent_dirs: Query<(&vmux_core::AgentWorkingDir, Option<&LastActivatedAt>)>,
    mut last_cwds: Local<Vec<String>>,
    mut snapshot: ResMut<CommandBarWorkSnapshot>,
) {
    let mut by_cwd: Vec<(String, i64)> = Vec::new();
    let merge = |cwd: &str, ts: i64, acc: &mut Vec<(String, i64)>| {
        if cwd.is_empty() {
            return;
        }
        if let Some(existing) = acc.iter_mut().find(|(p, _)| p == cwd) {
            existing.1 = existing.1.max(ts);
        } else {
            acc.push((cwd.to_string(), ts));
        }
    };
    for (launch, last) in &terminals {
        merge(&launch.cwd, last.map(|l| l.0).unwrap_or(0), &mut by_cwd);
    }
    // ACP (and other PTY-less agent) panes carry their cwd on `AgentWorkingDir`.
    for (dir, last) in &agent_dirs {
        merge(&dir.0, last.map(|l| l.0).unwrap_or(0), &mut by_cwd);
    }
    by_cwd.sort_by_key(|(_, ts)| std::cmp::Reverse(*ts));
    let cwds: Vec<String> = by_cwd.into_iter().map(|(p, _)| p).collect();
    if *last_cwds == cwds {
        return;
    }
    *last_cwds = cwds.clone();

    let mut entries: Vec<CommandBarWorkDir> = Vec::new();
    for cwd in &cwds {
        for entry in list_dir_entries(cwd) {
            if entries.iter().any(|e| e.path == entry.path) {
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
    if entries != snapshot.work_dirs {
        snapshot.work_dirs = entries;
    }
}

/// Rebuild the recent-files list: top-N `file://` history urls by frecency. Recomputes
/// only when a visit was added or a url's last-visited time changed.
pub fn update_recent_files_snapshot(
    changed: Query<(), Or<(Added<Url>, Changed<LastVisitedAt>)>>,
    urls: Query<(&PageMetadata, &VisitCount, &LastVisitedAt), With<Url>>,
    mut initialized: Local<bool>,
    mut snapshot: ResMut<CommandBarWorkSnapshot>,
) {
    if *initialized && changed.is_empty() {
        return;
    }
    *initialized = true;
    let now = vmux_core::now_millis();
    let mut scored: Vec<(f32, CommandBarRecentFile)> = urls
        .iter()
        .filter(|(meta, _, _)| meta.url.starts_with("file://") && !url_is_directory(&meta.url))
        .map(|(meta, count, last)| {
            (
                frecency(count.0, last.0, now),
                CommandBarRecentFile {
                    url: meta.url.clone(),
                    title: meta.title.clone(),
                },
            )
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let recent_files: Vec<CommandBarRecentFile> = scored
        .into_iter()
        .take(RECENT_FILES_CAP)
        .map(|(_, f)| f)
        .collect();
    if recent_files != snapshot.recent_files {
        snapshot.recent_files = recent_files;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::terminal::TerminalKind;

    fn launch(cwd: &str, kind: TerminalKind) -> TerminalLaunch {
        TerminalLaunch {
            command: "/bin/zsh".into(),
            args: vec![],
            cwd: cwd.into(),
            env: vec![],
            kind,
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
        app.init_resource::<CommandBarWorkSnapshot>()
            .add_systems(Update, update_work_dirs_snapshot);
        app.world_mut()
            .spawn((Terminal, launch(&cwd, TerminalKind::Plain)));
        app.update();

        let snap = app.world().resource::<CommandBarWorkSnapshot>();
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
        app.init_resource::<CommandBarWorkSnapshot>()
            .add_systems(Update, update_work_dirs_snapshot);
        app.world_mut().spawn((
            Terminal,
            launch(&root.to_string_lossy(), TerminalKind::Plain),
        ));
        app.update();
        let snap = app.world().resource::<CommandBarWorkSnapshot>();
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
        app.init_resource::<CommandBarWorkSnapshot>()
            .add_systems(Update, update_work_dirs_snapshot);
        app.world_mut()
            .spawn(vmux_core::AgentWorkingDir(cwd.clone()));
        app.update();

        let snap = app.world().resource::<CommandBarWorkSnapshot>();
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
        use vmux_core::CreatedAt;
        let mut app = App::new();
        app.init_resource::<CommandBarWorkSnapshot>()
            .add_systems(Update, update_recent_files_snapshot);
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
        let snap = app.world().resource::<CommandBarWorkSnapshot>();
        assert_eq!(snap.recent_files.len(), 1);
        assert_eq!(snap.recent_files[0].title, "main.rs");
    }
}
