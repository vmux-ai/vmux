use super::*;
use std::cell::Cell;
use std::collections::HashMap;
use std::process::Command;

#[derive(Resource)]
struct ObservationInput {
    tab: Entity,
    path: PathBuf,
}

#[derive(Resource, Default)]
struct CapturedStartupDir(Option<String>);

fn emit_observation(
    input: Res<ObservationInput>,
    mut observations: MessageWriter<TabDirectoryObserved>,
) {
    observations.write(TabDirectoryObserved {
        tab: input.tab,
        path: input.path.clone(),
        kind: TabDirectoryObservationKind::Read,
    });
}

fn capture_startup_dir(
    input: Res<ObservationInput>,
    tabs: Query<&Tab>,
    mut captured: ResMut<CapturedStartupDir>,
) {
    captured.0 = tabs.get(input.tab).unwrap().startup_dir.clone();
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    git(p, &["init", "-q", "-b", "main"]);
    git(p, &["config", "user.email", "t@example.com"]);
    git(p, &["config", "user.name", "Test"]);
    git(p, &["config", "commit.gpgsign", "false"]);
    std::fs::write(p.join("seed.txt"), "seed\n").unwrap();
    git(p, &["add", "seed.txt"]);
    git(p, &["commit", "-qm", "init"]);
    dir
}

fn observe(app: &mut App, tab: Entity, path: &Path) {
    observe_with_kind(app, tab, path, TabDirectoryObservationKind::Read);
}

fn observe_edit(app: &mut App, tab: Entity, path: &Path) {
    observe_with_kind(app, tab, path, TabDirectoryObservationKind::Edit);
}

fn observe_with_kind(app: &mut App, tab: Entity, path: &Path, kind: TabDirectoryObservationKind) {
    app.world_mut()
        .resource_mut::<Messages<TabDirectoryObserved>>()
        .write(TabDirectoryObserved {
            tab,
            path: path.to_path_buf(),
            kind,
        });
    app.update();
}

#[test]
fn sanitize_slug_normalizes() {
    assert_eq!(sanitize_slug("Auth Refactor!"), "auth-refactor");
    assert_eq!(sanitize_slug("  a//b  "), "a-b");
    assert_eq!(sanitize_slug("***"), "task");
    assert_eq!(sanitize_slug(""), "task");
}

#[test]
fn create_worktree_blocking_uses_repository_hashed_global_root() {
    let repo = init_repo();
    let managed_root = tempfile::tempdir().unwrap();
    let activation =
        create_worktree_blocking(repo.path(), "Auth Refactor", managed_root.path()).unwrap();
    let checkout_dir = PathBuf::from(&activation.metadata.checkout_dir);
    let managed_root = managed_root.path().canonicalize().unwrap();
    assert_eq!(activation.metadata.branch, "vmux/auth-refactor");
    assert!(checkout_dir.is_dir());
    assert!(
        checkout_dir.starts_with(&managed_root)
            && checkout_dir.ends_with("auth-refactor")
            && checkout_dir
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.rsplit_once('-')
                        .is_some_and(|(_, hash)| hash.len() == 12)
                }),
        "path is <managed-root>/<repo-hash>/auth-refactor: {checkout_dir:?}"
    );
    assert_eq!(activation.execution_dir, checkout_dir);
}

#[test]
fn create_worktree_for_branch_uses_exact_valid_branch() {
    let repo = init_repo();
    let managed_root = tempfile::tempdir().unwrap();

    let activation = create_worktree_for_branch_blocking(
        repo.path(),
        "vmux/fix-dashboard-tests",
        managed_root.path(),
    )
    .unwrap();

    assert_eq!(activation.metadata.branch, "vmux/fix-dashboard-tests");
    assert!(activation.execution_dir.ends_with("fix-dashboard-tests"));
    assert_eq!(
        worktree::head_ref(&activation.execution_dir).unwrap(),
        "vmux/fix-dashboard-tests"
    );
}

