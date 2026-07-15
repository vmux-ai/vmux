use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::event::{
    GIT_CHANGED_EVENT, GitChangedEvent, GitCommitRequest, GitDiffRequest, GitDiscardRequest,
    GitHunkRequest, GitPushRequest, GitStageRequest, GitStatusRequest, GitUnstageRequest,
};
use crate::job::{Emit, JobKind, emit_event_name, run_job};

pub type OutboxQueue = Vec<(Entity, Vec<Emit>)>;

#[derive(Resource, Clone, Default)]
pub struct GitOutbox(pub Arc<Mutex<OutboxQueue>>);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct GitWatchTarget {
    path: PathBuf,
    recursive: bool,
}

struct GitSubscription {
    path: PathBuf,
    targets: Vec<GitWatchTarget>,
    complete: bool,
}

struct GitWatch {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    watched: HashSet<GitWatchTarget>,
    subscriptions: HashMap<Entity, GitSubscription>,
}

fn canon(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf())
                .join(name),
            _ => path.to_path_buf(),
        })
}

fn resolve_git_path(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value.trim());
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    canon(&path)
}

fn git_watch_targets(file: &Path) -> Result<Vec<GitWatchTarget>, crate::runner::GitError> {
    let root = crate::runner::repo_root(file)?;
    let (stdout, stderr, ok) = crate::runner::git(
        &root,
        &["rev-parse", "--absolute-git-dir", "--git-common-dir"],
    )?;
    if !ok {
        return Err(crate::runner::git_err(&stdout, &stderr));
    }
    let mut lines = stdout.lines();
    let git_dir = lines
        .next()
        .map(|line| resolve_git_path(&root, line))
        .ok_or_else(|| crate::runner::GitError("missing git directory".into()))?;
    let common_dir = lines
        .next()
        .map(|line| resolve_git_path(&root, line))
        .ok_or_else(|| crate::runner::GitError("missing common git directory".into()))?;
    let mut targets = vec![GitWatchTarget {
        path: git_dir.clone(),
        recursive: false,
    }];
    if common_dir != git_dir {
        targets.push(GitWatchTarget {
            path: common_dir.clone(),
            recursive: false,
        });
    }
    targets.push(GitWatchTarget {
        path: common_dir.join("refs"),
        recursive: true,
    });
    Ok(targets)
}

fn target_matches(target: &GitWatchTarget, changed: &Path) -> bool {
    let changed = canon(changed);
    changed == target.path
        || if target.recursive {
            changed.starts_with(&target.path)
        } else {
            changed.parent() == Some(target.path.as_path())
        }
}

impl GitWatch {
    fn subscribe(&mut self, entity: Entity, path: &Path) {
        let path = canon(path);
        if self
            .subscriptions
            .get(&entity)
            .is_some_and(|subscription| subscription.path == path && subscription.complete)
        {
            return;
        }
        let Ok(targets) = git_watch_targets(&path) else {
            self.subscriptions.remove(&entity);
            return;
        };
        let mut complete = true;
        for target in &targets {
            if self.watched.contains(target) {
                continue;
            }
            let mode = if target.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if self.watcher.watch(&target.path, mode).is_ok() {
                self.watched.insert(target.clone());
            } else {
                complete = false;
            }
        }
        self.subscriptions.insert(
            entity,
            GitSubscription {
                path,
                targets,
                complete,
            },
        );
    }
}

/// Wires the git bridge: runs each git request on a background thread and drains completed
/// results back to the originating webview.
pub struct GitPlugin;

impl Plugin for GitPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = mpsc::channel();
        let proxy = app
            .world()
            .get_resource::<bevy::winit::EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            let wake = result
                .as_ref()
                .is_ok_and(|event| !matches!(event.kind, EventKind::Access(_)));
            let _ = tx.send(result);
            if wake && let Some(proxy) = proxy.as_ref() {
                let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        }) {
            Ok(watcher) => {
                app.insert_non_send(GitWatch {
                    watcher,
                    rx,
                    watched: HashSet::new(),
                    subscriptions: HashMap::new(),
                });
            }
            Err(error) => bevy::log::warn!("git watcher init failed: {error}"),
        }
        app.init_resource::<GitOutbox>()
            .add_plugins(BinEventEmitterPlugin::<(
                GitStatusRequest,
                GitDiffRequest,
                GitStageRequest,
                GitUnstageRequest,
                GitDiscardRequest,
                GitCommitRequest,
                GitPushRequest,
                GitHunkRequest,
            )>::default())
            .add_observer(on_status_request)
            .add_observer(on_diff_request)
            .add_observer(on_stage_request)
            .add_observer(on_unstage_request)
            .add_observer(on_discard_request)
            .add_observer(on_commit_request)
            .add_observer(on_push_request)
            .add_observer(on_hunk_request)
            .add_systems(Update, (drain_git_watch, drain_git_outbox));
    }
}

fn spawn_job(outbox: &GitOutbox, webview: Entity, job: JobKind) {
    let sink = outbox.0.clone();
    std::thread::spawn(move || {
        let emits = run_job(job);
        sink.lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((webview, emits));
    });
}

