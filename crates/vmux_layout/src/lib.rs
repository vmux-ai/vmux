#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::new_ret_no_self
)]

pub mod event;
pub mod protocol;
pub mod reconcile;
#[cfg(not(target_arch = "wasm32"))]
pub mod snapshot;

#[cfg(not(target_arch = "wasm32"))]
pub mod cef;
#[cfg(not(target_arch = "wasm32"))]
mod focus_ring;
#[cfg(not(target_arch = "wasm32"))]
pub mod glass;
#[cfg(not(target_arch = "wasm32"))]
mod header;
#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub mod processes_monitor;
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

#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub mod drag;
#[cfg(not(target_arch = "wasm32"))]
pub mod pane;
#[cfg(not(target_arch = "wasm32"))]
pub mod side_sheet;
#[cfg(not(target_arch = "wasm32"))]
pub mod space;
#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
pub mod swap;
#[cfg(not(target_arch = "wasm32"))]
pub mod target;
#[cfg(not(target_arch = "wasm32"))]
pub mod toggle;
#[cfg(not(target_arch = "wasm32"))]
pub mod window;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
pub use cef::{Browser, LayoutCef, Loading, NavigationState, apply_chrome_state_from_cef};
#[cfg(not(target_arch = "wasm32"))]
pub use header::Header;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::LayoutPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use webview_reveal::PendingWebviewReveal;
#[cfg(not(target_arch = "wasm32"))]
pub use window::fit_window_to_screen;

#[cfg(not(target_arch = "wasm32"))]
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutStartupSet {
    Window,
    Persistence,
    DefaultSpace,
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
#[derive(Message, Clone)]
pub enum LayoutSpawnRequest {
    Terminal { stack: Entity },
    ProcessesMonitor { stack: Entity },
    OpenUrl { stack: Entity, url: String },
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone)]
pub struct BrowserNavigateRequest {
    pub url: String,
    pub pane: Option<String>,
}

#[cfg(test)]
mod tests {}
