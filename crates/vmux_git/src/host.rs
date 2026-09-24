mod app;
mod changes;
mod diff;
mod directory;
mod job;
mod job_runner;
mod repository;
mod repository_picker;
mod state;
mod status;
mod watch;

mod highlight;
mod parse;
pub mod runner;
pub mod worktree;

use bevy::prelude::*;
use vmux_core::host::page::NativelyHosted;

pub use app::GitCheckForUpdatesRequest;
pub use diff::GitDiffSource;
pub use status::FileGit;
pub use watch::RepoInfoCache;

use crate::host::app::AppPlugin;
use crate::host::changes::ChangesPlugin;
use crate::host::diff::DiffPlugin;
use crate::host::directory::DirectoryPlugin;
use crate::host::job_runner::JobPlugin;
use crate::host::repository::RepositoryPlugin;
use crate::host::repository_picker::RepositoryPickerPlugin;
use crate::host::status::StatusPlugin;
use crate::host::watch::WatchPlugin;

pub struct GitPlugin;

impl Plugin for GitPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins((
            crate::ui::GitPage::plugin(),
            crate::ui::LegacyGitPage::plugin(),
        ));
        app.world_mut().spawn((
            PAGE_MANIFEST,
            NativelyHosted::subtree(crate::GIT_PAGE_URL, "Git"),
        ));
        app.world_mut()
            .spawn(NativelyHosted::page(crate::GIT_DOCUMENT_URL, "Git"));
        app.configure_sets(
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
            JobPlugin,
            AppPlugin,
            ChangesPlugin,
            DiffPlugin,
            DirectoryPlugin,
            RepositoryPlugin,
            RepositoryPickerPlugin,
        ));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GitUpdateSet {
    Watch,
    Status,
    Diff,
    Jobs,
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "git",
    title: "Git",
    title_message_id: Some("git-title"),
    replaces_command: None,
    keywords: &[
        "repository",
        "changes",
        "commit",
        "branch",
        "source control",
    ],
    icon: Some(vmux_core::BuiltinIcon::GitBranch),
    command_bar: true,
};
