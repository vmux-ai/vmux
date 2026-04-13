mod tab;
mod outline;
pub(crate) mod rounded;

pub(crate) mod display;
pub(crate) mod pane;
pub(crate) mod side_sheet;

use bevy::prelude::*;
use display::DisplayPlugin;
use outline::OutlinePlugin;
use pane::PanePlugin;
use rounded::RoundedMaterialPlugin;
use side_sheet::SideSheetPlugin;
use tab::TabPlugin;
use vmux_webview_app::JsEmitUiReadyPlugin;

pub(crate) use display::fit_display_glass_to_window;

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JsEmitUiReadyPlugin,
            DisplayPlugin,
            PanePlugin,
            TabPlugin,
            OutlinePlugin,
            RoundedMaterialPlugin,
            SideSheetPlugin,
        ));
    }
}
