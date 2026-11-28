use bevy::prelude::*;

pub use agent::{AgentOpenBeside, AgentPaneDirection};
pub use cef::{
    Browser, LayoutCef, LayoutCefPlugin, LayoutCefStateSet, Loading, NavigationState,
    ReloadRevision,
};
pub use contract::LayoutContractPlugin;
pub use pane::OpenBesideRequest;
pub use plugin::{ErrorPage, LayoutRuntimePlugin};
pub use stack::{CloseStackReason, CloseStackRequest};
pub use store::{LayoutRestoreCommitted, LayoutRestoreRequest, LayoutStoreSet};
pub use vmux_ecs::ContributedCommandChosen;
pub use vmux_ecs::launcher::LauncherDismissRequest;
pub use webview_reveal::PendingWebviewReveal;

pub mod active;
pub mod active_pane;
mod agent;
pub mod apply;
pub mod archive;
pub mod bookmark;
pub mod cef;
mod checkout;
mod command;
pub mod contract;
pub mod hosted_page;
pub mod overlay;
pub mod page_context;
pub mod page_slot;
pub mod pane;
pub mod placement;
pub mod plugin;
pub mod presentation;
pub mod profile;
pub mod projection;
pub mod settings;
pub mod side_sheet;
pub mod snapshot;
pub mod space;
pub mod stack;
mod store;
pub mod tab;
pub mod target;
pub mod toggle;
pub mod tool;
pub mod warm_page;
pub mod window;
#[cfg(any(target_os = "macos", test))]
mod window_buttons;
pub mod workspace_snapshot;
pub mod workspace_snapshot_publish;
pub mod worktree;
mod worktree_io;

mod swap;
mod webview_reveal;
mod zoom;

pub type LayoutUiStateUpdates = vmux_ecs::UiState<crate::state::LayoutUiState>;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutStartupSet {
    Window,
    Store,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpdateStage {
    #[default]
    Idle,
    Downloading,
    Installing,
    Ready,
}

#[derive(Component, Default, Clone, PartialEq, Eq, Debug)]
pub struct UpdateState {
    pub stage: UpdateStage,
    pub version: String,
    pub downloaded: u64,
    pub total: u64,
}

impl UpdateState {
    pub fn idle() -> Self {
        Self::default()
    }

    pub fn downloading(version: String, downloaded: u64, total: u64) -> Self {
        Self {
            stage: UpdateStage::Downloading,
            version,
            downloaded,
            total,
        }
    }

    pub fn installing(version: String) -> Self {
        Self {
            stage: UpdateStage::Installing,
            version,
            downloaded: 0,
            total: 0,
        }
    }

    pub fn ready(version: String) -> Self {
        Self {
            stage: UpdateStage::Ready,
            version,
            downloaded: 0,
            total: 0,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.stage == UpdateStage::Idle
    }

    pub fn is_ready(&self) -> bool {
        self.stage == UpdateStage::Ready
    }
}

#[derive(Message, Clone, Debug)]
pub struct TerminalLayoutSpawnRequest {
    pub stack: Entity,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabLayoutSpawnContent {
    pub url: Option<String>,
    pub pending_prompt: Option<String>,
}

impl TabLayoutSpawnContent {
    pub fn startup_url_or_prompt() -> Self {
        Self::default()
    }

    pub fn url(url: impl Into<String>, pending_prompt: Option<String>) -> Self {
        Self {
            url: Some(url.into()),
            pending_prompt,
        }
    }
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
