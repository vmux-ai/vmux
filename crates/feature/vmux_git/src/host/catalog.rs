use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::{GitCatalog, GitCatalogNextRequest};
use crate::state::{GitPageControllerState, GitPanel};

use super::GitUpdateSet;
use super::job::CatalogJob;
use super::job_runner::GitJob;
use super::repository::GitRepository;

const CATALOG_PAGE_SIZE: usize = 40;
const AUTO_COMMIT_LIMIT: usize = 400;

pub(super) struct CatalogPlugin;

#[derive(Component, Clone, PartialEq, Eq)]
struct CatalogContext {
    panel: GitPanel,
    branch_collection: crate::state::GitBranchCollection,
    selected_commit: String,
}

impl CatalogContext {
    fn from_controller(controller: &GitPageControllerState) -> Self {
        Self {
            panel: controller.focused_panel,
            branch_collection: controller.branch_collection,
            selected_commit: controller.selected_commit.clone(),
        }
    }
}

impl Plugin for CatalogPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(GitCatalogNextRequest,)>::default())
            .add_observer(load_next)
            .add_systems(Update, sync_catalogs.in_set(GitUpdateSet::Catalog));
    }
}

type ChangedCatalogPages<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut super::state::GitState,
        Ref<'static, GitPageControllerState>,
        Option<&'static CatalogContext>,
    ),
    Or<(
        Changed<super::state::GitState>,
        Changed<GitPageControllerState>,
    )>,
>;

fn sync_catalogs(mut pages: ChangedCatalogPages, mut commands: Commands) {
    for (entity, mut state, controller, context) in &mut pages {
        let (current_branch, repo_root, commit_offset, selected_commit_missing, has_staged) = {
            let Some(repository) = state.snapshot.repository.as_ref() else {
                continue;
            };
            (
                repository.branch.clone(),
                repository.repo_root.clone(),
                repository.commits.len(),
                !controller.selected_commit.is_empty()
                    && !repository
                        .commits
                        .iter()
                        .any(|entry| entry.sha == controller.selected_commit),
                repository.files.iter().any(|entry| entry.staged),
            )
        };
        let next_context = CatalogContext::from_controller(&controller);
        if context != Some(&next_context) {
            let mut retried = false;
            let mut retries = vec![GitCatalog::LocalBranches];
            match controller.focused_panel {
                GitPanel::Branches => retries.push(controller.branch_collection.into()),
                GitPanel::Commits => retries.push(GitCatalog::Commits),
                GitPanel::Stash => retries.push(GitCatalog::Stashes),
                GitPanel::Files => {}
            }
            retries.dedup();
            for catalog in retries {
                let progress = state.snapshot.catalog_mut(catalog);
                if progress.failed {
                    *progress = Default::default();
                    retried = true;
                }
            }
            if retried && !state.snapshot.has_failed_catalog() {
                state.snapshot.message.clear();
            }
            commands.entity(entity).insert(next_context);
        }
        let commits = state.snapshot.commits;
        if selected_commit_missing
            && controller.selected_commit_is_object_id()
            && commit_offset < AUTO_COMMIT_LIMIT
            && commits.loaded
            && commits.has_more
            && !commits.loading
            && !commits.failed
        {
            state.snapshot.commits.loading = true;
            commands.spawn((
                GitJob::new(entity),
                GitRepository::at(repo_root.clone()),
                CatalogJob {
                    catalog: GitCatalog::Commits,
                    current_branch: current_branch.clone(),
                    offset: commit_offset,
                    limit: CATALOG_PAGE_SIZE,
                    revision: state.snapshot.nonce,
                },
            ));
        }
        let mut catalogs = vec![GitCatalog::LocalBranches];
        if !controller.selected_commit.is_empty() || has_staged {
            catalogs.push(GitCatalog::Commits);
        }
        match controller.focused_panel {
            GitPanel::Branches => catalogs.push(controller.branch_collection.into()),
            GitPanel::Commits => catalogs.push(GitCatalog::Commits),
            GitPanel::Stash => catalogs.push(GitCatalog::Stashes),
            GitPanel::Files => {}
        }
        catalogs.dedup();
        for catalog in catalogs {
            let progress = state.snapshot.catalog(catalog);
            if progress.loaded || progress.loading || progress.failed {
                continue;
            }
            state.snapshot.catalog_mut(catalog).loading = true;
            commands.spawn((
                GitJob::new(entity),
                GitRepository::at(repo_root.clone()),
                CatalogJob {
                    catalog,
                    current_branch: current_branch.clone(),
                    offset: 0,
                    limit: CATALOG_PAGE_SIZE,
                    revision: state.snapshot.nonce,
                },
            ));
        }
    }
}

