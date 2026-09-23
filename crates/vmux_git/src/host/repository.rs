use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::{BinReceive, UiEventPlugin};

use crate::event::{GitBranchLogRequest, GitRepositoryRequest};

use super::job::JobKind;
use super::outbox::GitOutbox;
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
    trigger: On<BinReceive<GitRepositoryRequest>>,
    outbox: Res<GitOutbox>,
    watch: Option<NonSendMut<GitWatch>>,
    mut pages: Query<&mut vmux_core::PageMetadata>,
) {
    let webview = trigger.event().webview;
    let path: PathBuf = trigger.event().payload.path.clone().into();
    let repo_root = if let Some(mut watch) = watch {
        match watch.subscribe(webview, &path) {
            Ok(repo_root) => repo_root,
            Err(error) => {
                outbox.error(webview, error.0);
                return;
            }
        }
    } else {
        match super::runner::repo_root(&path) {
            Ok(repo_root) => repo_root,
            Err(error) => {
                outbox.error(webview, error.0);
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
    outbox.spawn(webview, JobKind::Repository { path });
}

fn on_branch_log_request(trigger: On<BinReceive<GitBranchLogRequest>>, outbox: Res<GitOutbox>) {
    let request = &trigger.event().payload;
    outbox.spawn(
        trigger.event().webview,
        JobKind::BranchLog {
            repo_root: Path::new(&request.repo_root).to_path_buf(),
            branch: request.branch.clone(),
        },
    );
}
