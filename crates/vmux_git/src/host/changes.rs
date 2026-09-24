use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::{BinReceive, UiEventPlugin};

use crate::event::{
    GitCommitRequest, GitDiffRequest, GitDiscardRequest, GitFetchRequest, GitHunkRequest,
    GitOperationRequest, GitPullRequest, GitPushRequest, GitStageAllRequest, GitStageRequest,
    GitUnstageRequest,
};

use super::job::JobKind;
use super::job_runner::GitJob;

pub(super) struct ChangesPlugin;

impl Plugin for ChangesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            GitDiffRequest,
            GitStageRequest,
            GitUnstageRequest,
            GitDiscardRequest,
            GitCommitRequest,
            GitFetchRequest,
            GitOperationRequest,
            GitPullRequest,
            GitPushRequest,
            GitStageAllRequest,
            GitHunkRequest,
        )>::default())
            .add_observer(on_diff_request)
            .add_observer(on_stage_request)
            .add_observer(on_unstage_request)
            .add_observer(on_discard_request)
            .add_observer(on_commit_request)
            .add_observer(on_fetch_request)
            .add_observer(on_operation_request)
            .add_observer(on_pull_request)
            .add_observer(on_push_request)
            .add_observer(on_stage_all_request)
            .add_observer(on_hunk_request);
    }
}

#[derive(Component, Clone, Debug, Default)]
pub struct GitDiffSource {
    pub content: String,
    pub dirty: bool,
}

fn on_diff_request(
    trigger: On<BinReceive<GitDiffRequest>>,
    sources: Query<&GitDiffSource>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let repo_root = PathBuf::from(&request.repo_root);
    let path =
        super::runner::RequestPath::new(&request.path, &request.path_bytes).resolve(&repo_root);
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Diff {
            repo_root,
            path,
            reference: request.reference.clone(),
            generation: request.generation,
            top_line: request.top_line,
            rows: request.rows,
            content: sources
                .get(trigger.event().webview)
                .ok()
                .filter(|source| source.dirty)
                .map(|source| source.content.clone()),
        },
    );
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
    mut views: Query<&mut super::view::GitView>,
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

fn on_operation_request(trigger: On<BinReceive<GitOperationRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    GitJob::enqueue(
        &mut commands,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation.clone(),
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