#[test]
fn create_worktree_for_branch_initializes_unborn_repository() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q", "-b", "main"]);
    git(repo.path(), &["config", "user.email", "t@example.com"]);
    git(repo.path(), &["config", "user.name", "Test"]);
    git(repo.path(), &["config", "commit.gpgsign", "false"]);
    let managed_root = tempfile::tempdir().unwrap();

    let activation = create_worktree_for_branch_blocking(
        repo.path(),
        "feat/izakaya-website",
        managed_root.path(),
    )
    .unwrap();

    assert_eq!(activation.metadata.base_ref, "main");
    assert_eq!(
        worktree::head_ref(&activation.execution_dir).unwrap(),
        "feat/izakaya-website"
    );
    assert_eq!(worktree::head_ref(repo.path()).unwrap(), "main");
}

#[test]
fn create_worktree_from_repository_marked_bare() {
    let repo = init_repo();
    git(repo.path(), &["config", "core.bare", "true"]);
    let managed_root = tempfile::tempdir().unwrap();

    let activation =
        create_worktree_for_branch_blocking(repo.path(), "vmux/bare-source", managed_root.path())
            .unwrap();

    assert_eq!(activation.metadata.branch, "vmux/bare-source");
    assert_eq!(
        worktree::head_ref(&activation.execution_dir).unwrap(),
        "vmux/bare-source"
    );
}

#[test]
fn create_worktree_for_branch_rejects_existing_or_invalid_branch() {
    let repo = init_repo();
    let managed_root = tempfile::tempdir().unwrap();
    git(repo.path(), &["branch", "vmux/existing"]);

    let existing =
        create_worktree_for_branch_blocking(repo.path(), "vmux/existing", managed_root.path())
            .unwrap_err();
    let invalid =
        create_worktree_for_branch_blocking(repo.path(), "bad branch", managed_root.path())
            .unwrap_err();

    assert!(existing.contains("already exists"));
    assert!(!invalid.is_empty());
}

#[test]
fn generated_tab_names_use_project_name_as_slug_hint() {
    assert_eq!(
        tab_worktree_slug_hint("Tab 2", Path::new("/repo/dashboard")),
        "dashboard"
    );
    assert_eq!(
        tab_worktree_slug_hint("Auth Refactor", Path::new("/repo/dashboard")),
        "Auth Refactor"
    );
}

#[test]
fn create_worktree_preserves_nested_project_directory() {
    let repo = init_repo();
    let nested = repo.path().join("crates/app");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("main.rs"), "fn main() {}\n").unwrap();
    git(repo.path(), &["add", "crates/app/main.rs"]);
    git(repo.path(), &["commit", "-qm", "nested project"]);
    let managed_root = tempfile::tempdir().unwrap();

    let activation = create_worktree_blocking(&nested, "nested", managed_root.path()).unwrap();

    assert!(activation.execution_dir.ends_with("nested/crates/app"));
    assert!(activation.execution_dir.join("main.rs").is_file());
}

#[test]
fn plan_worktree_skips_existing_branch_name() {
    let repo = init_repo();
    let managed_root = tempfile::tempdir().unwrap();
    git(repo.path(), &["branch", "vmux/feat"]);
    let checkout = worktree::checkout_info(repo.path()).unwrap();
    let (path, branch) = plan_worktree(&checkout, managed_root.path(), "feat");
    assert_eq!(branch, "vmux/feat-2");
    assert!(path.starts_with(managed_root.path()));
    assert!(path.ends_with("feat-2"), "{path:?}");
}

#[test]
fn reconcile_recovers_missing_worktree_without_dropping_metadata() {
    let repo = init_repo();
    let managed_root = tempfile::tempdir().unwrap();
    let activation = create_worktree_blocking(repo.path(), "recover", managed_root.path()).unwrap();
    let checkout_dir = PathBuf::from(&activation.metadata.checkout_dir);
    std::fs::remove_dir_all(&checkout_dir).unwrap();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn((
            Tab {
                name: "recover".into(),
                startup_dir: Some(activation.execution_dir.to_string_lossy().into_owned()),
            },
            TabWorkspace {
                project_dir: repo.path().to_string_lossy().into_owned(),
            },
            activation.metadata,
        ))
        .id();

    app.update();

    assert!(checkout_dir.is_dir());
    assert!(app.world().get::<TabWorktree>(tab).is_some());
    assert!(app.world().get::<TabWorktreeUnavailable>(tab).is_none());
}

