mod app;
mod changes;
mod directory;
mod outbox;
mod repository;
mod repository_picker;
mod status;
mod watch;

pub mod highlight;
pub mod job;
pub mod parse;
pub mod runner;
pub mod worktree;

use bevy::prelude::*;
use vmux_core::host::page::NativelyHosted;

pub use app::GitCheckForUpdatesRequest;
pub use changes::GitDiffSource;
pub use watch::RepoInfoCache;

use crate::host::app::AppPlugin;
use crate::host::changes::ChangesPlugin;
use crate::host::directory::DirectoryPlugin;
use crate::host::outbox::OutboxPlugin;
use crate::host::repository::RepositoryPlugin;
use crate::host::repository_picker::RepositoryPickerPlugin;
use crate::host::status::StatusPlugin;
use crate::host::watch::WatchPlugin;

pub struct GitPlugin;

impl Plugin for GitPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn((
            PAGE_MANIFEST,
            NativelyHosted::subtree(crate::GIT_PAGE_URL, "Git"),
        ));
        app.world_mut()
            .spawn(NativelyHosted::page(crate::GIT_DOCUMENT_URL, "Git"));
        vmux_core::register_host_spawn(app, "git");
        vmux_core::register_scheme_spawn(app, "git");
        app.configure_sets(
            Update,
            (
                GitUpdateSet::Watch,
                GitUpdateSet::Outbox,
                GitUpdateSet::Status,
            )
                .chain(),
        )
        .add_plugins((
            WatchPlugin,
            OutboxPlugin,
            StatusPlugin,
            AppPlugin,
            ChangesPlugin,
            DirectoryPlugin,
            RepositoryPlugin,
            RepositoryPickerPlugin,
        ));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GitUpdateSet {
    Watch,
    Outbox,
    Status,
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
