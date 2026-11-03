use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_path::{PathIdentity, ScopedPath};

use crate::event::{
    GitAmendRequest, GitCheckoutCommitRequest, GitCherryPickRequest, GitCommitRequest,
    GitCreateBranchRequest, GitDeleteBranchRequest, GitDiscardRequest, GitFastForwardRequest,
    GitFetchRequest, GitHunkRequest, GitMergeRequest, GitOperationRequests, GitPullRequest,
    GitPushRequest, GitRebaseRequest, GitRevertRequest, GitStageAllRequest, GitStageRequest,
    GitStashDropRequest, GitStashPopRequest, GitStashPushRequest, GitUnstageRequest,
};
use crate::state::GitPageControllerState;

use super::job::{
    AmendJob, CheckoutCommitJob, CherryPickJob, CommitJob, CreateBranchJob, DeleteBranchJob,
    DiscardJob, FastForwardJob, FetchJob, HunkJob, MergeJob, PullJob, PushJob, RebaseJob,
    RevertJob, StageAllJob, StageJob, StashDropJob, StashPopJob, StashPushJob, UnstageJob,
};
use super::job_runner::GitJob;
use super::repository::{GitRepository, RequestPath};

#[derive(SystemParam)]
struct MutationAuthority<'w, 's> {
    pages: Query<
        'w,
        's,
        (
            &'static super::state::GitState,
            &'static GitPageControllerState,
        ),
    >,
    files: Query<'w, 's, &'static super::status::FileGit>,
}

#[derive(Clone, Copy)]
enum FileMutation {
    ToggleStage,
    Discard,
}

impl MutationAuthority<'_, '_> {
    fn root(&self, entity: Entity) -> Option<PathBuf> {
        if let Ok((state, _)) = self.pages.get(entity) {
            if state.snapshot.loading {
                return None;
            }
            return state
                .repository()
                .map(|repository| PathBuf::from(&repository.repo_root));
        }
        self.files.get(entity).ok()?.repo_root()
    }

    fn page(&self, entity: Entity) -> Option<(&super::state::GitState, &GitPageControllerState)> {
        let (state, controller) = self.pages.get(entity).ok()?;
        (!state.snapshot.loading).then_some((state, controller))
    }

    fn file(
        &self,
        entity: Entity,
        display: &str,
        bytes: &[u8],
        mutation: FileMutation,
    ) -> Option<(PathBuf, PathBuf)> {
        let root = self.root(entity)?;
        let requested = RequestPath::new(display, bytes).resolve(&root);
        let path = ScopedPath::resolve(&root, &requested).ok()?.into_path_buf();
        let identity = PathIdentity::resolve(&path);
        if let Ok(file) = self.files.get(entity) {
            return (PathIdentity::resolve(file.path()) == identity).then_some((root, path));
        }
        let (state, controller) = self.pages.get(entity).ok()?;
        let eligible = match mutation {
            FileMutation::ToggleStage => controller.operations.toggle_stage,
            FileMutation::Discard => controller.operations.discard,
        };
        if !eligible {
            return None;
        }
        let repository = state.repository()?;
        let selected = !controller.selected_abs_path.is_empty()
            && PathIdentity::resolve(&controller.selected_abs_path) == identity;
        let listed = repository.files.iter().any(|entry| {
            PathIdentity::resolve(RequestPath::new(&entry.path, &entry.path_bytes).resolve(&root))
                == identity
        });
        (selected || listed).then_some((root, path))
    }
}

pub(super) struct ChangesPlugin;

impl Plugin for ChangesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            GitStageRequest,
            GitUnstageRequest,
            GitDiscardRequest,
            GitCommitRequest,
            GitFetchRequest,
            GitPullRequest,
            GitPushRequest,
            GitStageAllRequest,
            GitHunkRequest,
        )>::default())
            .add_plugins(UiEventPlugin::<GitOperationRequests>::default())
            .add_observer(stage_request)
            .add_observer(unstage_request)
            .add_observer(discard_request)
            .add_observer(commit_request)
            .add_observer(fetch_request)
            .add_observer(amend_request)
            .add_observer(checkout_commit_request)
            .add_observer(cherry_pick_request)
            .add_observer(create_branch_request)
            .add_observer(delete_branch_request)
            .add_observer(fast_forward_request)
            .add_observer(merge_request)
            .add_observer(rebase_request)
            .add_observer(revert_request)
            .add_observer(stash_drop_request)
            .add_observer(stash_pop_request)
            .add_observer(stash_push_request)
            .add_observer(pull_request)
            .add_observer(push_request)
            .add_observer(stage_all_request)
            .add_observer(hunk_request);
    }
}

fn stage_request(
    trigger: On<UiInput<GitStageRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some((repo_root, path)) = authority.file(
        trigger.event().webview,
        &request.path,
        &request.path_bytes,
        FileMutation::ToggleStage,
    ) else {
        return;
    };
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(repo_root),
        StageJob { path },
    ));
}