#[test]
fn recovery_recreates_pruned_managed_registration() {
    let repo = init_repo();
    let managed_root = tempfile::tempdir().unwrap();
    let activation = create_worktree_blocking(repo.path(), "recover", managed_root.path()).unwrap();
    std::fs::remove_dir_all(&activation.metadata.checkout_dir).unwrap();
    git(repo.path(), &["worktree", "prune", "--expire", "now"]);
    let tab = Tab {
        name: "recover".into(),
        startup_dir: Some(activation.execution_dir.to_string_lossy().into_owned()),
    };
    let workspace = TabWorkspace {
        project_dir: repo.path().to_string_lossy().into_owned(),
    };

    let recovered =
        ensure_tab_worktree_available(&tab, &workspace, &activation.metadata, managed_root.path())
            .unwrap();

    assert!(recovered.execution_dir.is_dir());
    assert_eq!(
        worktree::head_ref(&recovered.execution_dir).unwrap(),
        activation.metadata.branch
    );
}

#[test]
fn reconcile_keeps_metadata_when_recovery_fails() {
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn((
            Tab {
                name: "missing".into(),
                startup_dir: Some("/no/such/vmux-worktree".into()),
            },
            TabWorkspace {
                project_dir: "/no/such/vmux-project".into(),
            },
            TabWorktree {
                repo_root: "/no/such/vmux-project".into(),
                checkout_dir: "/no/such/vmux-worktree".into(),
                branch: "vmux/missing".into(),
                base_ref: "main".into(),
            },
        ))
        .id();

    app.update();

    assert!(app.world().get::<TabWorktree>(tab).is_some());
    assert!(app.world().get::<TabWorktreeUnavailable>(tab).is_some());
}

#[test]
fn recovery_rejects_unregistered_path_outside_managed_root() {
    let repo = init_repo();
    let managed_root = tempfile::tempdir().unwrap();
    let activation = create_worktree_blocking(repo.path(), "managed", managed_root.path()).unwrap();
    let outside_parent = tempfile::tempdir().unwrap();
    let outside = outside_parent.path().join("escape");
    let mut metadata = activation.metadata;
    metadata.checkout_dir = outside.to_string_lossy().into_owned();
    let tab = Tab {
        name: "managed".into(),
        startup_dir: Some(outside.to_string_lossy().into_owned()),
    };
    let workspace = TabWorkspace {
        project_dir: repo.path().to_string_lossy().into_owned(),
    };

    let error = ensure_tab_worktree_available(&tab, &workspace, &metadata, managed_root.path())
        .unwrap_err();

    assert!(error.contains("repository storage directory"));
    assert!(!outside.exists());
}

#[cfg(unix)]
#[test]
fn managed_project_directory_cannot_escape_through_symlink() {
    use std::os::unix::fs::symlink;

    let repo = init_repo();
    let nested = repo.path().join("crates/app");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("main.rs"), "fn main() {}\n").unwrap();
    git(repo.path(), &["add", "crates/app/main.rs"]);
    git(repo.path(), &["commit", "-qm", "nested project"]);
    let managed_root = tempfile::tempdir().unwrap();
    let activation = create_worktree_blocking(&nested, "managed", managed_root.path()).unwrap();
    std::fs::remove_dir_all(&activation.execution_dir).unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), &activation.execution_dir).unwrap();
    let tab = Tab {
        name: "managed".into(),
        startup_dir: Some(activation.execution_dir.to_string_lossy().into_owned()),
    };
    let workspace = TabWorkspace {
        project_dir: nested.to_string_lossy().into_owned(),
    };

    let error =
        ensure_tab_worktree_available(&tab, &workspace, &activation.metadata, managed_root.path())
            .unwrap_err();

    assert!(error.contains("escapes worktree"));
}

