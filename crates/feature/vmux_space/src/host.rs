pub use agent::{
    AgentChooseWorkspace, AgentChooseWorkspaceAtPath, AgentCreateWorktree,
    AgentCreateWorktreeOnBranch, AgentListSpaces, AgentPrepareWorktree, AgentRenameProfile,
    AgentSpaceCreate, AgentSpaceDelete, AgentSpaceRename,
};
pub use cwd::valid_cwd;
pub use plugin::SpacePlugin;
pub use project::{ExpandedProjectDirs, SpaceProjects};
pub use spaces::Spaces;
pub use tool::SpaceToolPlugin;
pub use vmux_api::space::{
    SpaceAttachRequest, SpaceCreateRequest, SpaceDeleteRequest, SpaceOpenPageRequest,
    SpaceRenameRequest,
};
pub use workspace::{PendingProject, RepositoryNeedsWorktree};

mod agent;
mod agent_workspace;
mod composer;
mod cwd;
mod key;
mod persistence;
mod plugin;
mod project;
mod snapshot_updater;
mod spaces;
mod tool;
mod workspace;

type SpacesUiStateUpdates = vmux_ecs::host::UiState<vmux_api::space::SpacesUiState>;