fn on_status_request(
    trigger: On<BinReceive<GitStatusRequest>>,
    sources: Query<&GitDiffSource>,
    outbox: Res<GitOutbox>,
    watch: Option<NonSendMut<GitWatch>>,
) {
    if let Some(mut watch) = watch {
        watch.subscribe(
            trigger.event().webview,
            Path::new(&trigger.event().payload.path),
        );
    }
    spawn_job(&outbox, trigger.event().webview, JobKind::Status {
        path: trigger.event().payload.path.clone().into(),
        dirty: sources
            .get(trigger.event().webview)
            .is_ok_and(|source| source.dirty),
    });
}

fn drain_git_watch(watch: Option<NonSendMut<GitWatch>>, mut commands: Commands) {
    let Some(watch) = watch else {
        return;
    };
    let mut changed = HashSet::new();
    while let Ok(result) = watch.rx.try_recv() {
        let Ok(event) = result else {
            continue;
        };
        if matches!(event.kind, EventKind::Access(_)) {
            continue;
        }
        changed.extend(event.paths);
    }
    if changed.is_empty() {
        return;
    }
    let affected: Vec<Entity> = watch
        .subscriptions
        .iter()
        .filter(|(_, subscription)| {
            subscription
                .targets
                .iter()
                .any(|target| changed.iter().any(|path| target_matches(target, path)))
        })
        .map(|(entity, _)| *entity)
        .collect();
    for entity in affected {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            GIT_CHANGED_EVENT,
            &GitChangedEvent {},
        ));
    }
}

fn on_diff_request(
    trigger: On<BinReceive<GitDiffRequest>>,
    sources: Query<&GitDiffSource>,
    outbox: Res<GitOutbox>,
) {
    let p = &trigger.event().payload;
    spawn_job(&outbox, trigger.event().webview, JobKind::Diff {
        path: p.path.clone().into(),
        top_line: p.top_line,
        rows: p.rows,
        content: sources
            .get(trigger.event().webview)
            .ok()
            .filter(|source| source.dirty)
            .map(|source| source.content.clone()),
    });
}

fn on_stage_request(trigger: On<BinReceive<GitStageRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Stage {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_unstage_request(trigger: On<BinReceive<GitUnstageRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Unstage {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_discard_request(trigger: On<BinReceive<GitDiscardRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Discard {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_commit_request(trigger: On<BinReceive<GitCommitRequest>>, outbox: Res<GitOutbox>) {
    let p = &trigger.event().payload;
    spawn_job(&outbox, trigger.event().webview, JobKind::Commit {
        path: p.path.clone().into(),
        message: p.message.clone(),
    });
}

fn on_push_request(trigger: On<BinReceive<GitPushRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Push {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_hunk_request(trigger: On<BinReceive<GitHunkRequest>>, outbox: Res<GitOutbox>) {
    let p = &trigger.event().payload;
    spawn_job(&outbox, trigger.event().webview, JobKind::Hunk {
        path: p.path.clone().into(),
        hunk: p.hunk,
        accept: p.accept,
    });
}

fn drain_git_outbox(outbox: Res<GitOutbox>, mut commands: Commands) {
    let drained: OutboxQueue = {
        let mut q = outbox.0.lock().unwrap_or_else(|p| p.into_inner());
        q.drain(..).collect()
    };
    for (webview, emits) in drained {
        for emit in emits {
            let name = emit_event_name(&emit);
            match emit {
                Emit::Status(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
                Emit::DiffMeta(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
                Emit::DiffViewport(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
                Emit::Result(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
                Emit::Error(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::GitErrorEvent;
    use crate::runner::test_repo;

    #[test]
    fn drain_empties_outbox() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<GitOutbox>()
            .add_systems(Update, drain_git_outbox);

        let webview = app.world_mut().spawn_empty().id();
        app.world().resource::<GitOutbox>().0.lock().unwrap().push((
            webview,
            vec![Emit::Error(GitErrorEvent { message: "boom".into() })],
        ));

        app.update();

        assert!(app.world().resource::<GitOutbox>().0.lock().unwrap().is_empty());
    }

    #[test]
    fn git_watch_targets_cover_index_and_refs() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        let targets = git_watch_targets(&file).unwrap();
        let git_dir = canon(&repo.path().join(".git"));

        assert!(targets.contains(&GitWatchTarget {
            path: git_dir.clone(),
            recursive: false,
        }));
        assert!(targets.contains(&GitWatchTarget {
            path: git_dir.join("refs"),
            recursive: true,
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

        let targets = git_watch_targets(&worktree.join("a.txt")).unwrap();
        let common = canon(&repo.path().join(".git"));

        assert!(targets.iter().any(|target| {
            !target.recursive
                && target.path != common
                && target.path.starts_with(common.join("worktrees"))
        }));
        assert!(targets.contains(&GitWatchTarget {
            path: common.clone(),
            recursive: false,
        }));
        assert!(targets.contains(&GitWatchTarget {
            path: common.join("refs"),
            recursive: true,
        }));
    }

    #[test]
    fn watch_target_matching_respects_recursion() {
        let root = canon(Path::new("/tmp/vmux-git-watch"));
        let direct = GitWatchTarget {
            path: root.clone(),
            recursive: false,
        };
        let recursive = GitWatchTarget {
            path: root.clone(),
            recursive: true,
        };

        assert!(target_matches(&direct, &root.join("index")));
        assert!(!target_matches(&direct, &root.join("refs/heads/main")));
        assert!(target_matches(&recursive, &root.join("refs/heads/main")));
    }
}
