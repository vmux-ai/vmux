use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::{
    GitAmendRequest, GitCheckoutCommitRequest, GitCherryPickRequest, GitCommitRequest,
    GitCreateBranchRequest, GitDeleteBranchRequest, GitDiscardRequest, GitFastForwardRequest,
    GitFetchRequest, GitHunkRequest, GitMergeRequest, GitOperationRequests, GitPullRequest,
    GitPushRequest, GitRebaseRequest, GitRevertRequest, GitStageAllRequest, GitStageRequest,
    GitStashDropRequest, GitStashPopRequest, GitStashPushRequest, GitUnstageRequest,
};

use super::job::{
    AmendJob, CheckoutCommitJob, CherryPickJob, CommitJob, CreateBranchJob, DeleteBranchJob,
    DiscardJob, FastForwardJob, FetchJob, HunkJob, MergeJob, PullJob, PushJob, RebaseJob,
    RevertJob, StageAllJob, StageJob, StashDropJob, StashPopJob, StashPushJob, UnstageJob,
};
use super::job_runner::GitJob;
use super::repository::{GitRepository, RequestPath};

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

fn stage_request(trigger: On<UiInput<GitStageRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path = RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(repo_root),
        StageJob { path },
    ));
}

fn unstage_request(trigger: On<UiInput<GitUnstageRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path = RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(repo_root),
        UnstageJob { path },
    ));
}

fn discard_request(trigger: On<UiInput<GitDiscardRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path = RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(repo_root),
        DiscardJob { path },
    ));
}

fn commit_request(trigger: On<UiInput<GitCommitRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.spawn((
        GitJob::new(trigger.event().webview),
        CommitJob {
            path: request.path.clone().into(),
            message: request.message.clone(),
        },
    ));
}

fn fetch_request(
    trigger: On<UiInput<GitFetchRequest>>,
    mut views: Query<&mut super::state::GitState>,
    mut commands: Commands,
) {
    if let Ok(mut view) = views.get_mut(trigger.event().webview) {
        view.start_fetch();
    }
    commands.spawn((
        GitJob::new(trigger.event().webview),
        FetchJob {
            path: trigger.event().payload.path.clone().into(),
        },
    ));
}

fn amend_request(trigger: On<UiInput<GitAmendRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        AmendJob,
    ));
}

fn checkout_commit_request(trigger: On<UiInput<GitCheckoutCommitRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        CheckoutCommitJob {
            commit: request.commit.clone(),
        },
    ));
}

fn cherry_pick_request(trigger: On<UiInput<GitCherryPickRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        CherryPickJob {
            commit: request.commit.clone(),
        },
    ));
}

fn create_branch_request(
    trigger: On<UiInput<GitCreateBranchRequest>>,
    mut controllers: Query<&mut super::controller::GitController>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    if let Ok(mut controller) = controllers.get_mut(trigger.event().webview) {
        controller.begin_branch_creation(request.branch.clone());
    }
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        CreateBranchJob {
            branch: request.branch.clone(),
            start_point: request.start_point.clone(),
        },
    ));
}

fn delete_branch_request(
    trigger: On<UiInput<GitDeleteBranchRequest>>,
    pages: Query<(&super::state::GitState, &super::controller::GitController)>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Ok((state, controller)) = pages.get(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if repository.repo_root != request.repo_root
        || controller.state().selected_branch != request.branch
        || !controller.state().operations.delete_branch
    {
        return;
    }
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        DeleteBranchJob {
            branch: request.branch.clone(),
        },
    ));
}

fn fast_forward_request(trigger: On<UiInput<GitFastForwardRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        FastForwardJob {
            branch: request.branch.clone(),
        },
    ));
}

fn merge_request(trigger: On<UiInput<GitMergeRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        MergeJob {
            branch: request.branch.clone(),
        },
    ));
}

fn rebase_request(trigger: On<UiInput<GitRebaseRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        RebaseJob {
            branch: request.branch.clone(),
        },
    ));
}

fn revert_request(
    trigger: On<UiInput<GitRevertRequest>>,
    pages: Query<(&super::state::GitState, &super::controller::GitController)>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Ok((state, controller)) = pages.get(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if repository.repo_root != request.repo_root
        || controller.state().selected_commit != request.commit
        || !controller.state().operations.revert_commit
    {
        return;
    }
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        RevertJob {
            commit: request.commit.clone(),
        },
    ));
}

fn stash_drop_request(
    trigger: On<UiInput<GitStashDropRequest>>,
    pages: Query<(&super::state::GitState, &super::controller::GitController)>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let Ok((state, controller)) = pages.get(trigger.event().webview) else {
        return;
    };
    let Some(repository) = state.repository() else {
        return;
    };
    if repository.repo_root != request.repo_root
        || controller.state().selected_stash != request.reference
        || !controller.state().operations.stash_drop
    {
        return;
    }
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        StashDropJob {
            reference: request.reference.clone(),
        },
    ));
}

fn stash_pop_request(trigger: On<UiInput<GitStashPopRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        StashPopJob {
            reference: request.reference.clone(),
        },
    ));
}

fn stash_push_request(trigger: On<UiInput<GitStashPushRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitRepository::at(request.repo_root.clone()),
        StashPushJob,
    ));
}

fn pull_request(trigger: On<UiInput<GitPullRequest>>, mut commands: Commands) {
    commands.spawn((
        GitJob::new(trigger.event().webview),
        PullJob {
            path: trigger.event().payload.path.clone().into(),
        },
    ));
}

fn push_request(trigger: On<UiInput<GitPushRequest>>, mut commands: Commands) {
    commands.spawn((
        GitJob::new(trigger.event().webview),
        PushJob {
            path: trigger.event().payload.path.clone().into(),
        },
    ));
}

fn stage_all_request(trigger: On<UiInput<GitStageAllRequest>>, mut commands: Commands) {
    commands.spawn((
        GitJob::new(trigger.event().webview),
        StageAllJob {
            path: trigger.event().payload.path.clone().into(),
        },
    ));
}

fn hunk_request(trigger: On<UiInput<GitHunkRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path = RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
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
