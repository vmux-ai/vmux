use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::{GitBranchLogRequest, GitRepositoryRequest};

use super::job::{BranchLogJob, RepositoryJob};
use super::job_runner::{GitJob, GitJobFailure};
use super::watch::GitWatch;

pub(super) struct RepositoryPlugin;

impl Plugin for RepositoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(GitRepositoryRequest, GitBranchLogRequest)>::default())
            .add_observer(on_repository_request)
            .add_observer(on_branch_log_request);
    }
}

fn on_repository_request(
    trigger: On<UiInput<GitRepositoryRequest>>,
    watch: Option<NonSendMut<GitWatch>>,
    mut pages: Query<&mut vmux_core::PageMetadata>,
    mut views: Query<&mut super::state::GitState>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let path: PathBuf = trigger.event().payload.path.clone().into();
    if let Ok(mut view) = views.get_mut(webview) {
        view.start_repository(&path);
    }
    let repo_root = if let Some(mut watch) = watch {
        match watch.subscribe(webview, &path) {
            Ok(repo_root) => repo_root,
            Err(error) => {
                commands.trigger(GitJobFailure {
                    webview,
                    message: error.0,
                });
                return;
            }
        }
    } else {
        match super::runner::repo_root(&path) {
            Ok(repo_root) => repo_root,
            Err(error) => {
                commands.trigger(GitJobFailure {
                    webview,
                    message: error.0,
                });
                return;
            }
        }
    };
    if let Ok(mut page) = pages.get_mut(webview)
        && let Some(url) = crate::GitUrl::from_path(&repo_root)
        && page.url != url
    {
        page.url = url;
    }
    commands.spawn((GitJob::new(webview), RepositoryJob { path }));
}

fn on_branch_log_request(trigger: On<UiInput<GitBranchLogRequest>>, mut commands: Commands) {
    let request = &trigger.event().payload;
    commands.spawn((
        GitJob::new(trigger.event().webview),
        BranchLogJob {
            repo_root: Path::new(&request.repo_root).to_path_buf(),
            branch: request.branch.clone(),
        },
    ));
}
