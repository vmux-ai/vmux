//! Everything that only exists on a desktop: the window, the CEF webviews, and the Bevy
//! plugin that composes spaces, tabs, panes and stacks into them.
//!
//! One gate for the lot, rather than an attribute on each declaration. The crate's public paths
//! are unchanged: `lib.rs` re-exports this module's contents, so `vmux_layout::pane` still
//! resolves from outside and `crate::pane` still resolves from within.

use bevy::prelude::*;

pub mod active;
pub mod active_panes;
pub mod apply;
pub mod archive;
pub mod bookmark;
pub mod cef;
pub mod contract;
pub mod debug;
pub mod native_pointer;
pub mod pane;
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
pub mod worktree;

mod header;
mod swap;
mod webview_reveal;

pub mod scene;

pub use cef::{
    Browser, LayoutCef, Loading, NavigationState, apply_cef_state_from_webview,
    mirror_metadata_to_url,
};
pub use contract::LayoutContractPlugin;
pub use header::Header;
pub use pane::{OpenBesideRequest, handle_open_beside_requests};
pub use plugin::LayoutPlugin;
pub use stack::CloseStackRequest;
pub use webview_reveal::PendingWebviewReveal;
pub use window::fit_window_to_screen;

pub use crate::command_bar::handler::PendingCommandBarReveal;

pub const LAYOUT_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "layout",
    title: "Layout",
    title_message_id: None,
    replaces_command: None,
    keywords: &[],
    icon: None,
    command_bar: false,
};
pub const COMMAND_BAR_PAGE_MANIFEST: vmux_core::page::PageManifest =
    vmux_core::page::PageManifest {
        host: "command-bar",
        title: "Command Bar",
        title_message_id: None,
        replaces_command: None,
        keywords: &[],
        icon: None,
        command_bar: false,
    };
pub const DEBUG_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "debug",
    title: "Debug",
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

#[derive(Resource, Default)]
pub struct NewStackContext {
    pub stack: Option<Entity>,
    pub previous_stack: Option<Entity>,
    pub needs_open: bool,
    pub dismiss_modal: bool,
}

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
        /// Left on the new stack as a [`vmux_core::PendingPrompt`] for the page to claim.
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

/// A command-bar row contributed through `CommandBarContributions` was chosen.
///
/// The command bar does not know what the row means — whoever published the id answers this.
/// `stack` is the empty stack the bar was opened on; when there is none, `pane` is the focused
/// pane to spawn into. Exactly one of the two is set.
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
    fn debug_manifest_and_url_are_consistent() {
        assert_eq!(super::DEBUG_PAGE_MANIFEST.host, "debug");
        assert_eq!(crate::debug::DEBUG_PAGE_URL, "vmux://debug/");
    }
}
