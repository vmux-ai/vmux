mod changes;
mod controller;
mod diff;
mod directory;
mod job;
mod job_runner;
mod repository_picker;
mod state;
mod status;
mod watch;

mod highlight;
mod parse;
mod repository;
pub mod worktree;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::host::manifest::FeaturePlugin;
use vmux_ecs::host::page::NativelyHosted;
use vmux_ecs::{PageOpenRequest, PageOpenTarget};

use crate::event::{GitConfigEditRequest, GitUpdateCheckRequest};
#[cfg(ui)]
use crate::ui::{GitPage, LegacyGitPage};

pub use diff::GitDiffSource;
pub use repository::{GitError, GitRepository};
pub use status::FileGit;
pub use watch::RepoInfoCache;

use crate::host::changes::ChangesPlugin;
use crate::host::controller::ControllerPlugin;
use crate::host::diff::DiffPlugin;
use crate::host::directory::DirectoryPlugin;
use crate::host::job_runner::JobPlugin;
use crate::host::repository_picker::RepositoryPickerPlugin;
use crate::host::status::StatusPlugin;
use crate::host::watch::WatchPlugin;

#[vmux_native::page]
pub struct GitPlugin;

impl Plugin for GitPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        #[cfg(ui)]
        app.add_plugins((GitPage::plugin(), LegacyGitPage::plugin()));
        app.add_message::<GitCheckForUpdatesRequest>()
            .add_plugins(UiEventPlugin::<(GitConfigEditRequest, GitUpdateCheckRequest)>::default())
            .add_observer(config_edit_request)
            .add_observer(update_check_request)
            .configure_sets(
                Update,
                (
                    GitUpdateSet::Watch,
                    GitUpdateSet::Status,
                    GitUpdateSet::Diff,
                    GitUpdateSet::Jobs,
                )
                    .chain(),
            )
            .add_plugins((
                WatchPlugin,
                StatusPlugin,
                state::StatePlugin,
                ControllerPlugin,
                JobPlugin,
                ChangesPlugin,
                DiffPlugin,
                DirectoryPlugin,
                RepositoryPickerPlugin,
            ))
            .add_plugins(
                Self::MANIFEST
                    .plugin()
                    .hosted(NativelyHosted::subtree(crate::GIT_PAGE_URL, "Git"))
                    .alias(NativelyHosted::page(Self::MANIFEST.url, "Git")),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GitUpdateSet {
    Watch,
    Status,
    Diff,
    Jobs,
}

#[derive(Message, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GitCheckForUpdatesRequest;

fn config_edit_request(
    trigger: On<UiInput<GitConfigEditRequest>>,
    child_of: Query<&ChildOf>,
    mut page_open: MessageWriter<PageOpenRequest>,
) {
    let target = child_of
        .get(trigger.event().webview)
        .ok()
        .and_then(|stack| child_of.get(stack.parent()).ok())
        .map(|pane| PageOpenTarget::NewStackInPane(pane.parent()))
        .unwrap_or(PageOpenTarget::ActiveStack);
    let Ok(path) = GitRepository::at(trigger.event().payload.repo_root.clone()).config_path()
    else {
        return;
    };
    let Ok(url) = url::Url::from_file_path(path) else {
        return;
    };
    page_open.write(PageOpenRequest {
        target,
        url: url.to_string(),
        request_id: None,
    });
}

fn update_check_request(
    _trigger: On<UiInput<GitUpdateCheckRequest>>,
    mut requests: MessageWriter<GitCheckForUpdatesRequest>,
) {
    requests.write(GitCheckForUpdatesRequest);
}
