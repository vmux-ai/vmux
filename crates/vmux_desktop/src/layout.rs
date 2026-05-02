mod focus_ring;
mod footer;
pub(crate) mod glass;
mod header;
pub(crate) mod tab;
mod webview_reveal;

#[allow(dead_code)]
pub(crate) mod drag;
pub(crate) mod pane;
pub(crate) mod side_sheet;
pub(crate) mod space;
#[allow(dead_code)]
pub(crate) mod swap;
pub(crate) mod window;

use bevy::prelude::*;
use focus_ring::FocusRingPlugin;
use footer::FooterLayoutPlugin;
use glass::GlassMaterialPlugin;
use header::HeaderLayoutPlugin;
use moonshine_save::prelude::*;
use pane::PanePlugin;
use side_sheet::SideSheetLayoutPlugin;
use space::SpacePlugin;
use tab::TabPlugin;
use vmux_webview_app::JsEmitUiReadyPlugin;
pub(crate) use webview_reveal::PendingWebviewReveal;
use webview_reveal::WebviewRevealPlugin;
use window::WindowPlugin;
pub(crate) use window::fit_window_to_screen;

/// Marker component indicating a panel (header, side-sheet) is open.
/// Added/removed at runtime; persisted on state entities.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub(crate) struct Open;

/// Persisted entity that mirrors header open state.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct HeaderState;

/// Persisted entity that mirrors side-sheet open state.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[require(Save)]
pub(crate) struct SideSheetState;

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Open>()
            .register_type::<HeaderState>()
            .register_type::<SideSheetState>();
        app.add_plugins((
            JsEmitUiReadyPlugin,
            WindowPlugin,
            SpacePlugin,
            PanePlugin,
            TabPlugin,
            FocusRingPlugin,
            GlassMaterialPlugin,
            SideSheetLayoutPlugin,
            HeaderLayoutPlugin,
            FooterLayoutPlugin,
            WebviewRevealPlugin,
        ));
    }
}
