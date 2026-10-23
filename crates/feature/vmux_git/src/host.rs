use crate::event::{
    GitBranchDraftRequest, GitBranchPromptCloseRequest, GitBranchPromptOpenRequest,
    GitBranchSubmitRequest, GitCommitDraftRequest, GitCommitSubmitRequest, GitConfigEditRequest,
    GitUpdateCheckRequest,
};
use crate::host::catalog::CatalogPlugin;
use crate::host::changes::ChangesPlugin;
use crate::host::controller::ControllerPlugin;
use crate::host::diff::DiffPlugin;
use crate::host::directory::DirectoryPlugin;
use crate::host::job_runner::JobPlugin;
use crate::host::repository_picker::RepositoryPickerPlugin;
use crate::host::repository_provision::RepositoryProvisionPlugin;
use crate::host::status::StatusPlugin;
use crate::host::watch::WatchPlugin;
#[cfg(ui)]
use crate::ui::{GitPage, LegacyGitPage};
use bevy::prelude::*;
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
pub use diff::GitDiffSource;
pub use repository::{GitError, GitRepository};
pub use repository_provision::{
    RepositoryProvisionCompleted, RepositoryProvisionOperation, RepositoryProvisionRequest,
};
pub use runtime::{GitRuntime, GitTask};
pub use status::FileGit;
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::page::{HostedPage, PrewarmPage};
use vmux_ecs::{PageOpenRequest, PageOpenTarget};
pub use watch::RepoInfoCache;

mod catalog;
mod changes;
mod controller;
mod diff;
mod directory;
mod job;
mod job_runner;
mod repository_picker;
mod repository_provision;
mod runtime;
mod state;
mod status;
mod watch;

mod highlight;
mod parse;
mod repository;
pub mod worktree;

#[vmux_page::page]
pub struct GitPlugin;

impl Plugin for GitPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .init_resource::<GitRuntime>();
        #[cfg(ui)]
        app.add_plugins((GitPage::plugin(), LegacyGitPage::plugin()));
        app.add_systems(PreStartup, prewarm)
            .add_message::<GitCheckForUpdatesRequest>()
            .add_plugins(UiEventPlugin::<(
                GitConfigEditRequest,
                GitUpdateCheckRequest,
                GitBranchPromptOpenRequest,
                GitBranchPromptCloseRequest,
                GitBranchDraftRequest,
                GitBranchSubmitRequest,
                GitCommitDraftRequest,
                GitCommitSubmitRequest,
            )>::default())
            .add_observer(config_edit_request)
            .add_observer(update_check_request)
            .add_systems(Update, poll_config_edits)
            .configure_sets(
                Update,
                (
                    GitUpdateSet::Watch,
                    GitUpdateSet::Status,
                    GitUpdateSet::Catalog,
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
                CatalogPlugin,
                DiffPlugin,
                DirectoryPlugin,
                RepositoryPickerPlugin,
                RepositoryProvisionPlugin,
            ))
            .add_plugins(
                Self::MANIFEST
                    .plugin()
                    .hosted(HostedPage::subtree(crate::GIT_PAGE_URL, "Git"))
                    .alias(HostedPage::page(Self::MANIFEST.url, "Git")),
            );
    }
}

fn prewarm(mut commands: Commands) {
    commands.spawn(PrewarmPage {
        host: "git",
        url: crate::GIT_PAGE_URL,
        title: "Git",
        pool_size: 1,
    });
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GitUpdateSet {
    Watch,
    Status,
    Catalog,
    Diff,
    Jobs,
}

#[derive(Message, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GitCheckForUpdatesRequest;

fn config_edit_request(
    trigger: On<UiInput<GitConfigEditRequest>>,
    child_of: Query<&ChildOf>,
    runtime: Res<GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let target = child_of
        .get(trigger.event().webview)
        .ok()
        .and_then(|stack| child_of.get(stack.parent()).ok())
        .map(|pane| PageOpenTarget::NewStackInPane(pane.parent()))
        .unwrap_or(PageOpenTarget::ActiveStack);
    let repository = GitRepository::at(trigger.event().payload.repo_root.clone());
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    let Some(task) = runtime.spawn(wake, move || repository.config_path()) else {
        return;
    };
    commands.spawn(GitConfigOpenTask { target, task });
}

#[derive(Component)]
struct GitConfigOpenTask {
    target: PageOpenTarget,
    task: GitTask<Result<std::path::PathBuf, GitError>>,
}

fn poll_config_edits(
    mut tasks: Query<(Entity, &mut GitConfigOpenTask)>,
    mut page_open: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    for (entity, mut open) in &mut tasks {
        let Some(result) = open.task.poll() else {
            continue;
        };
        commands.entity(entity).despawn();
        let Ok(Ok(path)) = result else {
            continue;
        };
        let Ok(url) = url::Url::from_file_path(path) else {
            continue;
        };
        page_open.write(PageOpenRequest {
            target: open.target.clone(),
            url: url.to_string(),
            request_id: None,
        });
    }
}

fn update_check_request(
    _trigger: On<UiInput<GitUpdateCheckRequest>>,
    mut requests: MessageWriter<GitCheckForUpdatesRequest>,
) {
    requests.write(GitCheckForUpdatesRequest);
}
