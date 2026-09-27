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

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    url: vmux_api::space::SPACES_PAGE_URL,
    asset_host: "spaces",
    owns_subtree: false,
    title: "Spaces",
    title_message_id: Some("spaces-title"),
    replaces_command: None,
    keywords: &["space"],
    icon: Some(vmux_core::BuiltinIcon::Layers),
    command_bar: true,
};

pub use plugin::{SaveSpaceRequest, SpacePlugin};
pub use project::{ExpandedProjectDirs, SpaceProjects};
pub use spaces::{ActiveSpace, Spaces};
pub use tool::SpaceToolPlugin;
pub use vmux_api::space::{
    SpaceAttachRequest, SpaceCreateRequest, SpaceDeleteRequest, SpaceOpenPageRequest,
    SpaceRenameRequest,
};
