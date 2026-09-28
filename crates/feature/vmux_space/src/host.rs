mod agent;
mod composer;
pub mod cwd;
mod key;
pub mod plugin;
pub mod project;
pub mod snapshot_updater;
pub mod spaces;
mod tool;

type SpacesUiStateUpdates = vmux_core::host::UiState<vmux_api::space::SpacesUiState>;

pub use plugin::SpacePlugin;
pub use project::{ExpandedProjectDirs, SpaceProjects};
pub use spaces::Spaces;
pub use tool::SpaceToolPlugin;
pub use vmux_api::space::{
    SpaceAttachRequest, SpaceCreateRequest, SpaceDeleteRequest, SpaceOpenPageRequest,
    SpaceRenameRequest,
};
