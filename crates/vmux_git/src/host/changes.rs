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

use super::job::JobKind;
use super::job_runner::GitJobRequest;

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

fn on_stage_request(trigger: On<UiInput<GitStageRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path =
        super::runner::RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Stage { repo_root, path },
    });
}

fn on_unstage_request(trigger: On<UiInput<GitUnstageRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path =
        super::runner::RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Unstage { repo_root, path },
    });
}

fn on_discard_request(trigger: On<UiInput<GitDiscardRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path =
        super::runner::RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Discard { repo_root, path },
    });
}

fn on_commit_request(trigger: On<UiInput<GitCommitRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Commit {
            path: request.path.clone().into(),
            message: request.message.clone(),
        },
    });
}

fn on_fetch_request(
    trigger: On<UiInput<GitFetchRequest>>,
    mut views: Query<&mut super::state::GitState>,
    mut commands: Commands,
) {
    if let Ok(mut view) = views.get_mut(trigger.event().webview) {
        view.start_fetch();
    }
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Fetch {
            path: trigger.event().payload.path.clone().into(),
        },
    });
}

fn on_amend_request(trigger: On<UiInput<GitAmendRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_checkout_commit_request(
    trigger: On<UiInput<GitCheckoutCommitRequest>>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_cherry_pick_request(trigger: On<UiInput<GitCherryPickRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_create_branch_request(
    trigger: On<UiInput<GitCreateBranchRequest>>,
    mut controllers: Query<&mut super::controller::GitController>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    if let Ok(mut controller) = controllers.get_mut(trigger.event().webview) {
        controller.begin_branch_creation(request.branch.clone());
    }
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_delete_branch_request(
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
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_fast_forward_request(trigger: On<UiInput<GitFastForwardRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_merge_request(trigger: On<UiInput<GitMergeRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_rebase_request(trigger: On<UiInput<GitRebaseRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_revert_request(
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
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_stash_drop_request(
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
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_stash_pop_request(trigger: On<UiInput<GitStashPopRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_stash_push_request(trigger: On<UiInput<GitStashPushRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation(),
        },
    });
}

fn on_pull_request(trigger: On<UiInput<GitPullRequest>>, mut commands: Commands) {
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Pull {
            path: trigger.event().payload.path.clone().into(),
        },
    });
}

fn on_push_request(trigger: On<UiInput<GitPushRequest>>, mut commands: Commands) {
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Push {
            path: trigger.event().payload.path.clone().into(),
        },
    });
}

fn on_stage_all_request(trigger: On<UiInput<GitStageAllRequest>>, mut commands: Commands) {
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::StageAll {
            path: trigger.event().payload.path.clone().into(),
        },
    });
}

fn on_hunk_request(trigger: On<UiInput<GitHunkRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path =
        super::runner::RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    commands.trigger(GitJobRequest {
        webview: trigger.event().webview,
        job: JobKind::Hunk {
            repo_root,
            path,
            hunk: request.hunk,
            accept: request.accept,
        },
    });
}
