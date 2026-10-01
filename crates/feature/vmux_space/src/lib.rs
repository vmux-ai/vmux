#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

#[cfg(host)]
pub use host::{
    AgentChooseWorkspace, AgentChooseWorkspaceAtPath, AgentCreateWorktree,
    AgentCreateWorktreeOnBranch, AgentListSpaces, AgentPrepareWorktree, AgentRenameProfile,
    AgentSpaceCreate, AgentSpaceDelete, AgentSpaceRename, ExpandedProjectDirs, PendingProject,
    RepositoryNeedsWorktree, SpaceAttachRequest, SpaceCreateRequest, SpaceDeleteRequest,
    SpaceOpenPageRequest, SpacePlugin, SpaceProjects, SpaceRenameRequest, SpaceToolPlugin, Spaces,
    valid_cwd,
};
pub use vmux_api::space as event;

#[cfg(host)]
pub(crate) struct Feature;

#[cfg(host)]
impl vmux_ecs::host::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

pub mod model;
#[cfg(ui)]
pub mod ui;

#[cfg(host)]
mod host;
