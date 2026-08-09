    use super::*;
    use crate::event::GitErrorEvent;
    use crate::runner::test_repo;

    #[test]
    fn drain_empties_outbox() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<GitOutbox>()
            .init_resource::<GitStatusJobs>()
            .add_systems(Update, drain_git_outbox);

        let webview = app.world_mut().spawn_empty().id();
        app.world()
            .resource::<GitOutbox>()
            .0
            .lock()
            .unwrap()
            .push(GitOutboxItem::Events {
                webview,
                emits: vec![Emit::Error(GitErrorEvent {
                    message: "boom".into(),
                })],
            });

        app.update();

        assert!(app.world().resource::<GitOutbox>().0.lock().unwrap().is_empty());
    }

    #[test]
    fn git_watch_targets_cover_index_and_refs() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        let (_, targets) = git_watch_targets(&file).unwrap();
        let git_dir = canon(&repo.path().join(".git"));

        assert!(targets.contains(&GitWatchTarget {
            path: git_dir.clone(),
            recursive: false,
            kind: GitWatchKind::Metadata,
        }));
        assert!(targets.contains(&GitWatchTarget {
            path: git_dir.join("refs"),
            recursive: true,
            kind: GitWatchKind::Metadata,
        }));
    }

    #[test]
    fn linked_worktree_targets_cover_private_and_common_git_dirs() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let parent = tempfile::tempdir().unwrap();
        let worktree = parent.path().join("linked");
        test_repo::run(
            repo.path(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "linked",
                worktree.to_str().unwrap(),
            ],
        );

        let (_, targets) = git_watch_targets(&worktree.join("a.txt")).unwrap();
        let common = canon(&repo.path().join(".git"));

        assert!(targets.iter().any(|target| {
            !target.recursive
                && target.path != common
                && target.path.starts_with(common.join("worktrees"))
        }));
        assert!(targets.contains(&GitWatchTarget {
            path: common.clone(),
            recursive: false,
            kind: GitWatchKind::Metadata,
        }));
        assert!(targets.contains(&GitWatchTarget {
            path: common.join("refs"),
            recursive: true,
            kind: GitWatchKind::Metadata,
        }));
    }

    #[test]
    fn watch_target_matching_respects_recursion() {
        let root = canon(Path::new("/tmp/vmux-git-watch"));
        let direct = GitWatchTarget {
            path: root.clone(),
            recursive: false,
            kind: GitWatchKind::Metadata,
        };
        let recursive = GitWatchTarget {
            path: root.clone(),
            recursive: true,
            kind: GitWatchKind::Metadata,
        };

        assert!(target_matches(&direct, &root.join("index")));
        assert!(!target_matches(&direct, &root.join("index.lock")));
        assert!(!target_matches(&direct, &root.join("refs/heads/main")));
        assert!(target_matches(&recursive, &root.join("refs/heads/main")));
        assert!(!target_matches(
            &recursive,
            &root.join("refs/heads/main.lock")
        ));

        let worktree = GitWatchTarget {
            path: root.clone(),
            recursive: true,
            kind: GitWatchKind::Worktree,
        };
        assert!(target_matches(&worktree, &root.join("Cargo.lock")));
        assert!(!target_matches(&worktree, &root.join(".git/index")));
    }

    #[test]
    fn repo_info_targets_cover_the_checkout_and_git_metadata() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let info = crate::worktree::repo_info(repo.path()).unwrap();
        let targets = repo_info_watch_targets(repo.path(), Some(&info));

        assert!(
            targets
                .iter()
                .any(|target| target_matches(target, &file))
        );
        assert!(
            targets
                .iter()
                .any(|target| target_matches(target, &info.git_dir.join("HEAD")))
        );
        assert!(
            targets
                .iter()
                .all(|target| !target_matches(target, &info.git_dir.join("index.lock")))
        );
    }

    #[test]
    fn repo_info_cache_refreshes_only_after_invalidation() {
        IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let path = canon(repo.path());
        let mut cache = RepoInfoCache {
            entries: HashMap::new(),
            wake: None,
        };
        let wait_for = |cache: &mut RepoInfoCache, expected| {
            for _ in 0..500 {
                if let Some(info) = cache.get(&path)
                    && info.uncommitted == expected
                {
                    return info;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("repo info did not reach uncommitted={expected}");
        };

        assert_eq!(wait_for(&mut cache, 0).uncommitted, 0);
        test_repo::write(repo.path(), "a.txt", "two\n");
        assert_eq!(cache.get(&path).unwrap().uncommitted, 0);
        cache.invalidate(&path);
        assert_eq!(wait_for(&mut cache, 1).uncommitted, 1);
    }

    #[test]
    fn repo_info_cache_keeps_changes_that_arrive_during_refresh() {
        IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let path = canon(repo.path());
        let stale = crate::worktree::repo_info(&path);
        test_repo::write(repo.path(), "a.txt", "two\n");
        let mut cache = RepoInfoCache {
            entries: HashMap::from([(
                path.clone(),
                RepoInfoCacheEntry {
                    info: None,
                    loaded: false,
                    dirty: false,
                    watched: false,
                    pending: Some(IoTaskPool::get().spawn(async move { stale })),
                    ignore_events_until: None,
                },
            )]),
            wake: None,
        };

        cache.invalidate(&path);
        for _ in 0..500 {
            if cache.get(&path).is_some_and(|info| info.uncommitted == 1) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("repo info stayed stale after an in-flight invalidation");
    }

    #[test]
    fn status_jobs_batch_by_repo_and_keep_one_batch_in_flight() {
        let root = PathBuf::from("/repo");
        let other_root = PathBuf::from("/other");
        let first = Entity::from_bits(1);
        let second = Entity::from_bits(2);
        let mut jobs = GitStatusJobs::default();

        jobs.queue(
            root.clone(),
            PendingStatusRequest {
                webview: first,
                path: root.join("a.txt"),
                dirty: false,
            },
        );
        jobs.queue(
            root.clone(),
            PendingStatusRequest {
                webview: first,
                path: root.join("a.txt"),
                dirty: true,
            },
        );
        jobs.queue(
            root.clone(),
            PendingStatusRequest {
                webview: second,
                path: root.join("b.txt"),
                dirty: false,
            },
        );
        jobs.queue(
            other_root.clone(),
            PendingStatusRequest {
                webview: first,
                path: other_root.join("c.txt"),
                dirty: false,
            },
        );

        let batches = jobs.take_ready();
        assert_eq!(batches.len(), 2);
        let (_, requests) = batches
            .iter()
            .find(|(batch_root, _)| batch_root == &root)
            .unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .any(|request| request.webview == first && request.dirty)
        );

        jobs.queue(
            root.clone(),
            PendingStatusRequest {
                webview: first,
                path: root.join("a.txt"),
                dirty: false,
            },
        );
        assert!(jobs.take_ready().is_empty());

        jobs.complete(&root);
        let batches = jobs.take_ready();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].0, root);
        assert_eq!(batches[0].1.len(), 1);
    }
