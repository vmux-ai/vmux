use bevy::prelude::*;

pub mod active;
pub mod active_panes;
pub mod apply;
pub mod archive;
pub mod bookmark;
pub mod cef;
pub mod contract;
pub mod native_open;
pub mod native_pointer;
pub mod overlay_adopt;
pub mod pane;
pub mod pending_stack;
pub mod placement;
pub mod plugin;
pub mod profile;
pub mod settings;
pub mod side_sheet;
pub mod snapshot;
pub mod space;
pub mod stack;
pub mod tab;
pub mod target;
pub mod toggle;
pub mod unit;
pub mod warm_page;
pub mod window;
pub mod workspace_snapshot;
pub mod workspace_snapshot_publish;
pub mod worktree;

mod header;
mod swap;
mod webview_reveal;

pub use cef::{
    Browser, LayoutCef, Loading, NavigationState, apply_cef_state_from_webview,
    mirror_metadata_to_url,
};
pub use contract::LayoutContractPlugin;
pub use header::Header;
pub use pane::{OpenBesideRequest, handle_open_beside_requests};
pub use plugin::LayoutPlugin;
pub use stack::{CloseStackReason, CloseStackRequest};
pub use vmux_core::ContributedCommandChosen;
pub use vmux_core::launcher::PendingLaunch;
pub use webview_reveal::PendingWebviewReveal;
pub use window::fit_window_to_screen;

pub const LAYOUT_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "layout",
    title: "Layout",
    title_message_id: None,
    replaces_command: None,
    keywords: &[],
    icon: None,
    command_bar: false,
};
pub const ERROR_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "error",
    title: "Error",
    title_message_id: None,
    replaces_command: None,
    keywords: &[],
    icon: None,
    command_bar: false,
};

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
pub struct CloseRequiresConfirmation;

#[derive(Resource, Default)]
pub struct SpaceFilePresent(pub bool);

#[derive(Resource, Default, Clone, PartialEq, Debug)]
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
pub enum LayoutSpawnRequest {
    Terminal { stack: Entity },
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

#[derive(Message, Clone)]
pub struct ExtensionInstallRequest {
    pub source: String,
}

#[cfg(test)]
mod tests {
    #[test]
    fn debug_manifest_and_url_are_consistent() {}
}
