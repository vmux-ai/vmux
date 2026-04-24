pub(crate) mod tab;
mod focus_ring;
pub(crate) mod glass;
mod header;

pub(crate) mod window;
pub(crate) mod pane;
pub(crate) mod side_sheet;
pub(crate) mod space;
pub(crate) mod swap;
pub(crate) mod drag;

use bevy::prelude::*;
use focus_ring::FocusRingPlugin;
use header::HeaderLayoutPlugin;
use pane::PanePlugin;
use glass::GlassMaterialPlugin;
use side_sheet::SideSheetLayoutPlugin;
use space::SpacePlugin;
use tab::TabPlugin;
use vmux_webview_app::JsEmitUiReadyPlugin;
pub(crate) use window::fit_window_to_screen;
use window::WindowPlugin;

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
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
        ));
    }
}