fn unstage_request(
    trigger: On<UiInput<GitUnstageRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some((repo_root, path)) = authority.file(
        trigger.event().webview,
        &request.path,
        &request.path_bytes,
        FileMutation::ToggleStage,
    ) else {
        return;
    };
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(repo_root),
        UnstageJob { path },
    ));
}

fn discard_request(
    trigger: On<UiInput<GitDiscardRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some((repo_root, path)) = authority.file(
        trigger.event().webview,
        &request.path,
        &request.path_bytes,
        FileMutation::Discard,
    ) else {
        return;
    };
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(repo_root),
        DiscardJob { path },
    ));
}

fn commit_request(
    trigger: On<UiInput<GitCommitRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    let Some(path) = authority.root(entity) else {
        return;
    };
    if authority
        .page(entity)
        .is_some_and(|(_, controller)| !controller.operations.commit)
    {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(path.clone()),
        CommitJob {
            path,
            message: request.message.clone(),
        },
    ));
}

fn fetch_request(
    trigger: On<UiInput<GitFetchRequest>>,
    mut pages: Query<&mut super::state::GitState>,
    files: Query<&super::status::FileGit>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = if let Ok(mut state) = pages.get_mut(entity) {
        let Some(repository) = state.repository() else {
            return;
        };
        let path = PathBuf::from(&repository.repo_root);
        state.snapshot.fetching = true;
        path
    } else {
        let Some(path) = files
            .get(entity)
            .ok()
            .and_then(super::status::FileGit::repo_root)
        else {
            return;
        };
        path
    };
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(path.clone()),
        FetchJob { path },
    ));
}

fn amend_request(
    trigger: On<UiInput<GitAmendRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository().filter(|_| controller.operations.amend) else {
        return;
    };
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        AmendJob,
    ));
}

fn checkout_commit_request(
    trigger: On<UiInput<GitCheckoutCommitRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if !controller.operations.checkout_commit || controller.selected_commit != request.commit {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        CheckoutCommitJob {
            commit: request.commit.clone(),
        },
    ));
}

fn cherry_pick_request(
    trigger: On<UiInput<GitCherryPickRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if !controller.operations.cherry_pick || controller.selected_commit != request.commit {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        CherryPickJob {
            commit: request.commit.clone(),
        },
    ));
}

fn create_branch_request(
    trigger: On<UiInput<GitCreateBranchRequest>>,
    authority: MutationAuthority,
    mut pending: Query<&mut super::controller::PendingBranchCheckout>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    let base = match controller.branch_prompt.as_ref() {
        Some(crate::state::GitBranchPrompt::Create { base }) => base,
        _ => return,
    };
    if !controller.operations.create_branch || base != &request.start_point {
        return;
    }
    if let Ok(mut pending) = pending.get_mut(entity) {
        pending.0.clone_from(&request.branch);
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        CreateBranchJob {
            branch: request.branch.clone(),
            start_point: request.start_point.clone(),
        },
    ));
}

fn delete_branch_request(
    trigger: On<UiInput<GitDeleteBranchRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if controller.selected_branch != request.branch || !controller.operations.delete_branch {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        DeleteBranchJob {
            branch: request.branch.clone(),
        },
    ));
}

fn fast_forward_request(
    trigger: On<UiInput<GitFastForwardRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if !controller.operations.fast_forward || controller.selected_branch != request.branch {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        FastForwardJob {
            branch: request.branch.clone(),
        },
    ));
}

fn merge_request(
    trigger: On<UiInput<GitMergeRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if !controller.operations.merge || controller.selected_branch != request.branch {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        MergeJob {
            branch: request.branch.clone(),
        },
    ));
}

fn rebase_request(
    trigger: On<UiInput<GitRebaseRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if !controller.operations.rebase || controller.selected_branch != request.branch {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        RebaseJob {
            branch: request.branch.clone(),
        },
    ));
}

fn revert_request(
    trigger: On<UiInput<GitRevertRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if controller.selected_commit != request.commit || !controller.operations.revert_commit {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        RevertJob {
            commit: request.commit.clone(),
        },
    ));
}

fn stash_drop_request(
    trigger: On<UiInput<GitStashDropRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if controller.selected_stash != request.reference || !controller.operations.stash_drop {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        StashDropJob {
            reference: request.reference.clone(),
        },
    ));
}

fn stash_pop_request(
    trigger: On<UiInput<GitStashPopRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if !controller.operations.stash_pop || controller.selected_stash != request.reference {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        StashPopJob {
            reference: request.reference.clone(),
        },
    ));
}

fn stash_push_request(
    trigger: On<UiInput<GitStashPushRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Some((state, controller)) = authority.page(entity) else {
        return;
    };
    let Some(repository) = state.repository().filter(|_| controller.operations.stash) else {
        return;
    };
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(repository.repo_root.clone()),
        StashPushJob,
    ));
}

fn pull_request(
    trigger: On<UiInput<GitPullRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Some(path) = authority.root(entity) else {
        return;
    };
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(path.clone()),
        PullJob { path },
    ));
}

