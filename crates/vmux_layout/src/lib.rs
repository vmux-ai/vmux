//! The window and layout shell: spaces, tabs, panes, stacks, focus ring, header and
//! side-sheet, command-bar input, and the single CEF layout webview that composes every page.
#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

#[cfg(not(web))]
pub mod apply;
pub mod command_bar;
#[cfg(web)]
pub mod debug_page;
#[cfg(web)]
pub mod error_page;
pub mod event;
#[cfg(web)]
pub mod extensions_page;
#[cfg(web)]
pub mod page;
pub mod protocol;
pub mod reconcile;
#[cfg(not(web))]
pub mod snapshot;
pub mod start;
#[cfg(web)]
pub mod tools_page;
#[cfg(web)]
pub mod vault_page;

#[cfg(not(web))]
pub mod active_panes;
#[cfg(not(web))]
pub mod cef;
#[cfg(not(web))]
pub mod debug;
#[cfg(all(not(web), feature = "player-mode"))]
mod focus_ring;
#[cfg(not(web))]
mod header;
#[cfg(not(web))]
pub mod plugin;
#[cfg(not(web))]
pub mod profile;
#[cfg(all(not(web), feature = "player-mode"))]
pub mod scene;
#[cfg(all(not(web), not(feature = "player-mode")))]
#[path = "scene_user.rs"]
pub mod scene;
#[cfg(not(web))]
pub mod settings;
#[cfg(not(web))]
pub mod stack;
#[cfg(not(web))]
pub mod unit;
#[cfg(not(web))]
pub mod warm_page;
#[cfg(not(web))]
mod webview_reveal;

#[cfg(not(web))]
pub mod active;
#[cfg(not(web))]
pub mod archive;
#[cfg(not(web))]
pub mod bookmark;
#[cfg(not(web))]
pub mod native_pointer;
#[cfg(not(web))]
pub mod pane;
#[cfg(not(web))]
pub mod placement;
#[cfg(not(web))]
pub mod side_sheet;
#[cfg(not(web))]
pub mod space;
#[cfg(not(web))]
mod swap;
#[cfg(not(web))]
pub mod tab;
#[cfg(not(web))]
pub mod target;
#[cfg(not(web))]
pub mod toggle;
#[cfg(not(web))]
pub mod window;
#[cfg(not(web))]
pub mod worktree;

#[cfg(not(web))]
use bevy::prelude::*;
#[cfg(not(web))]
pub use cef::{
    Browser, LayoutCef, Loading, NavigationState, apply_cef_state_from_webview,
    mirror_metadata_to_url,
};
#[cfg(not(web))]
pub use command_bar::handler::PendingCommandBarReveal;
#[cfg(not(web))]
pub use header::Header;
#[cfg(not(web))]
pub use pane::{OpenBesideRequest, handle_open_beside_requests};
#[cfg(not(web))]
pub use plugin::LayoutPlugin;
#[cfg(not(web))]
pub use stack::CloseStackRequest;
#[cfg(not(web))]
pub use webview_reveal::PendingWebviewReveal;
#[cfg(not(web))]
pub use window::fit_window_to_screen;

#[cfg(not(web))]
pub const LAYOUT_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "layout",
    title: "Layout",
    keywords: &[],
    icon: None,
    command_bar: false,
};
#[cfg(not(web))]
pub const COMMAND_BAR_PAGE_MANIFEST: vmux_core::page::PageManifest =
    vmux_core::page::PageManifest {
        host: "command-bar",
        title: "Command Bar",
        keywords: &[],
        icon: None,
        command_bar: false,
    };
#[cfg(not(web))]
pub const DEBUG_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "debug",
    title: "Debug",
    keywords: &[],
    icon: None,
    command_bar: false,
};
#[cfg(not(web))]
pub const ERROR_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "error",
    title: "Error",
    keywords: &[],
    icon: None,
    command_bar: false,
};

#[cfg(not(web))]
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutStartupSet {
    Window,
    Persistence,
    DefaultTab,
    Post,
}

#[cfg(not(web))]
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout"]
pub struct Open;

#[cfg(not(web))]
#[derive(Resource, Default)]
pub struct NewStackContext {
    pub stack: Option<Entity>,
    pub previous_stack: Option<Entity>,
    pub needs_open: bool,
    pub dismiss_modal: bool,
}

#[cfg(not(web))]
#[derive(Component)]
pub struct CloseRequiresConfirmation;

#[cfg(not(web))]
#[derive(Resource, Default)]
pub struct SpaceFilePresent(pub bool);

#[cfg(not(web))]
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

#[cfg(not(web))]
#[derive(Message, Clone, Debug)]
pub enum LayoutSpawnRequest {
    Terminal { stack: Entity },
}

#[cfg(not(web))]
#[derive(Clone, Debug)]
pub enum TabLayoutSpawnContent {
    StartupUrlOrPrompt,
    Url {
        url: String,
        /// Left on the new stack as a [`vmux_core::PendingPrompt`] for the page to claim.
        pending_prompt: Option<String>,
    },
}

#[cfg(not(web))]
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

/// A command-bar row contributed through `CommandBarContributions` was chosen.
///
/// The command bar does not know what the row means — whoever published the id answers this.
/// `stack` is the empty stack the bar was opened on; when there is none, `pane` is the focused
/// pane to spawn into. Exactly one of the two is set.
#[cfg(not(web))]
#[derive(Message, Clone, Debug)]
pub struct ContributedCommandChosen {
    pub id: String,
    pub stack: Option<Entity>,
    pub pane: Option<Entity>,
}

/// Open `url` in a new focused tab in the active space.
///
/// Layout picks the space, the tab name and the working directory; the sender only says what to
/// show. `pending_prompt` rides along as a [`vmux_core::PendingPrompt`] on the new stack, for
/// whatever the URL opens to claim once it is ready.
#[cfg(not(web))]
#[derive(Message, Clone, Debug)]
pub struct NewTabRequest {
    pub url: String,
    pub pending_prompt: Option<String>,
}

#[cfg(not(web))]
#[derive(Message, Clone)]
pub struct BrowserNavigateRequest {
    pub url: String,
    pub pane: Option<String>,
    pub request_id: Option<[u8; 16]>,
    pub new_stack: bool,
    pub profile: Option<String>,
}

#[cfg(not(web))]
#[derive(Message, Clone)]
pub struct BrowserGoBackRequest {
    pub pane: Option<String>,
}

#[cfg(not(web))]
#[derive(Message, Clone)]
pub struct BrowserGoForwardRequest {
    pub pane: Option<String>,
}

#[cfg(not(web))]
#[derive(Message, Clone)]
pub struct OpenInNewStackRequest {
    pub url: String,
}

#[cfg(not(web))]
#[derive(Message, Clone)]
pub struct ExtensionInstallRequest {
    pub source: String,
}

#[cfg(test)]
#[path = "lib.test.rs"]
mod tests;
