pub(crate) mod tab;
mod focus_ring;
pub(crate) mod rounded;

pub(crate) mod window;
pub(crate) mod pane;
pub(crate) mod side_sheet;

use bevy::prelude::*;
use focus_ring::FocusRingPlugin;
use pane::PanePlugin;
use rounded::RoundedMaterialPlugin;
use side_sheet::SideSheetPlugin;
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
            PanePlugin,
            TabPlugin,
            FocusRingPlugin,
            RoundedMaterialPlugin,
            SideSheetPlugin,
        ));
    }
}