fn load_next(
    trigger: On<UiInput<GitCatalogNextRequest>>,
    mut pages: Query<&mut super::state::GitState>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let catalog = trigger.event().payload.catalog;
    let Ok(mut state) = pages.get_mut(webview) else {
        return;
    };
    let progress = state.snapshot.catalog(catalog);
    if !progress.loaded || progress.loading || !progress.has_more {
        return;
    }
    let Some(repository) = state.snapshot.repository.as_ref() else {
        return;
    };
    let current_branch = repository.branch.clone();
    let repo_root = repository.repo_root.clone();
    let offset = repository.catalog_len(catalog);
    state.snapshot.catalog_mut(catalog).loading = true;
    commands.spawn((
        GitJob::new(webview),
        GitRepository::at(repo_root),
        CatalogJob {
            catalog,
            current_branch,
            offset,
            limit: CATALOG_PAGE_SIZE,
            revision: state.snapshot.nonce,
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{GitCommitEntry, GitRepositorySnapshot};

    fn repository() -> GitRepositorySnapshot {
        GitRepositorySnapshot {
            path: "/tmp/repository".to_string(),
            repo_root: "/tmp/repository".to_string(),
            repo_name: "repository".to_string(),
            is_worktree: false,
            branch: "main".to_string(),
            base_ref: String::new(),
            upstream: String::new(),
            ahead: 0,
            behind: 0,
            files: Vec::new(),
            commits: Vec::new(),
            branches: Vec::new(),
            remote_branches: Vec::new(),
            tags: Vec::new(),
            stashes: Vec::new(),
        }
    }

    fn commit(index: usize) -> GitCommitEntry {
        let sha = format!("{index:040x}");
        GitCommitEntry {
            short_sha: sha[..7].to_string(),
            sha,
            author: String::new(),
            date: String::new(),
            summary: String::new(),
            body: String::new(),
            references: String::new(),
        }
    }

    #[test]
    fn commit_message_edits_do_not_retry_failed_catalogs() {
        let mut app = App::new();
        app.add_plugins(CatalogPlugin);
        let page = app
            .world_mut()
            .spawn((
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        repository: Some(repository()),
                        local_branches: crate::state::GitCatalogProgress {
                            loaded: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                },
                GitPageControllerState {
                    focused_panel: GitPanel::Files,
                    ..Default::default()
                },
            ))
            .id();
        app.update();

        app.world_mut()
            .get_mut::<super::super::state::GitState>(page)
            .unwrap()
            .snapshot = crate::state::GitPageSnapshot {
            repository: Some(repository()),
            local_branches: crate::state::GitCatalogProgress {
                loaded: true,
                ..Default::default()
            },
            commits: crate::state::GitCatalogProgress {
                failed: true,
                ..Default::default()
            },
            message: "catalog failed".to_string(),
            ..Default::default()
        };
        app.world_mut()
            .get_mut::<GitPageControllerState>(page)
            .unwrap()
            .commit_message = "message".to_string();
        app.update();

        assert!(
            app.world()
                .get::<super::super::state::GitState>(page)
                .unwrap()
                .snapshot
                .commits
                .failed
        );
        assert_eq!(
            app.world()
                .get::<super::super::state::GitState>(page)
                .unwrap()
                .snapshot
                .message,
            "catalog failed"
        );

        app.world_mut()
            .get_mut::<GitPageControllerState>(page)
            .unwrap()
            .focused_panel = GitPanel::Commits;
        app.update();

        let progress = app
            .world()
            .get::<super::super::state::GitState>(page)
            .unwrap()
            .snapshot
            .commits;
        assert!(!progress.failed);
        assert!(progress.loading);
        assert!(
            app.world()
                .get::<super::super::state::GitState>(page)
                .unwrap()
                .snapshot
                .message
                .is_empty()
        );
    }

    #[test]
    fn automatic_commit_lookup_stops_at_the_bounded_limit() {
        let mut app = App::new();
        app.add_plugins(CatalogPlugin);
        let mut repository = repository();
        repository.commits = (0..AUTO_COMMIT_LIMIT).map(commit).collect();
        let controller = GitPageControllerState {
            selected_commit: "f".repeat(40),
            focused_panel: GitPanel::Commits,
            ..Default::default()
        };
        let page = app
            .world_mut()
            .spawn((
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        repository: Some(repository),
                        local_branches: crate::state::GitCatalogProgress {
                            loaded: true,
                            ..Default::default()
                        },
                        commits: crate::state::GitCatalogProgress {
                            loaded: true,
                            has_more: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                },
                controller.clone(),
                CatalogContext::from_controller(&controller),
            ))
            .id();

        app.update();

        let state = app
            .world()
            .get::<super::super::state::GitState>(page)
            .unwrap();
        assert!(!state.snapshot.commits.loading);
        let mut jobs = app.world_mut().query::<&CatalogJob>();
        assert_eq!(jobs.iter(app.world()).count(), 0);
    }
}
