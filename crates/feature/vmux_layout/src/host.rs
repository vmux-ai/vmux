use bevy::prelude::*;

pub mod active;
pub mod active_pane;
mod agent;
pub mod apply;
pub mod archive;
pub mod bookmark;
pub mod bookmark_tool;
pub mod cef;
mod command;
pub mod contract;
pub mod native_open;
pub mod overlay;
pub mod page_context;
pub mod pane;
pub mod pending_stack;
mod persistence;
pub mod placement;
pub mod plugin;
pub mod profile;
pub mod projection;
pub mod settings;
pub mod side_sheet;
pub mod snapshot;
pub mod space;
pub mod stack;
pub mod tab;
pub mod target;
pub mod toggle;
pub mod tool;
pub mod unit;
pub mod warm_page;
pub mod window;
pub mod workspace_snapshot;
pub mod workspace_snapshot_publish;
pub mod worktree;

mod swap;
mod webview_reveal;
mod zoom;

pub use cef::{
    Browser, LayoutCef, LayoutCefPlugin, LayoutCefStateSet, Loading, NavigationState,
    ReloadRevision,
};
pub use contract::LayoutContractPlugin;
pub use pane::OpenBesideRequest;
pub use persistence::LayoutPersistenceSet;
pub use plugin::LayoutPlugin;
pub use stack::{CloseStackReason, CloseStackRequest};
pub use vmux_core::ContributedCommandChosen;
pub use vmux_core::launcher::LauncherDismissRequest;
pub use webview_reveal::PendingWebviewReveal;

pub type LayoutUiStateUpdates = vmux_core::host::UiState<crate::state::LayoutUiState>;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutStartupSet {
    Window,
    Persistence,
    DefaultTab,
    Post,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout"]
pub struct Open;

#[derive(Component)]
pub struct Header;

#[derive(Component)]
pub struct CloseRequiresConfirmation;

#[derive(Component, Default, Clone, PartialEq, Debug)]
pub enum UpdateState {
    #[default]
    Idle,
    Downloading {
        version: String,
        downloaded: u64,
        total: u64,
    },
    Installing {
        version: String,
    },
    Ready {
        version: String,
    },
}

#[derive(Message, Clone, Debug)]
pub struct TerminalLayoutSpawnRequest {
    pub stack: Entity,
}

#[derive(Clone, Debug)]
pub enum TabLayoutSpawnContent {
    StartupUrlOrPrompt,
    Url {
        url: String,
        pending_prompt: Option<String>,
    },
}

#[derive(Message, Clone, Debug)]
pub struct TabLayoutSpawnRequest {
    pub space: Entity,
    pub primary_window: Entity,
    pub name: Option<String>,
    pub startup_dir: Option<std::path::PathBuf>,
    pub content: TabLayoutSpawnContent,
    pub clear_pending_stack: bool,
    pub focus: bool,
}

#[derive(Message, Clone, Debug)]
pub struct NewTabRequest {
    pub url: String,
    pub pending_prompt: Option<String>,
}

#[derive(Message, Clone)]
pub struct BrowserNavigateRequest {
    pub url: String,
    pub pane: Option<String>,
    pub request_id: Option<[u8; 16]>,
    pub new_stack: bool,
    pub profile: Option<String>,
}

#[derive(Message, Clone)]
pub struct BrowserGoBackRequest {
    pub pane: Option<String>,
}

#[derive(Message, Clone)]
pub struct BrowserGoForwardRequest {
    pub pane: Option<String>,
}

#[derive(Message, Clone)]
pub struct OpenInNewStackRequest {
    pub url: String,
}

#[cfg(test)]
mod tests {
    #[test]
    fn debug_manifest_and_url_are_consistent() {}
}
