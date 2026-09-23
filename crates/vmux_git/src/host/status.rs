use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::{BinReceive, UiEventPlugin};

use crate::event::{FileStatus, GitErrorEvent, GitStatusRequest};

use super::GitDiffSource;
use super::GitUpdateSet;
use super::job::Emit;
use super::outbox::GitOutbox;
use super::watch::GitWatch;

pub(super) struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitStatusJobs>()
            .add_plugins(UiEventPlugin::<(GitStatusRequest,)>::default())
            .add_observer(on_status_request)
            .add_systems(Update, dispatch_status_jobs.in_set(GitUpdateSet::Status));
    }
}

#[derive(Clone, Debug)]
struct PendingStatusRequest {
    webview: Entity,
    path: PathBuf,
    dirty: bool,
}

#[derive(Resource, Default)]
pub(super) struct GitStatusJobs {
    pending: HashMap<PathBuf, HashMap<Entity, PendingStatusRequest>>,
    in_flight: HashSet<PathBuf>,
}

impl GitStatusJobs {
    fn queue(&mut self, repo_root: PathBuf, request: PendingStatusRequest) {
        self.pending
            .entry(repo_root)
            .or_default()
            .insert(request.webview, request);
    }

    fn take_ready(&mut self) -> Vec<(PathBuf, Vec<PendingStatusRequest>)> {
        let roots: Vec<PathBuf> = self
            .pending
            .keys()
            .filter(|root| !self.in_flight.contains(*root))
            .cloned()
            .collect();
        roots
            .into_iter()
            .filter_map(|root| {
                let requests = self.pending.remove(&root)?;
                self.in_flight.insert(root.clone());
                Some((root, requests.into_values().collect()))
            })
            .collect()
    }

    pub(super) fn complete(&mut self, repo_root: &Path) {
        self.in_flight.remove(repo_root);
    }
}

fn on_status_request(
    trigger: On<BinReceive<GitStatusRequest>>,
    sources: Query<&GitDiffSource>,
    outbox: Res<GitOutbox>,
    watch: Option<NonSendMut<GitWatch>>,
    mut jobs: ResMut<GitStatusJobs>,
) {
    let webview = trigger.event().webview;
    let path: PathBuf = trigger.event().payload.path.clone().into();
    if !super::runner::has_repository(&path) {
        outbox.events(
            webview,
            vec![Emit::Status(super::runner::non_repository_status(&path))],
        );
        return;
    }
    let repo_root = if let Some(mut watch) = watch {
        watch.subscribe(webview, &path)
    } else {
        super::runner::repo_root(&path)
    };
    match repo_root {
        Ok(repo_root) => jobs.queue(
            repo_root,
            PendingStatusRequest {
                webview,
                path,
                dirty: sources.get(webview).is_ok_and(|source| source.dirty),
            },
        ),
        Err(error) => outbox.error(webview, error.0),
    }
}

fn dispatch_status_jobs(mut jobs: ResMut<GitStatusJobs>, outbox: Res<GitOutbox>) {
    for (repo_root, requests) in jobs.take_ready() {
        let outbox = outbox.clone();
        std::thread::spawn(move || {
            let paths: Vec<PathBuf> = requests
                .iter()
                .map(|request| request.path.clone())
                .collect();
            let results = match super::runner::statuses(&repo_root, &paths) {
                Ok(events) => requests
                    .into_iter()
                    .zip(events)
                    .map(|(request, mut event)| {
                        if request.dirty {
                            event.file_status = match event.file_status {
                                FileStatus::Clean => FileStatus::Modified,
                                FileStatus::Staged => FileStatus::StagedModified,
                                status => status,
                            };
                        }
                        (request.webview, vec![Emit::Status(event)])
                    })
                    .collect(),
                Err(error) => requests
                    .into_iter()
                    .map(|request| {
                        (
                            request.webview,
                            vec![Emit::Error(GitErrorEvent {
                                message: error.0.clone(),
                            })],
                        )
                    })
                    .collect(),
            };
            outbox.status_batch(repo_root, results);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
