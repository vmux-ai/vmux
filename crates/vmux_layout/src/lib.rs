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
pub mod chrome;
#[cfg(not(target_arch = "wasm32"))]
mod focus_ring;
#[cfg(not(target_arch = "wasm32"))]
pub mod glass;
#[cfg(not(target_arch = "wasm32"))]
mod header;
#[cfg(not(target_arch = "wasm32"))]
pub mod processes_monitor;
#[cfg(not(target_arch = "wasm32"))]
pub mod profile;
#[cfg(not(target_arch = "wasm32"))]
pub mod scene;
#[cfg(not(target_arch = "wasm32"))]
pub mod settings;
#[cfg(not(target_arch = "wasm32"))]
pub mod spaces;
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
pub mod toggle_layout;
#[cfg(not(target_arch = "wasm32"))]
pub mod window;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
pub use chrome::{
    Browser, LayoutChrome, LayoutChromePlugin, Loading, NavigationState,
    apply_chrome_state_from_cef,
};
#[cfg(not(target_arch = "wasm32"))]
use focus_ring::FocusRingPlugin;
#[cfg(not(target_arch = "wasm32"))]
use glass::GlassMaterialPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use header::Header;
#[cfg(not(target_arch = "wasm32"))]
use header::HeaderLayoutPlugin;
#[cfg(not(target_arch = "wasm32"))]
use pane::PanePlugin;
#[cfg(not(target_arch = "wasm32"))]
use side_sheet::SideSheetLayoutPlugin;
#[cfg(not(target_arch = "wasm32"))]
use space::SpacePlugin;
#[cfg(not(target_arch = "wasm32"))]
use stack::StackPlugin;
#[cfg(not(target_arch = "wasm32"))]
use toggle_layout::ToggleLayoutPlugin;
#[cfg(not(target_arch = "wasm32"))]
use vmux_webview_app::JsEmitUiReadyPlugin;
#[cfg(not(target_arch = "wasm32"))]
pub use webview_reveal::PendingWebviewReveal;
#[cfg(not(target_arch = "wasm32"))]
use webview_reveal::WebviewRevealPlugin;
#[cfg(not(target_arch = "wasm32"))]
use window::WindowPlugin;
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

#[cfg(not(target_arch = "wasm32"))]
pub struct LayoutPlugin;

#[cfg(not(target_arch = "wasm32"))]
impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Open>();
        app.init_resource::<NewStackContext>()
            .init_resource::<settings::ConfirmCloseSettings>()
            .add_message::<LayoutSpawnRequest>()
            .add_message::<BrowserNavigateRequest>()
            .add_message::<reconcile::LayoutApplyRequest>()
            .add_message::<reconcile::LayoutApplyResponse>()
            .add_message::<reconcile::LayoutSnapshotRequest>()
            .add_message::<reconcile::LayoutSnapshotResponse>()
            .configure_sets(
                Startup,
                (
                    LayoutStartupSet::Window,
                    LayoutStartupSet::Persistence,
                    LayoutStartupSet::DefaultSpace,
                    LayoutStartupSet::Post,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    reconcile::apply_layout_requests,
                    reconcile::serve_snapshot_requests,
                ),
            );
        app.add_plugins((
            JsEmitUiReadyPlugin,
            WindowPlugin,
            SpacePlugin,
            PanePlugin,
            StackPlugin,
            FocusRingPlugin,
            GlassMaterialPlugin,
            SideSheetLayoutPlugin,
            HeaderLayoutPlugin,
            ToggleLayoutPlugin,
            WebviewRevealPlugin,
        ));
    }
}

#[cfg(test)]
mod tests {}
