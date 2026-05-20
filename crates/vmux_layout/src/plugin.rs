use bevy::prelude::*;
use bevy_cef::prelude::BinEventEmitterPlugin;
use vmux_server::{PAGE_READY_BIN_EVENT_ID, PageReady, mark_webview_page_ready_on_js_emit};

use crate::cef::LayoutCefPlugin;
use crate::command_bar::handler::CommandBarInputPlugin;
use crate::command_bar::plugin::CommandBarPagePlugin;
use crate::focus_ring::FocusRingPlugin;
use crate::glass::GlassMaterialPlugin;
use crate::header::HeaderLayoutPlugin;
use crate::pane::PanePlugin;
use crate::profile::ProfilePlugin;
use crate::scene::ScenePlugin;
use crate::side_sheet::SideSheetLayoutPlugin;
use crate::space::SpacePlugin;
use crate::stack::StackPlugin;
use crate::toggle::TogglePlugin;
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
        app.add_plugins(BinEventEmitterPlugin::<(PageReady,)>::with_id(
            PAGE_READY_BIN_EVENT_ID,
        ))
        .add_observer(mark_webview_page_ready_on_js_emit);
        app.add_plugins((
            ProfilePlugin,
            ScenePlugin,
            LayoutCefPlugin,
            WindowPlugin,
            SpacePlugin,
            PanePlugin,
            StackPlugin,
            FocusRingPlugin,
            GlassMaterialPlugin,
            SideSheetLayoutPlugin,
            HeaderLayoutPlugin,
        ))
        .add_plugins((
            CommandBarPagePlugin,
            CommandBarInputPlugin,
            TogglePlugin,
            WebviewRevealPlugin,
        ));
    }
}