#[test]
fn restore_reconciles_at_most_one_worktree_per_frame() {
    let repo = init_repo();
    let managed_root = tempfile::tempdir().unwrap();
    let first = create_worktree_blocking(repo.path(), "first", managed_root.path()).unwrap();
    let second = create_worktree_blocking(repo.path(), "second", managed_root.path()).unwrap();
    std::fs::remove_dir_all(&first.metadata.checkout_dir).unwrap();
    std::fs::remove_dir_all(&second.metadata.checkout_dir).unwrap();
    let mut app = App::new();
    app.insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()))
        .add_plugins(WorktreePlugin);
    for activation in [first, second] {
        app.world_mut().spawn((
            Tab {
                name: "restore".into(),
                startup_dir: Some(activation.execution_dir.to_string_lossy().into_owned()),
            },
            TabWorkspace {
                project_dir: repo.path().to_string_lossy().into_owned(),
            },
            activation.metadata,
        ));
    }

    app.update();
    assert_eq!(
        app.world()
            .iter_entities()
            .filter(|entity| entity.contains::<TabWorktreeReady>())
            .count(),
        1
    );

    app.update();
    assert_eq!(
        app.world()
            .iter_entities()
            .filter(|entity| entity.contains::<TabWorktreeReady>())
            .count(),
        2
    );
}

#[test]
fn observation_rebinds_managed_tab_to_same_repo_checkout() {
    let repo = init_repo();
    let managed_root = tempfile::tempdir().unwrap();
    let managed = create_worktree_blocking(repo.path(), "managed", managed_root.path()).unwrap();
    let touched = repo.path().join("seed.txt");
    let expected = repo
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn((
            Tab {
                name: "tab".into(),
                startup_dir: Some(managed.execution_dir.to_string_lossy().into_owned()),
            },
            TabWorkspace {
                project_dir: repo.path().to_string_lossy().into_owned(),
            },
            managed.metadata.clone(),
        ))
        .id();

    observe_edit(&mut app, tab, &touched);

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(expected.as_str())
    );
    assert!(app.world().get::<TabWorktree>(tab).is_none());
    assert!(
        Path::new(&managed.metadata.checkout_dir).is_dir(),
        "old checkout is preserved"
    );
}

#[test]
fn observation_rebinds_before_same_frame_consumers() {
    let repo = init_repo();
    let managed_root = tempfile::tempdir().unwrap();
    let managed = create_worktree_blocking(repo.path(), "managed", managed_root.path()).unwrap();
    let expected = repo
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin)
        .init_resource::<CapturedStartupDir>();
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(managed.execution_dir.to_string_lossy().into_owned()),
        })
        .id();
    app.insert_resource(ObservationInput {
        tab,
        path: repo.path().join("seed.txt"),
    })
    .add_systems(Update, emit_observation.before(TabDirectoryRebindSet))
    .add_systems(Update, capture_startup_dir.after(TabDirectoryRebindSet));

    app.update();

    assert_eq!(
        app.world().resource::<CapturedStartupDir>().0.as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn observation_rebinds_repeatedly_within_same_repo() {
    let repo = init_repo();
    let managed_root = tempfile::tempdir().unwrap();
    let first = create_worktree_blocking(repo.path(), "first", managed_root.path()).unwrap();
    let second_path = repo.path().join(".worktrees/second");
    worktree::worktree_add(repo.path(), &second_path, "vmux/second", "main").unwrap();
    let second_file = second_path.join("seed.txt");
    let main_file = repo.path().join("seed.txt");
    let second_expected = second_path
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let main_expected = repo
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(first.execution_dir.to_string_lossy().into_owned()),
        })
        .id();

    observe(&mut app, tab, &second_file);
    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(second_expected.as_str())
    );

    observe(&mut app, tab, &main_file);
    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(main_expected.as_str())
    );
}