fn push_request(
    trigger: On<UiInput<GitPushRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Some(path) = authority.root(entity) else {
        return;
    };
    if authority
        .page(entity)
        .is_some_and(|(_, controller)| !controller.operations.push)
    {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(path.clone()),
        PushJob { path },
    ));
}

fn stage_all_request(
    trigger: On<UiInput<GitStageAllRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Some(path) = authority.root(entity) else {
        return;
    };
    if authority
        .page(entity)
        .is_some_and(|(_, controller)| !controller.operations.stage_all)
    {
        return;
    }
    commands.spawn((
        GitJob::new(entity),
        GitRepository::at(path.clone()),
        StageAllJob { path },
    ));
}

fn hunk_request(
    trigger: On<UiInput<GitHunkRequest>>,
    authority: MutationAuthority,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Some((repo_root, path)) = authority.file(
        trigger.event().webview,
        &request.path,
        &request.path_bytes,
        if request.accept {
            FileMutation::ToggleStage
        } else {
            FileMutation::Discard
        },
    ) else {
        return;
    };
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(repo_root),
        HunkJob {
            path,
            hunk: request.hunk,
            accept: request.accept,
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{FileStatus, GitFileEntry, GitRepositoryRequest, GitRepositorySnapshot};
    use crate::host::repository::test_repo;

    fn page(app: &mut App, root: &std::path::Path, file: &std::path::Path) -> Entity {
        let request = GitRepositoryRequest::at(root.to_string_lossy().into_owned());
        let mut repository = GitRepositorySnapshot::loading(&request);
        repository.repo_root = root.to_string_lossy().into_owned();
        repository.path.clone_from(&repository.repo_root);
        repository.files.push(GitFileEntry {
            path: file.to_string_lossy().into_owned(),
            path_bytes: Vec::new(),
            previous_path: None,
            previous_path_bytes: None,
            status: FileStatus::Modified,
            staged: false,
            unstaged: true,
        });
        app.world_mut()
            .spawn((
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        workspace: repository.repo_root.clone(),
                        repository: Some(repository),
                        loading: false,
                        ..Default::default()
                    },
                },
                GitPageControllerState {
                    selected_abs_path: file.to_string_lossy().into_owned(),
                    operations: crate::state::GitOperationEligibility {
                        toggle_stage: true,
                        discard: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ))
            .id()
    }

    #[test]
    fn file_mutation_uses_the_host_repository_root() {
        let repository = test_repo::init();
        let file = test_repo::write(repository.path(), "a.txt", "one\n");
        let root = repository.path().to_path_buf();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ChangesPlugin));
        let webview = page(&mut app, repository.path(), &file);

        app.world_mut().trigger(UiInput {
            webview,
            payload: GitStageRequest {
                repo_root: "/tmp/untrusted".to_string(),
                path: file.to_string_lossy().into_owned(),
                path_bytes: Vec::new(),
            },
        });
        app.update();

        let mut jobs = app
            .world_mut()
            .query_filtered::<(&GitRepository, &StageJob), With<GitJob>>();
        let (job_repository, job) = jobs.single(app.world()).unwrap();
        assert_eq!(
            PathIdentity::resolve(job_repository.path()),
            PathIdentity::resolve(root)
        );
        assert_eq!(
            PathIdentity::resolve(&job.path),
            PathIdentity::resolve(&file)
        );
    }

    #[test]
    fn file_mutation_rejects_paths_outside_the_host_repository() {
        let repository = test_repo::init();
        let file = test_repo::write(repository.path(), "a.txt", "one\n");
        let outside = tempfile::NamedTempFile::new().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ChangesPlugin));
        let webview = page(&mut app, repository.path(), &file);

        app.world_mut().trigger(UiInput {
            webview,
            payload: GitStageRequest {
                repo_root: repository.path().to_string_lossy().into_owned(),
                path: outside.path().to_string_lossy().into_owned(),
                path_bytes: Vec::new(),
            },
        });
        app.update();

        let mut jobs = app.world_mut().query_filtered::<Entity, With<StageJob>>();
        assert_eq!(jobs.iter(app.world()).count(), 0);
    }

    #[test]
    fn file_mutation_rejects_a_revalidating_page() {
        let repository = test_repo::init();
        let file = test_repo::write(repository.path(), "a.txt", "one\n");
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ChangesPlugin));
        let webview = page(&mut app, repository.path(), &file);
        app.world_mut()
            .get_mut::<super::super::state::GitState>(webview)
            .unwrap()
            .snapshot
            .loading = true;

        app.world_mut().trigger(UiInput {
            webview,
            payload: GitStageRequest {
                repo_root: repository.path().to_string_lossy().into_owned(),
                path: file.to_string_lossy().into_owned(),
                path_bytes: Vec::new(),
            },
        });
        app.update();

        let mut jobs = app.world_mut().query_filtered::<&StageJob, With<GitJob>>();
        assert_eq!(jobs.iter(app.world()).count(), 0);
    }
}
