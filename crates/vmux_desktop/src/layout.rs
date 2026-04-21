pub(crate) mod tab;
mod focus_ring;
pub(crate) mod glass;

pub(crate) mod window;
pub(crate) mod pane;
pub(crate) mod side_sheet;
pub(crate) mod space;

use bevy::prelude::*;
use focus_ring::FocusRingPlugin;
use pane::PanePlugin;
use glass::GlassMaterialPlugin;
use side_sheet::SideSheetPlugin;
use space::SpacePlugin;
use tab::TabPlugin;
use vmux_webview_app::JsEmitUiReadyPlugin;
use window::WindowPlugin;

pub(crate) use window::fit_window_to_screen;

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
            SideSheetPlugin,
        ));
    }
}
