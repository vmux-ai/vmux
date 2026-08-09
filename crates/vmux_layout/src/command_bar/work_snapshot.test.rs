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

#[test]
fn search_engines_are_ordered_by_most_recent_visit() {
    use vmux_core::CreatedAt;
    let mut app = App::new();
    app.init_resource::<CommandBarWorkSnapshot>()
        .add_systems(Update, update_recent_files_snapshot);
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

    let engines = &app
        .world()
        .resource::<CommandBarWorkSnapshot>()
        .search_engines;
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
