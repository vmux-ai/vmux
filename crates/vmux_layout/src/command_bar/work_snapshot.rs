use bevy::prelude::*;
use vmux_command::event::{CommandBarRecentFile, CommandBarWorkDir};
use vmux_command::snapshot::CommandBarWorkSnapshot;
use vmux_core::terminal::{Terminal, TerminalKind, TerminalLaunch};
use vmux_core::{LastVisitedAt, PageMetadata, Url, VisitCount};
use vmux_history::LastActivatedAt;

/// How many entries each work-section group carries in the payload.
const WORK_GROUP_CAP: usize = 8;

/// Frecency: visit count decayed by recency (mirrors `vmux_history`'s ranking;
/// inlined to avoid depending on that crate's module visibility).
fn frecency(visit_count: u32, last_visited_at: i64, now: i64) -> f32 {
    let age_hours = ((now - last_visited_at).max(0) as f32) / 3_600_000.0;
    let decay = 1.0 / (1.0 + age_hours / 24.0);
    (visit_count as f32) * decay
}

fn kind_label(kind: &TerminalKind) -> &'static str {
    match kind {
        TerminalKind::Plain => "Terminal",
        TerminalKind::Vibe => "Vibe",
        TerminalKind::Claude => "Claude",
        TerminalKind::Codex => "Codex",
    }
}

/// Rebuild the open-pane working-dir list from every open terminal/agent (`Terminal`
/// entities carry `TerminalLaunch`), deduped by cwd, most-recently-active first.
pub fn update_work_dirs_snapshot(
    terminals: Query<(&TerminalLaunch, Option<&LastActivatedAt>), With<Terminal>>,
    mut snapshot: ResMut<CommandBarWorkSnapshot>,
) {
    let mut by_cwd: Vec<(String, &'static str, i64)> = Vec::new();
    for (launch, last) in &terminals {
        if launch.cwd.is_empty() {
            continue;
        }
        let ts = last.map(|l| l.0).unwrap_or(0);
        if let Some(existing) = by_cwd.iter_mut().find(|(p, _, _)| *p == launch.cwd) {
            if ts > existing.2 {
                existing.1 = kind_label(&launch.kind);
                existing.2 = ts;
            }
        } else {
            by_cwd.push((launch.cwd.clone(), kind_label(&launch.kind), ts));
        }
    }
    by_cwd.sort_by(|a, b| b.2.cmp(&a.2));
    let work_dirs: Vec<CommandBarWorkDir> = by_cwd
        .into_iter()
        .take(WORK_GROUP_CAP)
        .map(|(path, kind_label, _)| CommandBarWorkDir {
            path,
            kind_label: kind_label.to_string(),
        })
        .collect();
    if work_dirs != snapshot.work_dirs {
        snapshot.work_dirs = work_dirs;
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
        .filter(|(meta, _, _)| meta.url.starts_with("file://"))
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
        .take(WORK_GROUP_CAP)
        .map(|(_, f)| f)
        .collect();
    if recent_files != snapshot.recent_files {
        snapshot.recent_files = recent_files;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn work_dirs_dedupe_by_cwd() {
        let mut app = App::new();
        app.init_resource::<CommandBarWorkSnapshot>()
            .add_systems(Update, update_work_dirs_snapshot);
        app.world_mut()
            .spawn((Terminal, launch("/work/a", TerminalKind::Plain)));
        app.world_mut()
            .spawn((Terminal, launch("/work/a", TerminalKind::Vibe)));
        app.world_mut()
            .spawn((Terminal, launch("/work/b", TerminalKind::Plain)));
        app.update();
        let snap = app.world().resource::<CommandBarWorkSnapshot>();
        assert_eq!(snap.work_dirs.len(), 2);
        assert!(snap.work_dirs.iter().any(|d| d.path == "/work/a"));
        assert!(snap.work_dirs.iter().any(|d| d.path == "/work/b"));
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
