pub use agent::{
    AgentChooseWorkspace, AgentChooseWorkspaceAtPath, AgentCreateWorktree,
    AgentCreateWorktreeOnBranch, AgentListSpaces, AgentPrepareWorktree, AgentSpaceCreate,
    AgentSpaceDelete, AgentSpaceRename,
};
pub use cwd::WorkspaceCwd;
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
mod identity;
mod key;
mod plugin;
mod project;
mod restored;
mod snapshot_updater;
mod space_io;
mod spaces;
mod store;
mod tool;
mod workspace;
