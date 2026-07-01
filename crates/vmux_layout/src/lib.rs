//! The window and layout shell: spaces, tabs, panes, stacks, focus ring, header and
//! side-sheet, command-bar input, and the single CEF layout webview that composes every page.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod command_bar;
#[cfg(target_arch = "wasm32")]
pub mod debug_page;
#[cfg(target_arch = "wasm32")]
pub mod error_page;
pub mod event;
#[cfg(target_arch = "wasm32")]
pub mod extensions_page;
#[cfg(target_arch = "wasm32")]
pub mod page;
pub mod protocol;
pub mod reconcile;
#[cfg(not(target_arch = "wasm32"))]
pub mod snapshot;
pub mod start;

#[cfg(not(target_arch = "wasm32"))]
pub mod active_panes;
#[cfg(not(target_arch = "wasm32"))]
pub mod cef;
#[cfg(not(target_arch = "wasm32"))]
pub mod debug;
#[cfg(not(target_arch = "wasm32"))]
mod focus_ring;
#[cfg(not(target_arch = "wasm32"))]
mod header;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod profile;
#[cfg(not(target_arch = "wasm32"))]
pub mod scene;
#[cfg(not(target_arch = "wasm32"))]
pub mod settings;
#[cfg(not(target_arch = "wasm32"))]
pub mod stack;
#[cfg(not(target_arch = "wasm32"))]
pub mod unit;
#[cfg(not(target_arch = "wasm32"))]
mod webview_reveal;

#[cfg(not(target_arch = "wasm32"))]
pub mod active;
#[cfg(not(target_arch = "wasm32"))]
pub mod archive;
#[cfg(not(target_arch = "wasm32"))]
pub mod pane;
#[cfg(not(target_arch = "wasm32"))]
pub mod placement;
#[cfg(not(target_arch = "wasm32"))]
pub mod side_sheet;
#[cfg(not(target_arch = "wasm32"))]
pub mod space;
#[cfg(not(target_arch = "wasm32"))]
mod swap;
#[cfg(not(target_arch = "wasm32"))]
pub mod tab;
#[cfg(not(target_arch = "wasm32"))]
pub mod target;
#[cfg(not(target_arch = "wasm32"))]
pub mod toggle;
#[cfg(not(target_arch = "wasm32"))]
pub mod window;
#[cfg(not(target_arch = "wasm32"))]
pub mod worktree;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
pub use cef::{
    Browser, LayoutCef, Loading, NavigationState, apply_cef_state_from_webview,
    mirror_metadata_to_url,
};
#[cfg(not(target_arch = "wasm32"))]
pub use command_bar::handler::PendingCommandBarReveal;
#[cfg(not(target_arch = "wasm32"))]
pub use header::Header;
#[cfg(not(target_arch = "wasm32"))]
pub use pane::{OpenBesideRequest, handle_open_beside_requests};
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::LayoutPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use webview_reveal::PendingWebviewReveal;
#[cfg(not(target_arch = "wasm32"))]
pub use window::fit_window_to_screen;

#[cfg(not(target_arch = "wasm32"))]
pub const LAYOUT_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "layout",
    title: "Layout",
    keywords: &[],
    icon: None,
    command_bar: false,
};
#[cfg(not(target_arch = "wasm32"))]
pub const COMMAND_BAR_PAGE_MANIFEST: vmux_core::page::PageManifest =
    vmux_core::page::PageManifest {
        host: "command-bar",
        title: "Command Bar",
        keywords: &[],
        icon: None,
        command_bar: false,
    };
#[cfg(not(target_arch = "wasm32"))]
pub const DEBUG_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "debug",
    title: "Debug",
    keywords: &[],
    icon: None,
    command_bar: false,
};
#[cfg(not(target_arch = "wasm32"))]
pub const ERROR_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "error",
    title: "Error",
    keywords: &[],
    icon: None,
    command_bar: false,
};

#[cfg(not(target_arch = "wasm32"))]
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutStartupSet {
    Window,
    Persistence,
    DefaultTab,
    Post,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout"]
pub struct Open;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
pub struct NewStackContext {
    pub stack: Option<Entity>,
    pub previous_stack: Option<Entity>,
    pub needs_open: bool,
    pub dismiss_modal: bool,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
pub struct CloseRequiresConfirmation;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
pub struct SpaceFilePresent(pub bool);

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone, Debug)]
pub enum LayoutSpawnRequest {
    Terminal { stack: Entity },
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
pub enum TabLayoutSpawnContent {
    StartupUrlOrPrompt,
    Url(String),
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone, Debug)]
pub struct TabLayoutSpawnRequest {
    pub main: Entity,
    pub primary_window: Entity,
    pub name: Option<String>,
    pub startup_dir: Option<String>,
    pub content: TabLayoutSpawnContent,
    pub clear_pending_stack: bool,
    pub focus: bool,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone)]
pub struct BrowserNavigateRequest {
    pub url: String,
    pub pane: Option<String>,
    pub request_id: Option<[u8; 16]>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone)]
pub struct BrowserGoBackRequest {
    pub pane: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone)]
pub struct BrowserGoForwardRequest {
    pub pane: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone)]
pub struct OpenInNewStackRequest {
    pub url: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone)]
pub struct ExtensionInstallRequest {
    pub source: String,
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn debug_manifest_and_url_are_consistent() {
        assert_eq!(super::DEBUG_PAGE_MANIFEST.host, "debug");
        assert_eq!(crate::debug::DEBUG_PAGE_URL, "vmux://debug/");
    }
}