#[test]
fn observation_keeps_same_checkout_directory() {
    let repo = init_repo();
    let original = repo
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(original.clone()),
        })
        .id();

    observe(&mut app, tab, &repo.path().join("seed.txt"));

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(original.as_str())
    );
}

#[test]
fn observation_rebinds_from_main_checkout_to_nested_linked_worktree() {
    let repo = init_repo();
    let linked_path = repo.path().join(".worktrees/linked");
    worktree::worktree_add(repo.path(), &linked_path, "vmux/linked", "main").unwrap();
    let expected = linked_path
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(repo.path().to_string_lossy().into_owned()),
        })
        .id();

    observe(&mut app, tab, &linked_path.join("seed.txt"));

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn observation_ignores_unrelated_repo_nested_inside_checkout() {
    let repo = init_repo();
    let nested = repo.path().join("vendor/nested");
    std::fs::create_dir_all(&nested).unwrap();
    git(&nested, &["init", "-q", "-b", "main"]);
    git(&nested, &["config", "user.email", "t@example.com"]);
    git(&nested, &["config", "user.name", "Test"]);
    git(&nested, &["config", "commit.gpgsign", "false"]);
    std::fs::write(nested.join("nested.txt"), "nested\n").unwrap();
    git(&nested, &["add", "nested.txt"]);
    git(&nested, &["commit", "-qm", "init"]);
    let original = repo.path().to_string_lossy().into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(original.clone()),
        })
        .id();

    observe(&mut app, tab, &nested.join("nested.txt"));

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(original.as_str())
    );
}

#[test]
fn cached_checkout_info_resolves_again_after_startup_or_git_identity_changes() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    let next_root = repo.path().join(".worktrees/next");
    std::fs::create_dir_all(next_root.join(".git")).unwrap();
    let startup_dir = repo.path().to_string_lossy().into_owned();
    let next_startup_dir = next_root.to_string_lossy().into_owned();
    let tab = Entity::from_bits(1);
    let calls = Cell::new(0);
    let first = vmux_git::worktree::CheckoutInfo {
        root: repo.path().canonicalize().unwrap(),
        common_dir: repo.path().join(".git").canonicalize().unwrap(),
    };
    let second = vmux_git::worktree::CheckoutInfo {
        root: next_root.canonicalize().unwrap(),
        common_dir: repo.path().join(".git").canonicalize().unwrap(),
    };
    let mut cache = HashMap::new();

    let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
        calls.set(calls.get() + 1);
        Some(first.clone())
    })
    .unwrap();
    assert_eq!(resolved, first);
    let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
        calls.set(calls.get() + 1);
        Some(second.clone())
    })
    .unwrap();
    assert_eq!(resolved, first);
    std::fs::rename(repo.path().join(".git"), repo.path().join(".git-old")).unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
        calls.set(calls.get() + 1);
        Some(first.clone())
    })
    .unwrap();
    assert_eq!(resolved, first);
    let resolved = cached_checkout_info(&mut cache, tab, &next_startup_dir, |_| {
        calls.set(calls.get() + 1);
        Some(second.clone())
    })
    .unwrap();

    assert_eq!(resolved, second);
    assert_eq!(calls.get(), 3);
}

