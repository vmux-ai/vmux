use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::{GitCatalog, GitCatalogNextRequest};
use crate::state::{GitCatalogPatch, GitPageControllerState, GitPanel, GitUiState};

use super::GitUpdateSet;
use super::job::{CatalogJob, GitForegroundRead};
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
        &'static mut super::state::GitCatalogRevision,
        Option<&'static CatalogContext>,
    ),
    Or<(
        Changed<super::state::GitState>,
        Changed<GitPageControllerState>,
        Changed<super::state::GitCatalogRevision>,
    )>,
>;

fn sync_catalogs(
    mut pages: ChangedCatalogPages,
    mut ui_state: vmux_ecs::UiStateWriter<GitUiState>,
    mut commands: Commands,
) {
    for (entity, mut state, controller, mut revision, context) in &mut pages {
        let mut patches = Vec::new();
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
                let progress = state.snapshot.catalog(catalog);
                if progress.failed {
                    let progress = crate::state::GitCatalogProgress::default();
                    *state
                        .bypass_change_detection()
                        .snapshot
                        .catalog_mut(catalog) = progress;
                    patches.push(GitCatalogPatch {
                        catalog,
                        offset: 0,
                        entries: None,
                        progress,
                        message: None,
                    });
                    retried = true;
                }
            }
            if retried && !state.snapshot.has_failed_catalog() {
                state.bypass_change_detection().snapshot.message.clear();
                if let Some(patch) = patches.last_mut() {
                    patch.message = Some(String::new());
                }
            }
            commands.entity(entity).insert(next_context);
        }
        let branch_files_pending = controller.focused_panel == GitPanel::Branches
            && !controller.selected_branch.is_empty()
            && state.snapshot.branch_files_error.is_empty()
            && state.snapshot.branch_files_reference != controller.selected_branch;
        if branch_files_pending {
            for patch in patches {
                ui_state.push(entity, patch);
            }
            continue;
        }
        let local_branches = state.snapshot.local_branches;
        if !local_branches.loaded && !local_branches.loading && !local_branches.failed {
            let progress = crate::state::GitCatalogProgress {
                loading: true,
                ..local_branches
            };
            state.bypass_change_detection().snapshot.local_branches = progress;
            patches.push(GitCatalogPatch {
                catalog: GitCatalog::LocalBranches,
                offset: 0,
                entries: None,
                progress,
                message: None,
            });
            commands.spawn((
                GitJob::new(entity),
                GitForegroundRead,
                GitRepository::at(repo_root.clone()),
                CatalogJob {
                    catalog: GitCatalog::LocalBranches,
                    current_branch: current_branch.clone(),
                    offset: 0,
                    limit: CATALOG_PAGE_SIZE,
                    revision: state.snapshot.nonce,
                    generation: revision.start(GitCatalog::LocalBranches),
                },
            ));
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
            let progress = crate::state::GitCatalogProgress {
                loading: true,
                ..commits
            };
            state.bypass_change_detection().snapshot.commits = progress;
            patches.push(GitCatalogPatch {
                catalog: GitCatalog::Commits,
                offset: u32::try_from(commit_offset).unwrap_or(u32::MAX),
                entries: None,
                progress,
                message: None,
            });
            commands.spawn((
                GitJob::new(entity),
                GitForegroundRead,
                GitRepository::at(repo_root.clone()),
                CatalogJob {
                    catalog: GitCatalog::Commits,
                    current_branch: current_branch.clone(),
                    offset: commit_offset,
                    limit: CATALOG_PAGE_SIZE,
                    revision: state.snapshot.nonce,
                    generation: revision.start(GitCatalog::Commits),
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
            let progress = crate::state::GitCatalogProgress {
                loading: true,
                ..progress
            };
            *state
                .bypass_change_detection()
                .snapshot
                .catalog_mut(catalog) = progress;
            patches.push(GitCatalogPatch {
                catalog,
                offset: 0,
                entries: None,
                progress,
                message: None,
            });
            commands.spawn((
                GitJob::new(entity),
                GitForegroundRead,
                GitRepository::at(repo_root.clone()),
                CatalogJob {
                    catalog,
                    current_branch: current_branch.clone(),
                    offset: 0,
                    limit: CATALOG_PAGE_SIZE,
                    revision: state.snapshot.nonce,
                    generation: revision.start(catalog),
                },
            ));
        }
        for patch in patches {
            ui_state.push(entity, patch);
        }
    }
}

fn load_next(
    trigger: On<UiInput<GitCatalogNextRequest>>,
    mut pages: Query<(
        &mut super::state::GitState,
        &mut super::state::GitCatalogRevision,
    )>,
    mut ui_state: vmux_ecs::UiStateWriter<GitUiState>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let catalog = trigger.event().payload.catalog;
    let Ok((mut state, mut revision)) = pages.get_mut(webview) else {
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
    let progress = crate::state::GitCatalogProgress {
        loading: true,
        ..progress
    };
    *state
        .bypass_change_detection()
        .snapshot
        .catalog_mut(catalog) = progress;
    ui_state.push(
        webview,
        GitCatalogPatch {
            catalog,
            offset: u32::try_from(offset).unwrap_or(u32::MAX),
            entries: None,
            progress,
            message: None,
        },
    );
    commands.spawn((
        GitJob::new(webview),
        GitRepository::at(repo_root),
        CatalogJob {
            catalog,
            current_branch,
            offset,
            limit: CATALOG_PAGE_SIZE,
            revision: state.snapshot.nonce,
            generation: revision.start(catalog),
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
        app.add_plugins((
            vmux_ecs::UiStatePlugin::<GitUiState>::default(),
            CatalogPlugin,
        ));
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
        app.add_plugins((
            vmux_ecs::UiStatePlugin::<GitUiState>::default(),
            CatalogPlugin,
        ));
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

    #[test]
    fn catalog_revision_continues_automatic_commit_lookup() {
        let mut app = App::new();
        app.add_plugins((
            vmux_ecs::UiStatePlugin::<GitUiState>::default(),
            CatalogPlugin,
        ));
        let mut repository = repository();
        repository.commits.push(commit(0));
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
                            loading: true,
                            has_more: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                },
                controller.clone(),
                CatalogContext::from_controller(&controller),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&CatalogJob>()
                .iter(app.world())
                .count(),
            0
        );

        app.world_mut()
            .get_mut::<super::super::state::GitState>(page)
            .unwrap()
            .bypass_change_detection()
            .snapshot
            .commits
            .loading = false;
        app.world_mut()
            .get_mut::<super::super::state::GitCatalogRevision>(page)
            .unwrap()
            .advance();
        app.update();

        let jobs = app
            .world_mut()
            .query::<&CatalogJob>()
            .iter(app.world())
            .map(|job| (job.catalog, job.offset))
            .collect::<Vec<_>>();
        assert_eq!(jobs, [(GitCatalog::Commits, 1)]);
    }

    #[test]
    fn branch_files_load_before_branch_catalogs() {
        let mut app = App::new();
        app.add_plugins((
            vmux_ecs::UiStatePlugin::<GitUiState>::default(),
            CatalogPlugin,
        ));
        let controller = GitPageControllerState {
            selected_branch: "feature".to_string(),
            focused_panel: GitPanel::Branches,
            ..Default::default()
        };
        let page = app
            .world_mut()
            .spawn((
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        repository: Some(repository()),
                        ..Default::default()
                    },
                },
                controller.clone(),
                CatalogContext::from_controller(&controller),
            ))
            .id();

        app.update();

        let mut jobs = app.world_mut().query::<&CatalogJob>();
        assert_eq!(jobs.iter(app.world()).count(), 0);

        let patch = crate::state::GitBranchFilesPatch {
            reference: "feature".to_string(),
            base_ref: Some("main".to_string()),
            base_revision: None,
            base_sha: None,
            head_sha: None,
            diff_paths: Some(Vec::new()),
            page: None,
            diff_loading: Some(false),
            loading: false,
            error: String::new(),
        };
        app.world_mut()
            .get_mut::<super::super::state::GitState>(page)
            .unwrap()
            .bypass_change_detection()
            .snapshot
            .apply_branch_files_patch(&patch);
        app.world_mut()
            .get_mut::<super::super::state::GitCatalogRevision>(page)
            .unwrap()
            .advance();
        app.update();

        let mut query = app
            .world_mut()
            .query_filtered::<&CatalogJob, With<GitForegroundRead>>();
        let jobs = query
            .iter(app.world())
            .map(|job| job.catalog)
            .collect::<Vec<_>>();
        assert_eq!(jobs, [GitCatalog::LocalBranches]);
    }
}
