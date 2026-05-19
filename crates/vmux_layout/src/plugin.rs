use bevy::prelude::*;
use vmux_page::JsEmitUiReadyPlugin;

use crate::chrome::LayoutChromePlugin;
use crate::focus_ring::FocusRingPlugin;
use crate::glass::GlassMaterialPlugin;
use crate::header::HeaderLayoutPlugin;
use crate::pane::PanePlugin;
use crate::profile::ProfilePlugin;
use crate::scene::ScenePlugin;
use crate::side_sheet::SideSheetLayoutPlugin;
use crate::space::SpacePlugin;
use crate::stack::StackPlugin;
use crate::toggle_layout::ToggleLayoutPlugin;
use crate::webview_reveal::WebviewRevealPlugin;
use crate::window::WindowPlugin;
use crate::{
    BrowserNavigateRequest, LayoutSpawnRequest, LayoutStartupSet, NewStackContext, Open, reconcile,
    settings,
};

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Open>();
        app.init_resource::<NewStackContext>()
            .init_resource::<settings::ConfirmCloseSettings>()
            .add_message::<LayoutSpawnRequest>()
            .add_message::<vmux_core::agent::SpawnAgentInStackRequest>()
            .add_message::<vmux_core::agent::RestartAgentPty>()
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
            ProfilePlugin,
            ScenePlugin,
            LayoutChromePlugin,
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