#[test]
fn cached_checkout_info_resolves_again_after_commondir_changes() {
    let root = tempfile::tempdir().unwrap();
    let admin = tempfile::tempdir().unwrap();
    let first_common = tempfile::tempdir().unwrap();
    let second_common = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join(".git"),
        format!("gitdir: {}\n", admin.path().display()),
    )
    .unwrap();
    std::fs::write(
        admin.path().join("commondir"),
        first_common.path().to_string_lossy().as_bytes(),
    )
    .unwrap();
    let startup_dir = root.path().to_string_lossy().into_owned();
    let tab = Entity::from_bits(1);
    let calls = Cell::new(0);
    let first = vmux_git::worktree::CheckoutInfo {
        root: root.path().canonicalize().unwrap(),
        common_dir: first_common.path().canonicalize().unwrap(),
    };
    let second = vmux_git::worktree::CheckoutInfo {
        root: root.path().canonicalize().unwrap(),
        common_dir: second_common.path().canonicalize().unwrap(),
    };
    let mut cache = HashMap::new();

    let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
        calls.set(calls.get() + 1);
        Some(first.clone())
    })
    .unwrap();
    assert_eq!(resolved, first);
    let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
        calls.set(calls.get() + 1);
        Some(second.clone())
    })
    .unwrap();
    assert_eq!(resolved, first);
    assert_eq!(calls.get(), 1);
    std::fs::write(
        admin.path().join("commondir"),
        second_common.path().to_string_lossy().as_bytes(),
    )
    .unwrap();
    let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
        calls.set(calls.get() + 1);
        Some(second.clone())
    })
    .unwrap();

    assert_eq!(resolved, second);
    assert_eq!(calls.get(), 2);
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn observation_ignores_non_utf8_checkout_root() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let current = init_repo();
    let observed_parent = tempfile::tempdir().unwrap();
    let observed = observed_parent
        .path()
        .join(OsString::from_vec(b"repo-\xff".to_vec()));
    std::fs::create_dir(&observed).unwrap();
    git(&observed, &["init", "-q", "-b", "main"]);
    git(&observed, &["config", "user.email", "t@example.com"]);
    git(&observed, &["config", "user.name", "Test"]);
    git(&observed, &["config", "commit.gpgsign", "false"]);
    std::fs::write(observed.join("seed.txt"), "seed\n").unwrap();
    git(&observed, &["add", "seed.txt"]);
    git(&observed, &["commit", "-qm", "init"]);
    let original = current.path().to_string_lossy().into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(original.clone()),
        })
        .id();

    observe_edit(&mut app, tab, &observed.join("seed.txt"));

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(original.as_str())
    );
}

#[test]
fn observation_ignores_unrelated_and_invalid_paths() {
    let repo = init_repo();
    let other = init_repo();
    let non_git = tempfile::tempdir().unwrap();
    let non_git_file = non_git.path().join("file.txt");
    std::fs::write(&non_git_file, "x").unwrap();
    let missing = repo.path().join("missing.txt");
    let original = repo.path().to_string_lossy().into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(original.clone()),
        })
        .id();

    observe(&mut app, tab, &other.path().join("seed.txt"));
    observe(&mut app, tab, &non_git_file);
    observe(&mut app, tab, &missing);

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(original.as_str())
    );
}

#[test]
fn observation_rebinds_to_different_repo_on_edit() {
    let current = init_repo();
    let observed = init_repo();
    let expected = observed
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(current.path().to_string_lossy().into_owned()),
        })
        .id();

    observe_edit(&mut app, tab, &observed.path().join("seed.txt"));

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn observation_rebinds_from_non_git_directory_on_edit() {
    let current = tempfile::tempdir().unwrap();
    let observed = init_repo();
    let expected = observed
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(current.path().to_string_lossy().into_owned()),
        })
        .id();

    observe_edit(&mut app, tab, &observed.path().join("seed.txt"));

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn observation_keeps_non_git_directory_on_read() {
    let current = tempfile::tempdir().unwrap();
    let observed = init_repo();
    let original = current
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(original.clone()),
        })
        .id();

    observe(&mut app, tab, &observed.path().join("seed.txt"));

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(original.as_str())
    );
}

#[test]
fn relative_observation_is_ignored() {
    assert_eq!(observed_start_dir(Path::new(".")), None);
}

#[test]
fn observation_keeps_missing_current_directory_on_edit() {
    let current = tempfile::tempdir().unwrap();
    let original = current.path().to_string_lossy().into_owned();
    drop(current);
    let observed = init_repo();
    let mut app = App::new();
    app.add_plugins(WorktreePlugin);
    let tab = app
        .world_mut()
        .spawn(Tab {
            name: "tab".into(),
            startup_dir: Some(original.clone()),
        })
        .id();

    observe_edit(&mut app, tab, &observed.path().join("seed.txt"));

    assert_eq!(
        app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
        Some(original.as_str())
    );
}
