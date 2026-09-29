mod agent;
mod composer;
pub mod cwd;
mod key;
mod persistence;
pub mod plugin;
pub mod project;
pub mod snapshot_updater;
pub mod spaces;
mod tool;

type SpacesUiStateUpdates = vmux_core::host::UiState<vmux_api::space::SpacesUiState>;

pub use plugin::SpacePlugin;
pub use agent::{
    AgentChooseWorkspace, AgentChooseWorkspaceAtPath, AgentCreateWorktree,
    AgentCreateWorktreeOnBranch, AgentListSpaces, AgentPrepareWorktree, AgentRenameProfile,
    AgentSpaceCreate, AgentSpaceDelete, AgentSpaceRename,
};
pub use project::{ExpandedProjectDirs, SpaceProjects};
pub use spaces::Spaces;
pub use tool::SpaceToolPlugin;
pub use vmux_api::space::{
    SpaceAttachRequest, SpaceCreateRequest, SpaceDeleteRequest, SpaceOpenPageRequest,
    SpaceRenameRequest,
};
