#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

#[cfg(host)]
pub use host::{
    ActiveProjectRows, AgentChooseWorkspace, AgentChooseWorkspaceAtPath, AgentCreateWorktree,
    AgentCreateWorktreeOnBranch, AgentListSpaces, AgentPrepareWorktree, AgentSpaceCreate,
    AgentSpaceDelete, AgentSpaceRename, ExpandedProjectDirs, PendingProject,
    RepositoryNeedsWorktree, SpaceAttachRequest, SpaceCreateRequest, SpaceDeleteRequest,
    SpaceOpenPageRequest, SpacePlugin, SpaceProjects, SpaceRenameRequest, SpaceToolPlugin, Spaces,
    WorkspaceCwd,
};
pub use vmux_api::space as event;

pub(crate) struct Feature;

impl vmux_feature::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod model;
#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
