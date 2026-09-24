use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::{BinReceive, UiEventPlugin};

use crate::event::{
    GitAmendRequest, GitCheckoutCommitRequest, GitCherryPickRequest, GitCommitRequest,
    GitCreateBranchRequest, GitDeleteBranchRequest, GitDiscardRequest, GitFastForwardRequest,
    GitFetchRequest, GitHunkRequest, GitMergeRequest, GitOperationRequests, GitPullRequest,
    GitPushRequest, GitRebaseRequest, GitRevertRequest, GitStageAllRequest, GitStageRequest,
    GitStashDropRequest, GitStashPopRequest, GitStashPushRequest, GitUnstageRequest,
};

use super::job::JobKind;
use super::job_runner::GitJob;

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
            .add_observer(on_stage_request)
            .add_observer(on_unstage_request)
            .add_observer(on_discard_request)
            .add_observer(on_commit_request)
            .add_observer(on_fetch_request)
            .add_observer(on_amend_request)
            .add_observer(on_checkout_commit_request)
            .add_observer(on_cherry_pick_request)
            .add_observer(on_create_branch_request)
            .add_observer(on_delete_branch_request)
            .add_observer(on_fast_forward_request)
            .add_observer(on_merge_request)
            .add_observer(on_rebase_request)
            .add_observer(on_revert_request)
            .add_observer(on_stash_drop_request)
            .add_observer(on_stash_pop_request)
            .add_observer(on_stash_push_request)
            .add_observer(on_pull_request)
            .add_observer(on_push_request)
            .add_observer(on_stage_all_request)
            .add_observer(on_hunk_request);
    }
}

fn on_stage_request(trigger: On<BinReceive<GitStageRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path =
        super::runner::RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Stage { repo_root, path },
    );
}

fn on_unstage_request(trigger: On<BinReceive<GitUnstageRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path =
        super::runner::RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Unstage { repo_root, path },
    );
}

fn on_discard_request(trigger: On<BinReceive<GitDiscardRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path =
        super::runner::RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Discard { repo_root, path },
    );
}

fn on_commit_request(trigger: On<BinReceive<GitCommitRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Commit {
            path: request.path.clone().into(),
            message: request.message.clone(),
        },
    );
}

fn on_fetch_request(
    trigger: On<BinReceive<GitFetchRequest>>,
    mut views: Query<&mut super::state::GitState>,
    mut commands: Commands,
) {
    if let Ok(mut view) = views.get_mut(trigger.event().webview) {
        view.start_fetch();
    }
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Fetch {
            path: trigger.event().payload.path.clone().into(),
        },
    );
}

fn on_amend_request(trigger: On<BinReceive<GitAmendRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_checkout_commit_request(
    trigger: On<BinReceive<GitCheckoutCommitRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_cherry_pick_request(
    trigger: On<BinReceive<GitCherryPickRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_create_branch_request(
    trigger: On<BinReceive<GitCreateBranchRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_delete_branch_request(
    trigger: On<BinReceive<GitDeleteBranchRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_fast_forward_request(
    trigger: On<BinReceive<GitFastForwardRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_merge_request(trigger: On<BinReceive<GitMergeRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_rebase_request(trigger: On<BinReceive<GitRebaseRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_revert_request(trigger: On<BinReceive<GitRevertRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_stash_drop_request(
    trigger: On<BinReceive<GitStashDropRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_stash_pop_request(
    trigger: On<BinReceive<GitStashPopRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_stash_push_request(
    trigger: On<BinReceive<GitStashPushRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    );
}

fn on_pull_request(trigger: On<BinReceive<GitPullRequest>>, mut commands: Commands) {
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Pull {
            path: trigger.event().payload.path.clone().into(),
        },
    );
}

fn on_push_request(trigger: On<BinReceive<GitPushRequest>>, mut commands: Commands) {
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Push {
            path: trigger.event().payload.path.clone().into(),
        },
    );
}

fn on_stage_all_request(trigger: On<BinReceive<GitStageAllRequest>>, mut commands: Commands) {
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::StageAll {
            path: trigger.event().payload.path.clone().into(),
        },
    );
}

fn on_hunk_request(trigger: On<BinReceive<GitHunkRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path =
        super::runner::RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Hunk {
            repo_root,
            path,
            hunk: request.hunk,
            accept: request.accept,
        },
    );
}
